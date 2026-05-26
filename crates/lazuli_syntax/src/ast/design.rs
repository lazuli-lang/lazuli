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

/// `design <name>` block — design-token surface (L0 #2).
///
/// Container for the nine design-token meta-groups. Every value stays
/// as raw text so the analyzer can validate (hex shape, shadow
/// composition, integer parsing) at lowering time.
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

/// One named color token + its state expansion (`base`/`hover`/...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorTokenAst {
    pub name: String,
    pub states: Vec<ColorStateAst>,
    pub span: Span,
}

/// One state row inside a [`ColorTokenAst`].
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

/// Typography meta-group sub-tree inside [`DesignDeclAst`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypographyAst {
    pub families: Vec<FamilyTokenAst>,
    pub scale: Vec<TextScaleTokenAst>,
    pub weights: Vec<WeightTokenAst>,
    pub tracking: Vec<TrackingTokenAst>,
}

/// One font-family stack row inside [`TypographyAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTokenAst {
    pub name: String,
    /// Comma-joined family stack literal verbatim.
    pub value: String,
}

/// One type-scale row inside [`TypographyAst`] (size + line-height pair).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleTokenAst {
    pub name: String,
    /// Size literal verbatim (e.g. `1rem`, `14px`).
    pub size: String,
    /// Line-height literal verbatim.
    pub line_height: String,
}

/// One weight token row inside [`TypographyAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightTokenAst {
    pub name: String,
    /// Weight literal as text (lowering parses to u16).
    pub value: String,
}

/// One letter-tracking row inside [`TypographyAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingTokenAst {
    pub name: String,
    /// Tracking literal verbatim (e.g. `0.02em`).
    pub value: String,
}

/// One scalar token (space / radius / breakpoint / motion duration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleTokenAst {
    pub name: String,
    /// Scalar literal verbatim (e.g. `4px`, `0.25rem`).
    pub value: String,
}

/// One shadow row inside [`DesignDeclAst`]. Single-layer enforcement
/// lives in the analyzer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowTokenAst {
    pub name: String,
    /// CSS shadow literal verbatim.
    pub value: String,
}

/// Motion meta-group sub-tree inside [`DesignDeclAst`] (durations + easings).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MotionAst {
    pub durations: Vec<ScaleTokenAst>,
    pub easings: Vec<EasingTokenAst>,
}

/// One easing token row inside [`MotionAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingTokenAst {
    pub name: String,
    /// Cubic-bezier or keyword easing string verbatim.
    pub value: String,
}

/// One z-index row inside [`DesignDeclAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZTokenAst {
    pub name: String,
    /// Integer literal as text (lowering parses to i32).
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typography_ast_default_is_all_empty() {
        let t = TypographyAst::default();
        assert!(t.families.is_empty());
        assert!(t.scale.is_empty());
    }

    #[test]
    fn custom_token_dark_optional_elides() {
        let c = CustomTokenAst {
            name: "brand-glow".into(),
            value: "#7c3aed".into(),
            dark: None,
            span: Span::new(0, 0),
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("dark"));
    }

    #[test]
    fn motion_ast_default_construct() {
        let m = MotionAst::default();
        assert!(m.durations.is_empty());
        assert!(m.easings.is_empty());
    }
}
