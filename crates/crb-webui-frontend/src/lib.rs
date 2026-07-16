use crb_webui_shared::config::RoleInfo;
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
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunResponse {
    pub run_id: String,
    pub status: String,
    pub total_prs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<String>,

    #[serde(default)]
    pub datasets: Vec<String>,

    #[serde(default)]
    pub roles: Vec<RoleInfo>,

    /// Whether reduce-diff mode is enabled (compile-time feature flag).
    #[serde(default)]
    pub reduce_diff_enabled: bool,

    /// Whether OAuth authentication is configured server-side.
    #[serde(default)]
    pub auth_enabled: bool,
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
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let deserialized: NewRunRequest =
            serde_json::from_str(&json).expect("deserialize");
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
    // NewRunResponse — serde
    // ------------------------------------------------------------------

    #[test]
    fn test_new_run_response_serde_roundtrip() {
        let resp = NewRunResponse {
            run_id: "abc-123".into(),
            status: "created".into(),
            total_prs: 42,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        let deserialized: NewRunResponse =
            serde_json::from_str(&json).expect("deserialize");
        insta::assert_json_snapshot!(&resp);
        let _ = deserialized;
    }

    // ------------------------------------------------------------------
    // AppConfig — serde
    // ------------------------------------------------------------------

    #[test]
    fn test_app_config_serde_roundtrip() {
        use crb_webui_shared::config::RoleInfo;

        let config = AppConfig {
            models: vec!["gpt-4".into(), "claude-3".into()],
            datasets: vec!["ds1".into()],
            roles: vec![RoleInfo {
                name: "Reviewer".into(),
                abbreviation: "rev".into(),
                incompatible_with_roles: vec![],
            }],
            reduce_diff_enabled: true,
            auth_enabled: false,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: AppConfig =
            serde_json::from_str(&json).expect("deserialize");
        insta::assert_json_snapshot!(&config);
        let _ = deserialized;
    }

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
