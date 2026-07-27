use serde::{Deserialize, Serialize};

pub trait WrappedData {
    fn get(&self) -> &str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt(pub String);

impl WrappedData for Prompt {
    fn get(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model(pub String);

impl WrappedData for Model {
    fn get(&self) -> &str {
        &self.0
    }
}

impl Model {
    pub fn is_claude(&self) -> bool {
        self.0.to_lowercase().contains("claude")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_wrapper_get() {
        let prompt = Prompt("test prompt".into());
        insta::assert_debug_snapshot!(prompt.get());
    }

    #[test]
    fn test_model_wrapper_get() {
        let model = Model("gpt-4".into());
        insta::assert_debug_snapshot!(model.get());
    }

    #[test]
    fn test_model_wrapper_clone() {
        let model = Model("claude-3".into());
        let cloned = model.clone();
        insta::assert_debug_snapshot!(cloned.get());
        // Verify the clone is independent
        let model2 = Model("gpt-4o".into());
        let cloned2 = model2.clone();
        insta::assert_debug_snapshot!(cloned2.get());
    }
}
