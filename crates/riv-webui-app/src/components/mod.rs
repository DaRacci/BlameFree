//! Shared UI components.

use riv_types::review::ReviewStatus;

pub mod agent_pane;
pub mod checkbox_group;
pub mod empty_state;
pub mod error_state;
pub mod form_page;
pub mod form_section;
pub mod loading_state;
pub mod log_section;
pub mod log_text;
pub mod log_viewer;
pub mod metrics_card;
pub mod metrics_grid;
pub mod page_header;
pub mod pr_selection;
pub mod progress_bar;
pub mod role_selector;
pub mod run_status_card;
pub mod run_table;
pub mod section_header;
pub mod select_field;
pub mod status_badge;
pub mod submit_button;
pub mod text_field;

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
