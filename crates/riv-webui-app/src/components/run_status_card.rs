use leptos::prelude::*;
use riv_types::review::{Review, ReviewMetadata};

use super::status_badge::StatusBadge;

fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
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
            let hash = short_hash(&c.commit_hash);
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
        ReviewMetadata::Commit(c) => short_hash(&c.commit_hash).to_string(),
        ReviewMetadata::Plain => r.id.to_string(),
    }
}

fn review_url(r: &Review) -> Option<String> {
    match &r.metadata {
        ReviewMetadata::PullRequest(pr) if !pr.meta.url.is_empty() => Some(pr.meta.url.clone()),
        _ => None,
    }
}

pub fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs_rem = total % 60;
    format!("{:02}:{:02} elapsed", mins, secs_rem)
}

/// Review status card.
///
/// `active = true` renders the running-state card style.
/// `active = false` (default) renders the history card with ID + maybe link in the footer.
#[component]
pub fn RunStatusCard(review: Review, #[prop(optional)] active: bool) -> impl IntoView {
    let label = review_label(&review);
    let subtitle = review_subtitle(&review);
    let agent_count = review.agent_sessions.len();
    let pr_url = review_url(&review);
    let id_str = review.id.to_string();
    let status = review.status.clone();

    let elapsed_opt = review.duration.map(|d| format_elapsed(d.as_secs_f64()));
    let elapsed_text = if active {
        elapsed_opt.unwrap_or_else(|| "In progress".into())
    } else {
        elapsed_opt.map(|e| format!(" ({})", e)).unwrap_or_default()
    };
    let class = match active {
        true => "card card--active-run",
        false => "card",
    };

    let link = if let Some(url) = pr_url
        && active
    {
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
    };

    view! {
        <div class={class}>
            <div class="card__header">
                <h3 class="card__title">{label}</h3>
                <StatusBadge status=status />
            </div>
            <div class="card__body">
                <div class="home-page__meta-row flex-row gap-lg text-sm text-secondary">
                    <span>{subtitle}</span>
                    <span>{agent_count} " agent(s)"</span>
                    <span>{elapsed_text}</span>
                </div>
            </div>
            <div class="card__footer flex-row justify-between items-center text-xs text-secondary">
                <span>{id_str}</span>
                {link}
            </div>
        </div>
    }
    .into_any()
}
