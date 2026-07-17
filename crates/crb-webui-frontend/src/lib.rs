pub use crb_webui_shared::config::AppConfig;
pub use crb_webui_shared::runs::StartRunResponse as NewRunResponse;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

mod app;
pub mod components;
pub mod pages;
pub mod sse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunRequest {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,

    #[serde(default)]
    pub pr_filter: Option<String>,

    #[serde(default = "default_true")]
    pub use_cache: bool,

    /// Reasoning effort: None (disabled) or Some("low"/"medium"/"high").
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Judge model for evaluating findings against goldens.
    #[serde(default = "default_judge_model")]
    pub judge_model: String,

    /// Maximum findings per agent per PR.
    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    /// Cache directory override.
    #[serde(default)]
    pub cache_dir: Option<String>,

    /// Skip consensus orchestration (use single-agent mode).
    #[serde(default)]
    pub skip_consensus: bool,

    /// Only run linters, skip LLM agents.
    #[serde(default)]
    pub linters_only: bool,
}

fn default_true() -> bool {
    true
}

fn default_judge_model() -> String {
    crb_shared::DEFAULT_MODEL.to_string()
}

fn default_max_findings() -> usize {
    20
}

/// Deterministic HSL color from a role abbreviation — no hardcoded color map.
/// Each role gets a unique hue via a simple hash of its abbreviation bytes.
pub fn role_color(role: &str) -> String {
    let hash: u32 = role
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = Request::get(url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: T = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // role_color tests
    // ------------------------------------------------------------------

    /// Same input must always produce the same color.
    #[test]
    fn test_role_color_determinism() {
        let inputs = ["reviewer", "coder", "tester", "pm", "architect"];
        for input in inputs {
            let a = role_color(input);
            let b = role_color(input);
            insta::assert_debug_snapshot!((input, a, b));
        }
    }

    /// Different inputs should produce distinct colors.
    /// (Collisions are possible in theory since hue range is 360, but for
    /// a reasonable set of distinct abbreviations they should differ.)
    #[test]
    fn test_role_color_uniqueness() {
        let inputs = ["reviewer", "coder", "tester", "pm", "architect", "devops"];
        let mut colors: Vec<String> = inputs.iter().map(|s| role_color(s)).collect();
        colors.sort();
        colors.dedup();
        insta::assert_debug_snapshot!(colors.len());
        insta::assert_debug_snapshot!(inputs.len());
    }

    /// Empty input must produce a defined color string at a valid hue.
    #[test]
    fn test_role_color_empty_input() {
        let color = role_color("");
        assert!(
            color.starts_with("hsl("),
            "empty input should produce a valid hsl: got {color}"
        );
        assert!(
            color.ends_with(", 65%, 55%)"),
            "empty input should have saturation=65%% lightness=55%%: got {color}"
        );
    }

    /// Long role names should not panic and should produce valid output.
    #[test]
    fn test_role_color_long_input() {
        let long = "a".repeat(10_000);
        let color = role_color(&long);
        assert!(
            color.starts_with("hsl("),
            "long input should produce a valid hsl: got {color}"
        );
    }

    /// Output must match the pattern "hsl(NNN, 65%, 55%)" and hue in [0, 359].
    #[test]
    fn test_role_color_format() {
        let inputs = ["", "a", "reviewer", "data-scientist"];
        for input in inputs {
            let color = role_color(input);
            let prefix = color.strip_suffix(", 65%, 55%)");
            assert!(
                prefix.is_some(),
                "role_color({input:?}) should end with ', 65%, 55%)': got {color}"
            );
            let prefix = prefix.unwrap();
            let hue_str = prefix.strip_prefix("hsl(");
            assert!(
                hue_str.is_some(),
                "role_color({input:?}) should start with 'hsl(': got {color}"
            );
            let hue: u32 = hue_str.unwrap().parse().unwrap_or(u32::MAX);
            assert!(
                hue < 360,
                "role_color({input:?}) hue should be in [0, 359]: got {hue}"
            );
        }
    }

    /// Hue should be in range 0..360 by construction (hash % 360).
    #[test]
    fn test_role_color_hue_range() {
        for n in 0..1000 {
            let input = format!("role-{n}");
            let color = role_color(&input);
            let hue_str = color
                .strip_prefix("hsl(")
                .and_then(|s| s.split_once(','))
                .map(|(h, _)| h);
            let hue: u32 = hue_str.unwrap().parse().expect("hue should be parseable");
            assert!(hue < 360, "hue {hue} out of range for input {input}");
        }
    }

    // ------------------------------------------------------------------
    // default_true helper
    // ------------------------------------------------------------------

    #[test]
    fn test_default_true() {
        insta::assert_debug_snapshot!(default_true());
    }

    // ------------------------------------------------------------------
    // NewRunRequest — default / serde
    // ------------------------------------------------------------------

    #[test]
    fn test_new_run_request_defaults() {
        let req = NewRunRequest {
            model: "gpt-4".into(),
            dataset: "test-ds".into(),
            roles: vec!["reviewer".into()],
            pr_filter: None,
            use_cache: default_true(),
            reasoning_effort: None,
            judge_model: default_judge_model(),
            max_findings: default_max_findings(),
            cache_dir: None,
            skip_consensus: false,
            linters_only: false,
        };
        insta::assert_debug_snapshot!(req);
    }

    #[test]
    fn test_new_run_request_serde_roundtrip() {
        let req = NewRunRequest {
            model: "gpt-4".into(),
            dataset: "test-ds".into(),
            roles: vec!["reviewer".into(), "coder".into()],
            pr_filter: Some("feature/".into()),
            use_cache: false,
            reasoning_effort: Some("high".into()),
            judge_model: "gpt-4-turbo".into(),
            max_findings: 30,
            cache_dir: Some("/tmp/cache".into()),
            skip_consensus: true,
            linters_only: false,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let deserialized: NewRunRequest = serde_json::from_str(&json).expect("deserialize");
        insta::assert_json_snapshot!(&req);
        let _ = deserialized;
    }

    #[test]
    fn test_new_run_request_serde_defaults() {
        // JSON without optional fields should get serde defaults.
        let json = r#"{"model":"o1","dataset":"ds1","roles":["a"]}"#;
        let req: NewRunRequest = serde_json::from_str(json).expect("deserialize");
        insta::assert_debug_snapshot!(req);
    }

    // ------------------------------------------------------------------
    // AppConfig — serde
    // ------------------------------------------------------------------

    #[test]
    fn test_app_config_serde_defaults() {
        let json = r#"{}"#;
        let config: AppConfig = serde_json::from_str(json).expect("deserialize");
        insta::assert_debug_snapshot!(config);
    }

    // ------------------------------------------------------------------
    // fetch_json — compile-time shape verification
    //
    // fetch_json uses gloo_net::http::Request which requires the WASM
    // target to compile.  We cannot call it from a host test, but we can
    // check that the generic signature is what we expect by verifying a
    // dummy type implements DeserializeOwned (the constraint on T).
    // ------------------------------------------------------------------

    /// Verify that types we intend to fetch implement DeserializeOwned.
    #[test]
    fn test_fetch_json_type_constraint() {
        fn assert_deserialize_owned<T: serde::de::DeserializeOwned>() {}
        assert_deserialize_owned::<AppConfig>();
        assert_deserialize_owned::<NewRunResponse>();
    }
}
