use serde::{Deserialize, Serialize};
use strum::{Display, EnumProperty, EnumString, IntoStaticStr, VariantArray};

/// The reasoning effort level for OpenAI style reasoning.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    VariantArray,
    EnumString,
    Display,
    IntoStaticStr,
    EnumProperty,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ReasoningEffort {
    /// Faster responses, less deep reasoning.
    #[strum(props(Label = "Low"))]
    Low = 2048,

    /// Balanced depth and speed.
    #[default]
    #[strum(props(Label = "Medium"))]
    Medium = 6144,

    /// More thorough reasoning.
    #[strum(props(Label = "High"))]
    High = 12288,

    /// Even more thorough reasoning.
    #[strum(props(Label = "X-High"))]
    XHigh = 16384,

    /// Most thorough, slowest.
    #[strum(props(Label = "Max"))]
    Max = 32768,
}
