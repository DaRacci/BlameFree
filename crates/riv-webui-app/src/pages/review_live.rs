use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use riv_types::review::{Review, ReviewMetadata};

use crate::components::{
    error_state::ErrorState,
    format_elapsed,
    loading_state::{LoadingState, LoadingVariant},
    log_viewer::LogViewer,
    metrics_card::MetricsCard,
    metrics_grid::MetricsGrid,
    page_header::PageHeader,
    status_badge::StatusBadge,
};

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

#[component]
pub fn ReviewLivePage() -> impl IntoView {
    let params = use_params_map();
    let review = Resource::new(
        move || params.read().get("id").unwrap_or_default(),
        |id| async move { read_live_review_snapshot(id).await },
    );

    view! {
        <Suspense fallback=move || view! {
            <LoadingState
                variant=LoadingVariant::SkeletonGrid {
                    count: 4,
                    grid_class: "content-grid--agent-panes",
                    item_height: "200px",
                }
            />
        }>
            {move || {
                review.get().map(|result| match result {
                    Ok(review) => render_live_snapshot(review),
                    Err(err) => view! {
                        <ErrorState
                            heading="Failed to load live review snapshot"
                            message=err.to_string()
                        />
                    }
                    .into_any(),
                })
            }}
        </Suspense>
    }
}

fn render_live_snapshot(review: Review) -> AnyView {
    let title = format!("Live: {}", review_title(&review));
    let detail_path = format!("/reviews/{}", review.id);
    let status = review.status.clone();
    let duration = review
        .duration
        .map(|duration| format_elapsed(duration.as_secs_f64()))
        .unwrap_or_else(|| "In progress".to_string());
    let sessions = review.agent_sessions.clone();

    view! {
        <div class="live-view-page">
            <PageHeader title=title>
                <StatusBadge status=status.clone() />
                <a href=detail_path class="btn btn--ghost btn--sm">"Back to Detail"</a>
            </PageHeader>

            <div class="card mb-lg">
                <div class="card__body">
                    <p class="text-secondary">
                        "Live snapshot now loads direct review detail. SSE stream route exists server-side, page wiring still follow-up."
                    </p>
                </div>
            </div>

            <MetricsGrid>
                <MetricsCard value=review.id.to_string() label="Review ID" />
                <MetricsCard value=review.status.to_string() label="Status" />
                <MetricsCard value=review.agent_sessions.len().to_string() label="Agent Sessions" />
                <MetricsCard value=duration label="Duration" />
            </MetricsGrid>

            <LogViewer agent_sessions=sessions />
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
