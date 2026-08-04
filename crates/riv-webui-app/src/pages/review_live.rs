use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_params_map;
#[cfg(target_arch = "wasm32")]
use riv_types::RunEvent;
#[cfg(target_arch = "wasm32")]
use riv_types::agent::AgentChunk;
use riv_types::review::{Review, ReviewMetadata, ReviewStatus};
use std::collections::HashMap;

use crate::LiveAgentInfo;
#[cfg(target_arch = "wasm32")]
use crate::components::format_elapsed;
use crate::components::{
    agent_pane::AgentPane,
    error_state::ErrorState,
    loading_state::{LoadingState, LoadingVariant},
    metrics_card::MetricsCard,
    metrics_grid::MetricsGrid,
    page_header::PageHeader,
    status_badge::StatusBadge,
};

#[cfg(target_arch = "wasm32")]
use {crate::sse, futures::StreamExt};

#[server]
async fn read_live_review_snapshot(review_id: String) -> Result<Review, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    let review_id = review_id
        .parse::<mti::prelude::MagicTypeId>()
        .map_err(|error| ServerFnError::new(format!("invalid review id: {error}")))?;

    (services.get_review)((review_id,))
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn read_live_review_agents(review_id: String) -> Result<Vec<LiveAgentInfo>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    let review_id = review_id
        .parse::<mti::prelude::MagicTypeId>()
        .map_err(|error| ServerFnError::new(format!("invalid review id: {error}")))?;

    (services.list_live_review_agents)((review_id,))
        .await
        .map_err(ServerFnError::new)
}

#[cfg(target_arch = "wasm32")]
async fn navigate_if_review_terminal(
    review_id: &str,
    navigate: &impl Fn(&str, leptos_router::NavigateOptions),
) -> bool {
    match read_live_review_snapshot(review_id.to_string()).await {
        Ok(review)
            if matches!(
                review.status,
                ReviewStatus::Completed | ReviewStatus::Failed | ReviewStatus::Cancelled
            ) =>
        {
            let path = format!("/reviews/{review_id}");
            navigate(&path, Default::default());
            true
        }
        Ok(_) => false,
        Err(error) => {
            log::warn!("Live SSE: failed to refresh review snapshot: {error}");
            false
        }
    }
}

#[derive(Clone, Default)]
struct LiveAgentState {
    response: String,
    status: ReviewStatus,
}

#[component]
pub fn ReviewLivePage() -> impl IntoView {
    let params = use_params_map();
    #[cfg(target_arch = "wasm32")]
    let navigate = use_navigate();
    let review_id = move || params.read().get("id").unwrap_or_default();

    let review = Resource::new(
        move || review_id(),
        |id| async move { read_live_review_snapshot(id).await },
    );
    let agents = Resource::new(
        move || review_id(),
        |id| async move { read_live_review_agents(id).await },
    );

    #[cfg(target_arch = "wasm32")]
    let (agent_states, set_agent_states) =
        signal::<HashMap<String, LiveAgentState>>(HashMap::new());
    #[cfg(not(target_arch = "wasm32"))]
    let (agent_states, _set_agent_states) =
        signal::<HashMap<String, LiveAgentState>>(HashMap::new());
    #[cfg(target_arch = "wasm32")]
    let (stream_status, set_stream_status) = signal::<ReviewStatus>(ReviewStatus::Pending);
    #[cfg(not(target_arch = "wasm32"))]
    let (stream_status, _set_stream_status) = signal::<ReviewStatus>(ReviewStatus::Pending);
    #[cfg(target_arch = "wasm32")]
    let (elapsed, set_elapsed) = signal(String::new());
    #[cfg(not(target_arch = "wasm32"))]
    let (elapsed, _set_elapsed) = signal(String::new());

    #[cfg(target_arch = "wasm32")]
    let id = review_id();

    #[cfg(target_arch = "wasm32")]
    leptos::task::spawn_local({
        let set_agent_states = set_agent_states;
        let set_stream_status = set_stream_status;
        let set_elapsed = set_elapsed;
        let navigate = navigate.clone();
        async move {
            if id.is_empty() {
                set_stream_status.update(|s| *s = ReviewStatus::Pending);
                return;
            }

            let url = format!("/api/reviews/{id}/stream");
            match sse::connect_sse(&url).await {
                Ok(mut rx) => {
                    set_stream_status.update(|s| *s = ReviewStatus::Running);

                    let start = std::time::Instant::now();
                    let mut last_elapsed_update = std::time::Instant::now();

                    while let Some(raw) = rx.next().await {
                        match serde_json::from_str::<RunEvent>(&raw) {
                            Ok(event) => match event {
                                RunEvent::AgentStarted { agent_id, .. } => {
                                    let agent_key = agent_id.to_string();
                                    set_agent_states.update(|states| {
                                        states.entry(agent_key).or_default().status =
                                            ReviewStatus::Running;
                                    });
                                }
                                RunEvent::AgentChunk { chunk, .. } => {
                                    let (agent_id, content) = match &chunk {
                                        AgentChunk::Output { id, content, .. } => {
                                            (id, content.as_str())
                                        }
                                        AgentChunk::Thinking { id, .. } => (id, ""),
                                        AgentChunk::Tool { id, .. } => (id, ""),
                                    };
                                    let agent_key = agent_id.to_string();
                                    if !content.is_empty() {
                                        set_agent_states.update(|states| {
                                            states
                                                .entry(agent_key)
                                                .or_default()
                                                .response
                                                .push_str(content);
                                        });
                                    }
                                }
                                RunEvent::AgentFinished { agent_id, .. } => {
                                    let agent_key = agent_id.to_string();
                                    set_agent_states.update(|states| {
                                        states.entry(agent_key).or_default().status =
                                            ReviewStatus::Completed;
                                    });
                                }
                                RunEvent::ReviewStarted { .. } => {
                                    set_stream_status.update(|s| *s = ReviewStatus::Running);
                                }
                                RunEvent::ReviewCompleted { .. } => {
                                    set_stream_status.update(|s| *s = ReviewStatus::Completed);
                                    if navigate_if_review_terminal(&id, &navigate).await {
                                        break;
                                    }
                                }
                                RunEvent::ReviewFailed { .. } => {
                                    set_stream_status.update(|s| *s = ReviewStatus::Failed);
                                    if navigate_if_review_terminal(&id, &navigate).await {
                                        break;
                                    }
                                }
                            },
                            Err(e) => {
                                log::warn!("Live SSE: failed to parse event: {e}");
                            }
                        }

                        if last_elapsed_update.elapsed().as_secs() >= 1 {
                            let secs = start.elapsed().as_secs_f64();
                            let elapsed_text = format_elapsed(secs);
                            set_elapsed.set(elapsed_text);
                            last_elapsed_update = std::time::Instant::now();
                        }
                    }

                    if !navigate_if_review_terminal(&id, &navigate).await {
                        match read_live_review_snapshot(id.clone()).await {
                            Ok(review) => {
                                set_stream_status.update(|status| *status = review.status);
                            }
                            Err(error) => {
                                log::warn!(
                                    "Live SSE: failed to refresh review snapshot after stream end: {error}"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Live SSE connection failed: {e}");
                    set_stream_status.update(|s| *s = ReviewStatus::Failed);
                }
            }
        }
    });

    view! {
        <Suspense fallback=move || view! {
            <LoadingState variant=LoadingVariant::SkeletonGrid {
                count: 4,
                grid_class: "content-grid--agent-panes",
                item_height: "200px",
            } />
        }>
            {move || {
                let review_result = review.get();
                let agents_result = agents.get();
                match (review_result, agents_result) {
                    (Some(Ok(review)), Some(Ok(agent_list))) => {
                        render_live_view(review, agent_list, agent_states, stream_status, elapsed)
                    }
                    (Some(Err(err)), _) | (_, Some(Err(err))) => view! {
                        <ErrorState
                            heading="Failed to load live review"
                            message=err.to_string()
                        />
                    }
                    .into_any(),
                    _ => view! {
                        <LoadingState variant=LoadingVariant::SkeletonGrid {
                            count: 4,
                            grid_class: "content-grid--agent-panes",
                            item_height: "200px",
                        } />
                    }
                    .into_any(),
                }
            }}
        </Suspense>
    }
}

fn render_live_view(
    review: Review,
    agent_list: Vec<LiveAgentInfo>,
    agent_states: ReadSignal<HashMap<String, LiveAgentState>>,
    stream_status: ReadSignal<ReviewStatus>,
    elapsed: ReadSignal<String>,
) -> AnyView {
    let title = format!("Live: {}", review_title(&review));
    let detail_path = format!("/reviews/{}", review.id);
    let status = move || stream_status.get();
    let duration = move || {
        let e = elapsed.get();
        if e.is_empty() {
            "In progress".to_string()
        } else {
            e
        }
    };
    let session_count = move || {
        agent_states
            .get()
            .values()
            .filter(|s| s.status == ReviewStatus::Completed)
            .count()
            .to_string()
    };

    view! {
        <div class="live-view-page">
            <PageHeader title=title>
                <StatusBadge status=status() />
                <a href=detail_path class="btn btn--ghost btn--sm">"Back to Detail"</a>
            </PageHeader>

            <MetricsGrid>
                <MetricsCard value=review.id.to_string() label="Review ID" truncate=true />
                <MetricsCard value=status().to_string() label="Status" />
                <MetricsCard value=session_count() label="Agents Complete" />
                <MetricsCard value=duration() label="Duration" />
            </MetricsGrid>

            <div class="content-grid content-grid--agent-panes">
                {move || {
                    let states = agent_states.get();
                    let agents = agent_list.clone();
                    agents.into_iter().map(|agent| {
                        let state = states.get(&agent.id.to_string()).cloned().unwrap_or_default();
                        let status_signal = Signal::derive(move || state.status.clone());
                        let response_signal = Signal::derive(move || {
                            if state.response.is_empty() {
                                None
                            } else {
                                Some(state.response.clone())
                            }
                        });
                        let pr_signal = Signal::derive(move || None::<String>);
                        view! {
                            <AgentPane
                                name=agent.name.clone()
                                status=status_signal
                                response=response_signal
                                current_pr=pr_signal
                            />
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>
        </div>
    }
    .into_any()
}

fn review_title(review: &Review) -> String {
    match &review.metadata {
        ReviewMetadata::PullRequest(pr) if !pr.meta.title.is_empty() => pr.meta.title.clone(),
        ReviewMetadata::PullRequest(pr) => {
            format!(
                "{}/{} #{}",
                pr.repository.owner, pr.repository.name, pr.meta.number
            )
        }
        ReviewMetadata::Commit(commit) => format!("Commit {}", short_hash(&commit.commit_hash)),
        ReviewMetadata::Plain => review.id.to_string(),
    }
}

fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
}
