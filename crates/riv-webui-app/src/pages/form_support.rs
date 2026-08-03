#[cfg(feature = "ssr")]
use riv_shared::{DEFAULT_MODEL, DEFAULT_MODEL_PRO};
use riv_types::{
    benchmark::golden::GoldenCommentEntry, capabilities::ReasoningEffort, vcs::pr::PrMeta,
};
#[cfg(feature = "ssr")]
use riv_webui_shared::config::AgentInfo;
use riv_webui_shared::config::DatasetInfo;

use crate::components::{pr_selection::PrItem, select_field::SelectOption};

#[cfg(feature = "ssr")]
pub fn placeholder_models() -> Vec<String> {
    vec![
        DEFAULT_MODEL.to_string(),
        DEFAULT_MODEL_PRO.to_string(),
        "openai/gpt-5-mini".to_string(),
    ]
}

#[cfg(feature = "ssr")]
pub fn placeholder_roles() -> Vec<AgentInfo> {
    vec![
        AgentInfo {
            name: "Security Analyst".to_string(),
            abbreviation: "SA".to_string(),
            incompatible_with_roles: Vec::new(),
        },
        AgentInfo {
            name: "Correctness Analyst".to_string(),
            abbreviation: "CA".to_string(),
            incompatible_with_roles: Vec::new(),
        },
        AgentInfo {
            name: "Performance Analyst".to_string(),
            abbreviation: "PA".to_string(),
            incompatible_with_roles: Vec::new(),
        },
    ]
}

#[cfg(feature = "ssr")]
pub fn placeholder_datasets() -> Vec<DatasetInfo> {
    vec![
        DatasetInfo {
            id: "placeholder-small".to_string(),
            path: "datasets/placeholder-small".to_string(),
            pr_count: 3,
        },
        DatasetInfo {
            id: "placeholder-large".to_string(),
            path: "datasets/placeholder-large".to_string(),
            pr_count: 5,
        },
    ]
}

#[cfg(feature = "ssr")]
pub fn placeholder_reasoning_efforts(_model: &str) -> Vec<ReasoningEffort> {
    vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::XHigh,
        ReasoningEffort::Max,
    ]
}

#[cfg(feature = "ssr")]
pub fn placeholder_repo_prs(owner: &str, repo: &str) -> Vec<PrMeta> {
    let base_url = format!("https://github.com/{owner}/{repo}/pull");
    [101_u32, 102, 103]
        .into_iter()
        .map(|number| PrMeta {
            title: format!("Placeholder PR #{number} for {owner}/{repo}"),
            url: format!("{base_url}/{number}"),
            number,
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub fn placeholder_dataset_prs(dataset_id: &str) -> Vec<GoldenCommentEntry> {
    [201_u32, 202, 203]
        .into_iter()
        .map(|number| GoldenCommentEntry {
            pr_title: format!("{dataset_id} placeholder PR #{number}"),
            url: format!("https://github.com/example/{dataset_id}/pull/{number}"),
            comments: Vec::new(),
        })
        .collect()
}

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
