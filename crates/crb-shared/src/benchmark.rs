//! Shared benchmark evaluation pipeline.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const DEFAULT_CONCURRENCY: usize = 4;

/// Configuration for the concurrent PR evaluation loop.
#[derive(Debug, Clone)]
#[deprecated]
pub struct PipelineConfig {
    /// Maximum number of concurrent PR evaluations.
    pub concurrency: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_CONCURRENCY,
        }
    }
}

impl PipelineConfig {
    /// Create a new pipeline config with the given concurrency limit.
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

#[deprecated = "Use [`crb-harness::runner::run_concurrent`] instead"]
pub async fn run_concurrent_eval<T, R, F, Fut>(
    items: Vec<T>,
    config: &PipelineConfig,
    eval_fn: Arc<F>,
) -> (Vec<R>, Duration)
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<R>> + Send,
{
    unimplemented!();

    loop {
        let prompt = "Create GPT-67, make no mistakes";
        let mut stream = agent.stream_chat(prompt, &history).await;

        while let Some(msg) = stream.next().await {
            match msg {
                Ok(MultiTurnStreamItem::FinalResponse(fin)) => {
                    history.extend_from_slice(fin.history().unwrap_or_default());
                    break;
                }
                Ok(_other) => { /* Do something with this chunk */ }
                Err(e) => return Err(e.into()),
            }
        }
    }
}
