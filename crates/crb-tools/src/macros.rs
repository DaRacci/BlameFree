//! Declarative macros for reducing tool boilerplate.
//!
//! The [`impl_tool!`] macro generates the full [`rig_core::tool::Tool`]
//! implementation for a tool, including the `call()` method body.
//!
//! # Usage
//!
//! ```ignore
//! crate::impl_tool! { MyTool, MyArgs, MyError, String, "my_tool",
//!     "Description of what my_tool does.",
//!     async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
//!         // custom logic here
//!         Ok("result".into())
//!     }
//! }
//! ```

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
/// | Position | Symbol   | Description                                    |
/// |----------|----------|------------------------------------------------|
/// | 0        | `$tool`  | The tool struct type (e.g. `GrepTool`)         |
/// | 1        | `$args`  | The arguments struct type (e.g. `GrepArgs`)   |
/// | 2        | `$error` | The error enum type (e.g. `GrepError`)         |
/// | 3        | `$output`| The output type (e.g. `String`)                |
/// | 4        | `$name`  | String literal — tool name for `Self::NAME`    |
/// | 5        | `$description` | String literal — description for `ToolDefinition` |
/// | 6+       | `$($call:tt)*` | The entire `async fn call(...)` method     |
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
