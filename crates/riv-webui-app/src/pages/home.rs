use leptos::prelude::*;
use lucide_leptos::TriangleAlert;
use riv_types::review::{Review, ReviewMetadata, ReviewStatus};

use crate::async_resource::{ReloadableResource, create_reloadable_resource};
use crate::components::{
    format_elapsed, page_header::PageHeader, run_table::RunTable, status_badge_class,
};

#[server]
async fn list_reviews_data() -> Result<Vec<Review>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_reviews)(())
        .await
        .map_err(ServerFnError::new)
}

fn review_label(r: &Review) -> String {
    match &r.metadata {
        ReviewMetadata::PullRequest(pr) => {
            if pr.meta.title.is_empty() {
                format!(
                    "{}/{} #{}",
                    pr.repository.owner, pr.repository.name, pr.meta.number
                )
            } else {
                pr.meta.title.clone()
            }
        }
        ReviewMetadata::Commit(c) => {
            let hash = if c.commit_hash.len() > 7 {
                &c.commit_hash[..7]
            } else {
                &c.commit_hash
            };
            let repo = c
                .repository
                .repo_root
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("commit");
            format!("{} @ {}", repo, hash)
        }
        ReviewMetadata::Plain => r.id.to_string(),
    }
}

fn review_subtitle(r: &Review) -> String {
    match &r.metadata {
        ReviewMetadata::PullRequest(pr) => {
            format!("{}/{}", pr.repository.owner, pr.repository.name)
        }
        ReviewMetadata::Commit(c) => {
            let hash = if c.commit_hash.len() > 7 {
                &c.commit_hash[..7]
            } else {
                &c.commit_hash
            };
            hash.to_string()
        }
        ReviewMetadata::Plain => r.id.to_string(),
    }
}

fn render_loading() -> AnyView {
    view! {
        <div class="mt-xl">
            <div class="skeleton skeleton--card mb-lg" style="height: 180px;"></div>
            <div class="skeleton skeleton--card" style="height: 300px;"></div>
        </div>
    }
    .into_any()
}

fn render_error(message: String, reviews: ReloadableResource<Vec<Review>>) -> AnyView {
    view! {
        <div class="error-state" role="alert">
            <div class="error-state__icon"><TriangleAlert size=24 /></div>
            <h3 class="error-state__heading">"Failed to load reviews"</h3>
            <p class="error-state__message">{format!("Something went wrong: {}", message)}</p>
            <div class="error-state__action">
                <button class="btn btn--primary" on:click=move |_| reviews.refresh()>
                    "Retry"
                </button>
            </div>
        </div>
    }
    .into_any()
}

fn render_reviews(reviews: Vec<Review>) -> AnyView {
    let active_runs = reviews
        .iter()
        .filter(|r| matches!(r.status, ReviewStatus::Pending | ReviewStatus::Running))
        .cloned()
        .collect::<Vec<_>>();

    let history_runs = reviews
        .into_iter()
        .filter(|r| {
            matches!(
                r.status,
                ReviewStatus::Completed | ReviewStatus::Failed | ReviewStatus::Cancelled
            )
        })
        .collect::<Vec<_>>();

    view! {
        <>
            <div class="section-header">
                <h2 class="section-header__title">
                    <span class="active-runs-indicator"></span>
                    "Active Reviews"
                </h2>
                {if !active_runs.is_empty() {
                    view! {
                        <span class="active-runs-count">
                            {format!("{} running", active_runs.len())}
                        </span>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>

            {if active_runs.is_empty() {
                view! {
                    <div class="empty-state py-xl">
                        <p class="empty-state__message" style="margin: 0;">
                            "No active reviews"
                        </p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="content-grid content-grid--cards">
                        {active_runs.into_iter().map(|run| {
                            let label = review_label(&run);
                            let subtitle = review_subtitle(&run);
                            let status = &run.status;
                            let badge_class = status_badge_class(status);
                            let agent_count = run.agent_sessions.len();
                            let elapsed = run.duration
                                .map(|d| format_elapsed(d.as_secs_f64()))
                                .unwrap_or_else(|| "In progress".into());
                            let id_str = run.id.to_string();
                            view! {
                                <div class="card card--active-run">
                                    <div class="card__header">
                                        <h3 class="card__title">{label}</h3>
                                        <span class=format!("badge {}", badge_class)>
                                            <span class="badge__dot badge__dot--pulse"></span>
                                            <span class="badge__label">{status.to_string()}</span>
                                        </span>
                                    </div>
                                    <div class="card__body">
                                        <div class="home-page__meta-row flex-row gap-lg text-sm text-secondary">
                                            <span>{subtitle}</span>
                                            <span>{agent_count} " agent(s)"</span>
                                            <span>{elapsed}</span>
                                        </div>
                                    </div>
                                    <div class="card__footer text-xs text-secondary">
                                        {id_str}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}

            <div class="section-header">
                <h2 class="section-header__title">"Previous Reviews"</h2>
            </div>

            {if history_runs.is_empty() {
                view! {
                    <div class="empty-state py-xl">
                        <p class="empty-state__message" style="margin: 0;">
                            "No previous reviews"
                        </p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <RunTable runs=history_runs />
                }.into_any()
            }}
        </>
    }
    .into_any()
}

#[component]
pub fn HomePage() -> impl IntoView {
    let reviews = create_reloadable_resource(move || async move {
        list_reviews_data().await.map_err(|err| err.to_string())
    });

    #[cfg(target_arch = "wasm32")]
    {
        let reviews = reviews.clone();
        Effect::new(move || {
            use gloo_timers::callback::Interval;

            let interval = Interval::new(10_000, {
                let reviews = reviews.clone();
                move || {
                    let any_active = reviews
                        .data()
                        .map(|items| {
                            items.iter().any(|r| {
                                matches!(r.status, ReviewStatus::Pending | ReviewStatus::Running)
                            })
                        })
                        .unwrap_or(false);

                    if any_active {
                        reviews.refresh();
                    }
                }
            });

            interval.forget();
        });
    }

    view! {
        <div class="home-page">
            <PageHeader title="Overview">
                <a href="/reviews/new" class="btn btn--primary btn--sm">"New Review"</a>
                <a href="/benchmarks/new" class="btn btn--ghost btn--sm">"New Benchmark"</a>
            </PageHeader>

            <Suspense fallback=render_loading>
                {move || {
                    reviews.resource.get().map(|result| {
                        let retry = reviews.clone();
                        match result {
                            Ok(items) => render_reviews(items),
                            Err(err) => render_error(err, retry),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
