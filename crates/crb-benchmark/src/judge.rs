use std::sync::Arc;

use crb_agents::prompts::PromptLibrary;
use crb_agents::{DEFAULT_TEMPERATURE, RuntimeProvider, stream_agent};
use crb_types::benchmark::golden::GoldenComment;
use crb_types::errors::ManyErrors;
use crb_types::finding::Finding;
use mti::prelude::{MagicTypeIdExt, V7};
use rig_core::completion::CompletionModel;

use crb_types::benchmark::judge::JudgeVerdict;
use serde_json::Map;
use tokio::task::JoinSet;
use tracing::warn;

fn create_judge_prompt(finding: &Finding, golden_comment: &GoldenComment) -> String {
    let mut ctx = Map::new();
    ctx.insert(
        "golden_comment".to_string(),
        serde_json::to_value(golden_comment).unwrap(),
    );
    ctx.insert(
        "finding".to_string(),
        serde_json::to_value(finding).unwrap(),
    );
    PromptLibrary::get_instance()
        .render_template("judge", ctx)
        .unwrap()
}

/// Run the Judge against a single finding and the set of golden comments.
///
/// If there are zero golden comments, the function will return early with an empty vector of verdicts and zero duration.
pub async fn judge_finding<R, A>(
    provider: Arc<R>,
    finding: &Finding,
    golden_comments: &[GoldenComment],
) -> (Vec<JudgeVerdict>, Option<ManyErrors>)
where
    R: RuntimeProvider<A> + Send + Sync + 'static,
    A: CompletionModel + Send + Sync + 'static,
{
    if golden_comments.is_empty() {
        return (Vec::new(), None);
    }

    let mut judge_set = JoinSet::new();
    for golden in golden_comments {
        let prompt = create_judge_prompt(finding, golden);
        let provider = provider.clone();
        judge_set.spawn(async move {
            let agent_id = "judge".create_type_id::<V7>();
            stream_agent::<JudgeVerdict, _, _>(provider, &agent_id, &prompt).await
        });
    }

    let mut errors = None;
    let mut verdicts = Vec::new();
    while let Some(res) = judge_set.join_next().await {
        match res {
            Ok(Ok(verdict)) => verdicts.push(verdict),
            Ok(Err(e)) => {
                let errors = errors.get_or_insert_with(ManyErrors::new);
                errors.push(e)
            }
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    (verdicts, errors)
}
