//! Declarative macros for reducing tool boilerplate.

/// Generate a full [`rig_core::tool::Tool`] implementation.
///
/// Expands to an `impl Tool for $tool` block containing:
/// - `const NAME`
/// - `type Error = $error; type Args = $args; type Output = $output`
/// - `async fn definition(&self, _prompt: String) -> ToolDefinition`
/// - The `async fn call(...)` body passed as trailing token trees
///
/// Uses fully-qualified paths so the macro works regardless of imports
/// at the call site.
///
/// # Arguments
///
/// | Position | Symbol   | Description                                             |
/// |----------|----------|---------------------------------------------------------|
/// | 0        | `$tool`  | The tool struct type (e.g. `GrepTool`)                  |
/// | 1        | `$args`  | The arguments struct type (e.g. `GrepArgs`)             |
/// | 2        | `$error` | The error enum type (e.g. `GrepError`)                  |
/// | 3        | `$output`| The output type (e.g. `String`)                         |
/// | 4        | `$name`  | String literal — tool name for `Self::NAME`             |
/// | 5        | `$description` | String literal — description for `ToolDefinition` |
/// | 6+       | `$($call:tt)*` | The entire `async fn call(...)` method            |
///
/// ```ignore
/// crate::impl_tool! { MyTool, MyArgs, MyError, String, "my_tool",
///     "Description of what my_tool does.",
///     async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
///         // custom logic here
///         Ok("result".into())
///     }
/// }
/// ```
#[macro_export]
macro_rules! impl_tool {
    ($tool:ty, $args:ty, $error:ty, $output:ty, $name:expr, $description:expr, $($call:tt)*) => {
        impl rig_core::tool::Tool for $tool {
            const NAME: &'static str = $name;

            type Error = $error;
            type Args = $args;
            type Output = $output;

            async fn definition(&self, _prompt: String) -> rig_core::completion::ToolDefinition {
                rig_core::completion::ToolDefinition {
                    name: Self::NAME.to_string(),
                    description: $description.to_string(),
                    parameters: serde_json::to_value(schemars::schema_for!($args))
                        .unwrap_or_default(),
                }
            }

            $($call)*
        }
    };
}

#[cfg(test)]
mod tests {
    use rig_core::tool::Tool;
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, JsonSchema)]
    struct MockArgs {
        pub input: String,
    }

    #[derive(Debug, thiserror::Error)]
    enum MockError {
        #[allow(unused)]
        #[error("mock error: {0}")]
        General(String),
    }

    struct MockTool;

    impl_tool! { MockTool, MockArgs, MockError, String, "mock_tool",
        "A mock tool for testing the impl_tool! macro.",
        async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
            Ok(args.input)
        }
    }

    fn _assert_types()
    where
        MockTool: Tool<Args = MockArgs, Error = MockError, Output = String>,
    {
    }

    #[tokio::test]
    async fn test_impl_tool_generates_correct_constants() {
        insta::assert_snapshot!(MockTool::NAME);

        let tool = MockTool;
        let def = tool.definition("prompt".into()).await;
        insta::assert_debug_snapshot!(def.name);
        insta::assert_debug_snapshot!(def.description);
    }

    struct MockToolEmptyDesc;

    impl_tool! { MockToolEmptyDesc, MockArgs, MockError, String, "empty_desc",
        "",
        async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
            Ok(args.input)
        }
    }

    #[tokio::test]
    async fn test_impl_tool_empty_description() {
        let tool = MockToolEmptyDesc;
        let def = tool.definition("".into()).await;
        insta::assert_snapshot!(def.description);
    }

    struct MockToolSpecial;

    impl_tool! { MockToolSpecial, MockArgs, MockError, String, "my_custom-tool_v2",
        "Tool with special characters in name.",
        async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
            Ok(args.input)
        }
    }

    #[test]
    fn test_impl_tool_special_characters_in_name() {
        insta::assert_snapshot!(MockToolSpecial::NAME);
    }
}
