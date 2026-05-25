//! `design.lzi` declaration AST — design-token surface (L0 #2).
//!
//! Every value is preserved as **raw text** at parse time (hex strings,
//! rem/px literals, font stacks, cubic-bezier strings, weight integers as
//! text) so the analyzer can apply lowering-time validation (hex regex
//! check, shadow single-layer check, `extends` rejection, integer parsing
//! for z-indices). The typed mirror is `lazuli_ir::Design`.
//!
//! Surface shape:
//!
//! ```text
//! design <name>
//!   colors / typography / spaces / radii / shadows / motion /
//!   breakpoints / z_indices / custom
//! ```
//!
//! The `custom` meta-group (L0 #2,
//! `docs/proposals/design-tokens-custom.md`) is flat — no state sub-blocks
//! — and lowering enforces hex validity + reserved-name + collision
//! diagnostics there.

use serde::{Deserialize, Serialize};

use super::Span;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDeclAst {
    pub name: String,
    /// `extends <name>` — captured if present (lowering rejects in v0).
    pub extends: Option<String>,
    pub colors: Vec<ColorTokenAst>,
    pub typography: TypographyAst,
    pub spaces: Vec<ScaleTokenAst>,
    pub radii: Vec<ScaleTokenAst>,
    pub shadows: Vec<ShadowTokenAst>,
    pub motion: MotionAst,
    pub breakpoints: Vec<ScaleTokenAst>,
    pub z_indices: Vec<ZTokenAst>,
    /// L0 #2 — 9th meta-group `custom` per `docs/proposals/design-tokens-custom.md`.
    /// Flat sub-grammar (no state sub-blocks). Lowering enforces hex validity
    /// + reserved-name + collision diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomTokenAst>,
    pub span: Span,
}

/// L0 #2 — single `custom` entry: `<kebab-name> "<hex>" [dark "<hex>"]`.
/// Verbatim values; lowering validates hex shape + reserved-name policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTokenAst {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorTokenAst {
    pub name: String,
    pub states: Vec<ColorStateAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorStateAst {
    /// One of `base | hover | active | foreground`. The analyzer maps
    /// to `ir::ColorStateKind`; unknown names raise a lowering error.
    pub kind: String,
    /// Hex literal verbatim, e.g. `"#7c3aed"`.
    pub value: String,
    /// Optional `dark <hex>` suffix, verbatim.
    pub dark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypographyAst {
    pub families: Vec<FamilyTokenAst>,
    pub scale: Vec<TextScaleTokenAst>,
    pub weights: Vec<WeightTokenAst>,
    pub tracking: Vec<TrackingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleTokenAst {
    pub name: String,
    pub size: String,
    pub line_height: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightTokenAst {
    pub name: String,
    /// Weight literal as text (lowering parses to u16).
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MotionAst {
    pub durations: Vec<ScaleTokenAst>,
    pub easings: Vec<EasingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZTokenAst {
    pub name: String,
    /// Integer literal as text (lowering parses to i32).
    pub value: String,
}
