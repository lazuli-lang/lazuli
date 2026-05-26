//! Local mirror of the `lazuli_ir::Design` IR — see module docs for
//! the cherry-pick swap plan. Shape MUST match `lazuli_ir` field for
//! field; deviations break the orchestrator's `pub use lazuli_ir::*`
//! reconciliation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<ColorToken>,
    #[serde(default)]
    pub typography: Typography,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spaces: Vec<ScaleToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radii: Vec<ScaleToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shadows: Vec<ShadowToken>,
    #[serde(default)]
    pub motion: Motion,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<ScaleToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub z_indices: Vec<ZToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorToken {
    pub name: String,
    pub states: Vec<ColorState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorState {
    pub kind: ColorStateKind,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorStateKind {
    Base,
    Hover,
    Active,
    Foreground,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Typography {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<FamilyToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scale: Vec<TextScaleToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<WeightToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracking: Vec<TrackingToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleToken {
    pub name: String,
    pub size: String,
    pub line_height: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightToken {
    pub name: String,
    pub value: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Motion {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub durations: Vec<ScaleToken>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub easings: Vec<EasingToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZToken {
    pub name: String,
    pub value: i32,
}
