use leptos::prelude::*;
use riv_types::review::ReviewStatus;

use crate::components::status_badge_class;

/// Status badge for a review.
#[component]
pub fn StatusBadge(status: ReviewStatus) -> impl IntoView {
    let variant = status_badge_class(&status);
    let is_running = matches!(status, ReviewStatus::Running);
    let label = status.to_string();
    let dot_class = if is_running {
        "badge__dot badge__dot--pulse"
    } else {
        "badge__dot"
    };

    view! {
        <span class=format!("badge {}", variant)>
            <span class=dot_class></span>
            <span class="badge__label">{label}</span>
        </span>
    }
}
