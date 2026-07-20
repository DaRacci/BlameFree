use serde::{Deserialize, Serialize};
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

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
)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    /// Faster responses, less deep reasoning.
    Low = 2048,

    /// Balanced depth and speed.
    #[default]
    Medium = 6144,

    /// More thorough reasoning.
    High = 12288,

    /// Even more thorough reasoning.
    XHigh = 16384,

    /// Most thorough, slowest.
    Max = 32768,
}
