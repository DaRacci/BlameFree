use riv_types::{
    benchmark::golden::GoldenCommentEntry, capabilities::ReasoningEffort, vcs::pr::PrMeta,
};
use riv_webui_shared::config::DatasetInfo;

use crate::components::{pr_selection::PrItem, select_field::SelectOption};

pub fn model_options(models: &[String]) -> Vec<SelectOption> {
    models
        .iter()
        .map(|model| SelectOption {
            value: model.clone(),
            label: model.clone(),
        })
        .collect()
}

pub fn dataset_options(datasets: &[DatasetInfo]) -> Vec<SelectOption> {
    datasets
        .iter()
        .map(|dataset| SelectOption {
            value: dataset.id.clone(),
            label: format!("{} ({} PRs)", dataset.id, dataset.pr_count),
        })
        .collect()
}

pub fn pr_options(prs: &[PrMeta]) -> Vec<SelectOption> {
    prs.iter()
        .map(|pr| SelectOption {
            value: pr.url.clone(),
            label: format!("#{} — {}", pr.number, pr.title),
        })
        .collect()
}

pub fn reasoning_options(levels: &[ReasoningEffort]) -> Vec<SelectOption> {
    levels
        .iter()
        .map(|level| SelectOption {
            value: reasoning_value(*level).to_string(),
            label: reasoning_label(*level).to_string(),
        })
        .collect()
}

pub const fn reasoning_value(level: ReasoningEffort) -> &'static str {
    match level {
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
        ReasoningEffort::Max => "max",
    }
}

pub const fn reasoning_label(level: ReasoningEffort) -> &'static str {
    match level {
        ReasoningEffort::Low => "Low",
        ReasoningEffort::Medium => "Medium",
        ReasoningEffort::High => "High",
        ReasoningEffort::XHigh => "X-High",
        ReasoningEffort::Max => "Max",
    }
}

pub fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffort> {
    match value {
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::XHigh),
        "max" => Some(ReasoningEffort::Max),
        _ => None,
    }
}

pub fn pr_number_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("?")
        .to_string()
}

impl PrItem for GoldenCommentEntry {
    fn pr_key(&self) -> &str {
        &self.url
    }

    fn pr_label(&self) -> String {
        format!("#{} — {}", pr_number_from_url(&self.url), self.pr_title)
    }
}
