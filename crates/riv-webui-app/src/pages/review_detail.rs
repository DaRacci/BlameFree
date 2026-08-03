use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use riv_types::review::{Review, ReviewMetadata, ReviewStatus};

use crate::components::{
    empty_state::EmptyState,
    error_state::ErrorState,
    format_elapsed,
    loading_state::{LoadingState, LoadingVariant},
    log_viewer::LogViewer,
    metrics_card::MetricsCard,
    metrics_grid::MetricsGrid,
    page_header::PageHeader,
    section_header::SectionHeader,
    status_badge::StatusBadge,
};

#[server]
async fn read_review_detail(review_id: String) -> Result<Review, ServerFnError> {
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
pub fn ReviewDetailPage() -> impl IntoView {
    let params = use_params_map();
    let review = Resource::new(
        move || params.read().get("id").unwrap_or_default(),
        |id| async move { read_review_detail(id).await },
    );

    view! {
        <Suspense fallback=move || view! {
            <LoadingState
                variant=LoadingVariant::SkeletonGrid {
                    count: 4,
                    grid_class: "content-grid--metrics",
                    item_height: "80px",
                }
            />
        }>
            {move || {
                review.get().map(|result| match result {
                    Ok(review) => render_review_detail(review),
                    Err(err) => view! {
                        <ErrorState
                            heading="Failed to load review detail"
                            message=err.to_string()
                        />
                    }
                    .into_any(),
                })
            }}
        </Suspense>
    }
}

fn render_review_detail(review: Review) -> AnyView {
    let title = review_title(&review);
    let subtitle = review_subtitle(&review);
    let pr_url = review_pr_url(&review);
    let review_id = review.id.to_string();
    let live_path = format!("/reviews/{review_id}/live");
    let is_live = matches!(review.status, ReviewStatus::Pending | ReviewStatus::Running);
    let status = review.status.clone();
    let duration = review
        .duration
        .map(|duration| format_elapsed(duration.as_secs_f64()))
        .unwrap_or_else(|| "—".to_string());
    let total_cost = review
        .analytics
        .as_ref()
        .map(|analytics| format!("${:.4}", analytics.total_cost()))
        .unwrap_or_else(|| "-".to_string());
    let session_count = review.agent_sessions.len().to_string();
    let sessions = review.agent_sessions.clone();

    view! {
        <div class="run-detail-page">
            <PageHeader title=title>
                <StatusBadge status=status.clone() />
                {if is_live {
                    view! {
                        <a href=live_path class="btn btn--ghost btn--sm">"Live"</a>
                    }
                    .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                {if let Some(url) = pr_url {
                    view! {
                        <a
                            href=url
                            target="_blank"
                            rel="noopener noreferrer"
                            class="btn btn--ghost btn--sm"
                        >
                            "Open PR"
                        </a>
                    }
                    .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </PageHeader>

            <p class="text-secondary mb-lg">{subtitle}</p>

            <MetricsGrid>
                <MetricsCard value=review_id label="Review ID" />
                <MetricsCard value=session_count label="Agent Sessions" />
                <MetricsCard value=duration label="Duration" />
                <MetricsCard value=total_cost label="Cost" />
            </MetricsGrid>

            <SectionHeader title="Agent Session Logs" />
            {if sessions.is_empty() {
                view! {
                    <EmptyState message="No agent sessions recorded yet." />
                }
                .into_any()
            } else {
                view! { <LogViewer agent_sessions=sessions /> }.into_any()
            }}
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

fn review_subtitle(review: &Review) -> String {
    match &review.metadata {
        ReviewMetadata::PullRequest(pr) => {
            format!("{}/{}", pr.repository.owner, pr.repository.name)
        }
        ReviewMetadata::Commit(commit) => commit.commit_hash.clone(),
        ReviewMetadata::Plain => "No review metadata available".to_string(),
    }
}

fn review_pr_url(review: &Review) -> Option<String> {
    match &review.metadata {
        ReviewMetadata::PullRequest(pr) if !pr.meta.url.is_empty() => Some(pr.meta.url.clone()),
        _ => None,
    }
}

fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
}
