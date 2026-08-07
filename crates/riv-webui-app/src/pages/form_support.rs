use riv_types::{
    benchmark::golden::GoldenCommentEntry, capabilities::ReasoningEffort, vcs::pr::PrMeta,
};
use riv_webui_shared::config::{AvailableModel, DatasetInfo};
use strum::EnumProperty;

use crate::components::{pr_selection::PrItem, radio_group::RadioOption};

/// Build model radio options from allowlisted model metadata, enabling the
/// hover popover in the form selector.
pub fn allowlisted_model_radio_options(models: &[AvailableModel]) -> Vec<RadioOption> {
    models
        .iter()
        .map(|m| RadioOption {
            value: m.id.clone(),
            label: m.id.clone(),
            disabled: false,
            tooltip: None,
            model: Some(m.clone()),
        })
        .collect()
}

pub fn dataset_radio_options(datasets: &[DatasetInfo]) -> Vec<RadioOption> {
    datasets
        .iter()
        .map(|d| RadioOption {
            value: d.id.clone(),
            label: format!("{} ({} PRs)", d.id, d.pr_count),
            disabled: false,
            tooltip: None,
            model: None,
        })
        .collect()
}

pub fn pr_radio_options(prs: &[PrMeta]) -> Vec<RadioOption> {
    prs.iter()
        .map(|p| RadioOption {
            value: p.url.clone(),
            label: format!("#{} - {}", p.number, p.title),
            disabled: false,
            tooltip: None,
            model: None,
        })
        .collect()
}

pub fn reasoning_radio_options(levels: &[ReasoningEffort]) -> Vec<RadioOption> {
    levels
        .iter()
        .map(|level| RadioOption {
            value: level.to_string(),
            label: level.get_str("Label").unwrap().to_string(),
            disabled: false,
            tooltip: None,
            model: None,
        })
        .collect()
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
        format!("#{} - {}", pr_number_from_url(&self.url), self.pr_title)
    }
}
