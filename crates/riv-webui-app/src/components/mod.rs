//! Shared UI components.

use riv_types::review::ReviewStatus;

pub mod agent_pane;
pub mod forms;
pub mod layout;
pub mod logs;
pub mod metrics;
pub mod popovers;
pub mod run_status_card;
pub mod run_table;
pub mod state;

pub(crate) const fn status_badge_class(status: &ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::Completed => "badge--success",
        ReviewStatus::Failed => "badge--danger",
        ReviewStatus::Running => "badge--warning",
        ReviewStatus::Pending | ReviewStatus::Cancelled => "badge--neutral",
    }
}

pub(crate) fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs_rem = total % 60;
    format!("{:02}:{:02} elapsed", mins, secs_rem)
}
