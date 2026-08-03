use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use riv_types::{
    benchmark::result::PrResult,
    review::{Review, ReviewMetadata, ReviewStatus},
};

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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ReviewDetailData {
    review: Review,
    pr_results: Vec<PrResult>,
}

#[server]
async fn read_review_detail(review_id: String) -> Result<ReviewDetailData, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    let review_id = review_id
        .parse::<mti::prelude::MagicTypeId>()
        .map_err(|error| ServerFnError::new(format!("invalid review id: {error}")))?;
    let review = (services.get_review)((review_id.clone(),))
        .await
        .map_err(ServerFnError::new)?;
    let pr_results = (services.list_pr_results)((review_id,))
        .await
        .map_err(ServerFnError::new)?;

    Ok(ReviewDetailData { review, pr_results })
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
                    count: 6,
                    grid_class: "content-grid--metrics",
                    item_height: "80px",
                }
            />
        }>
            {move || {
                review.get().map(|result| match result {
                    Ok(detail) => render_review_detail(detail),
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

fn render_review_detail(detail: ReviewDetailData) -> AnyView {
    let ReviewDetailData { review, pr_results } = detail;
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
    let inferred_session_count = if !review.agent_sessions.is_empty() {
        Some(review.agent_sessions.len())
    } else {
        review
            .analytics
            .as_ref()
            .map(|analytics| analytics.sessions.len())
    };
    let session_count = inferred_session_count
        .filter(|count| *count > 0)
        .map(|count| count.to_string())
        .unwrap_or_else(|| {
            if pr_results.is_empty() {
                "0".to_string()
            } else {
                "—".to_string()
            }
        });
    let pr_result_count = pr_results.len().to_string();
    let finding_count = pr_results
        .iter()
        .map(|result| result.findings.len())
        .sum::<usize>()
        .to_string();
    let sessions = review.agent_sessions.clone();
    let missing_transcripts = sessions.is_empty() && !pr_results.is_empty();

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
                <MetricsCard value=review_id label="Review ID" truncate=true />
                <MetricsCard value=session_count label="Agent Sessions" />
                <MetricsCard value=duration label="Duration" />
                <MetricsCard value=total_cost label="Cost" />
                <MetricsCard value=pr_result_count label="PR Results" />
                <MetricsCard value=finding_count label="Findings" />
            </MetricsGrid>

            {if missing_transcripts {
                view! {
                    <div class="card mb-lg">
                        <div class="card__body">
                            <p class="text-secondary">
                                "This run has stored PR result files, but no persisted agent transcript turns. Imported/legacy benchmark runs commonly look like this."
                            </p>
                        </div>
                    </div>
                }
                .into_any()
            } else {
                view! { <span></span> }.into_any()
            }}

            <SectionHeader title="Agent Session Logs" />
            {if sessions.is_empty() {
                view! {
                    <EmptyState
                        message=if missing_transcripts {
                            "No agent session transcripts stored for this run.".to_string()
                        } else {
                            "No agent sessions recorded yet.".to_string()
                        }
                    />
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
