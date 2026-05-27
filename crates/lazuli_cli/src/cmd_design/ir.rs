//! Local mirror of the `lazuli_ir::Design` IR — see module docs for
//! the cherry-pick swap plan. Shape MUST match `lazuli_ir` field for
//! field; deviations break the orchestrator's `pub use lazuli_ir::*`
//! reconciliation.

use serde::{Deserialize, Serialize};

/// The full `design.lzi` token catalog for one design system.
///
/// Mirrors `lazuli_ir::Design` 1:1 — the `cmd_design` surface keeps a
/// local copy so the CLI can compile against a fixed shape while
/// `lazuli_ir` is still iterating. Both shapes are kept in sync via
/// the planner-side reconciliation test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    /// Catalog identifier (e.g. `"the canonical pilot"`).
    pub name: String,
    /// Optional name of a parent catalog this one extends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Color tokens — one entry per semantic color (`brand`, `surface`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<ColorToken>,
    /// Typography block (families, scale, weights, tracking).
    #[serde(default)]
    pub typography: Typography,
    /// Spacing scale (4, 8, 12, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spaces: Vec<ScaleToken>,
    /// Border-radius scale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radii: Vec<ScaleToken>,
    /// Box-shadow scale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shadows: Vec<ShadowToken>,
    /// Motion durations + easings.
    #[serde(default)]
    pub motion: Motion,
    /// Responsive breakpoints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<ScaleToken>,
    /// Z-index layers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub z_indices: Vec<ZToken>,
}

/// One named color and its full state palette.
///
/// `states` carries the four variants the renderer recognises
/// (`base` / `hover` / `active` / `foreground`); the `dark` companion
/// on each [`ColorState`] holds the dark-theme override when present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorToken {
    /// Token name (`brand`, `surface`, …).
    pub name: String,
    /// Ordered list of color states.
    pub states: Vec<ColorState>,
}

/// One state-variant inside a [`ColorToken`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorState {
    /// Which slot this fills.
    pub kind: ColorStateKind,
    /// Light-mode hex value.
    pub value: String,
    /// Optional dark-mode override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
}

/// Closed catalog of color-state slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorStateKind {
    /// Default fill at rest.
    Base,
    /// Hover/focus variant.
    Hover,
    /// Active/pressed variant.
    Active,
    /// Foreground / text-on-fill variant.
    Foreground,
}

/// Typography catalog — families, scale, weights, tracking.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Typography {
    /// Font families (`sans`, `mono`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<FamilyToken>,
    /// Named text sizes (`xs`/`sm`/`base`/`lg`/…).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scale: Vec<TextScaleToken>,
    /// Font weights.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<WeightToken>,
    /// Letter-spacing tokens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracking: Vec<TrackingToken>,
}

/// One font-family token (e.g. `sans` → `"Inter, system-ui"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyToken {
    /// Token name.
    pub name: String,
    /// Stack value (CSS font-family list).
    pub value: String,
}

/// One entry in the text scale — size paired with line-height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleToken {
    /// Token name (`base`, `lg`, …).
    pub name: String,
    /// Font size (CSS length).
    pub size: String,
    /// Line height (CSS length or unit-less ratio).
    pub line_height: String,
}

/// Named font-weight (`regular: 400`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightToken {
    /// Token name.
    pub name: String,
    /// Numeric weight (1–999).
    pub value: u16,
}

/// Letter-spacing token (`tight: -0.02em`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingToken {
    /// Token name.
    pub name: String,
    /// CSS letter-spacing value.
    pub value: String,
}

/// Generic name/value token used for one-dimensional scales (spacing,
/// radii, breakpoints, durations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleToken {
    /// Token name (`4`, `lg`, `xl`, …).
    pub name: String,
    /// CSS-ready value.
    pub value: String,
}

/// Box-shadow token (`elevated: "0 4px 6px ..."`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowToken {
    /// Token name.
    pub name: String,
    /// CSS shorthand value.
    pub value: String,
}

/// Motion catalog — durations and easings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Motion {
    /// Named durations (`fast`, `normal`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub durations: Vec<ScaleToken>,
    /// Named easing curves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub easings: Vec<EasingToken>,
}

/// One easing token (`standard: cubic-bezier(...)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingToken {
    /// Token name.
    pub name: String,
    /// CSS easing function.
    pub value: String,
}

/// Z-index token (`modal: 1000`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZToken {
    /// Token name.
    pub name: String,
    /// Integer z-index (negative allowed for tucked-under layers).
    pub value: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_serde_roundtrip() {
        let design = Design {
            name: "demo".into(),
            extends: None,
            colors: vec![ColorToken {
                name: "brand".into(),
                states: vec![ColorState {
                    kind: ColorStateKind::Base,
                    value: "#000".into(),
                    dark: Some("#fff".into()),
                }],
            }],
            typography: Typography::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: Motion::default(),
            breakpoints: Vec::new(),
            z_indices: vec![ZToken {
                name: "modal".into(),
                value: 1000,
            }],
        };
        let json = serde_json::to_string(&design).expect("serialize");
        let parsed: Design = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, design);
    }

    #[test]
    fn color_state_kind_renames_to_snake_case() {
        let json = serde_json::to_string(&ColorStateKind::Foreground).unwrap();
        assert_eq!(json, "\"foreground\"");
    }
}
