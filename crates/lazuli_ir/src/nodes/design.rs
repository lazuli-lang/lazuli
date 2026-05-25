//! Design Tokens IR — `design.lzi` lowered shape.
//!
//! Design Tokens are the language's closed-catalog vocabulary for visual
//! primitives. The author writes `design.lzi` at the project root and
//! gets a tight palette of eight groups — `color`, `typography`, `space`,
//! `radius`, `shadow`, `motion`, `breakpoint`, `z` — plus an additive
//! `custom` group for product-domain colors. There is no ninth group an
//! application can add: extending the catalog requires a Lazuli core
//! proposal. This is Rule Zero in action — the surface is small enough
//! that an LLM can hold it entirely in working memory.
//!
//! ## Why a closed catalog
//!
//! Free-form design tokens are the standard industry shape (Tailwind,
//! Style Dictionary, every design-system framework) and they all share
//! the same failure mode: every product re-invents its own group names,
//! the namespace explodes, and downstream emitters can't reason about
//! what each token *means*. Lazuli locks the vocabulary so emitters
//! (`tokens.ts`, `tokens.css`, `tailwind.gen.ts`, …) can mechanically
//! project every token without per-app glue. The cost is rigidity; the
//! pay-off is multi-target portability.
//!
//! ## Color states
//!
//! [`ColorStateKind`] enumerates the closed catalog of color states
//! (`base` / `hover` / `active` / `foreground`). The flat form
//! `success "#16a34a"` lowers to one state with `kind=Base`; the
//! sub-block form `primary { base / hover / active / foreground }`
//! lowers to up to four entries. Dark-mode overlays ride on
//! [`ColorState::dark`].
//!
//! ## Custom group (additive)
//!
//! [`CustomToken`] is the ninth meta-group landed via
//! `docs/proposals/design-tokens-custom.md`. It exists for
//! product-domain color tokens (WhatsApp green, brand accents) that
//! don't fit the Shadcn-semantic vocabulary in `color`. Lowering emits
//! these alongside `colors` under the `--color-*` CSS-var prefix;
//! doctor enforces collision and reserved-name policy.
//!
//! ## See also
//!
//! - `docs/proposals/design-tokens.md` §3 — canonical surface.
//! - `docs/proposals/design-tokens-custom.md` — `custom` group rationale.
//! - `lazuli_codegen_*` — emitter consumers of this IR.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// L0 #2 — top-level design tokens catalog. Eight closed groups carry
/// closed-catalog token sub-shapes. `extends` is reserved for Cut B
/// brand variants; v0 lowering rejects it (DESIGN-EXTENDS-CUT-B). The
/// surface is intentionally narrow — no group can be extended outside
/// a Lazuli core proposal (closed catalog, Rule Zero).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    pub name: String,
    /// Reserved for Cut B (brand variants). v0 lowering rejects when
    /// `Some`. Keyword is parsed so v0 → Cut B is additive on lowering
    /// only, not grammar.
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
    /// L0 #2 — 9th meta-group `custom` (proposal-pending per
    /// `docs/proposals/design-tokens-custom.md`). Product-domain color
    /// tokens that don't fit the Shadcn-semantic vocabulary in `color`.
    /// Lowering emits these alongside `colors` under the `--color-*` CSS-var
    /// prefix; doctor enforces collision + reserved-name policy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// L0 #2 — flat custom-group entry. `name` is kebab-case; `base` is the
/// light-mode hex; `dark` is the optional dark-mode overlay. See
/// `docs/proposals/design-tokens-custom.md` §2 for the grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomToken {
    pub name: String,
    /// Hex literal preserved verbatim, e.g. `"#dcf8c6"`.
    pub base: String,
    /// Optional `dark <hex>` overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Single named color. `states` carries one entry (`kind=Base`) for the
/// flat form `success "#16a34a"`, or up to four entries (one per state)
/// for the sub-block form `primary { base / hover / active / foreground }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorToken {
    pub name: String,
    pub states: Vec<ColorState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorState {
    pub kind: ColorStateKind,
    /// Hex literal preserved verbatim, e.g. `"#7c3aed"`.
    pub value: String,
    /// Optional `dark <hex>` companion; `None` = same value in both themes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
}

/// Closed catalog of color states. Adding entries requires a new L0
/// proposal (per `docs/proposals/design-tokens.md` §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorStateKind {
    /// Default state. Used both for the flat form (`success "#16a34a"`)
    /// and the explicit `base "#..."` entry inside a sub-block.
    Base,
    Hover,
    Active,
    Foreground,
}

/// `typography` group with four closed sub-groups.
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
    /// Font stack string, e.g. `"Inter, system-ui, sans-serif"`.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleToken {
    pub name: String,
    /// Size literal preserved verbatim, e.g. `"0.75rem"`.
    pub size: String,
    /// Line-height literal preserved verbatim, e.g. `"1rem"` or `"1.5"`.
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
    /// Letter-spacing literal preserved verbatim, e.g. `"-0.025em"` or `"0"`.
    pub value: String,
}

/// Generic name/value token used by `space`, `radius`, `breakpoint`, and
/// the `motion.duration` sub-group. Values are CSS literals (`"0.25rem"`,
/// `"640px"`, `"150ms"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowToken {
    pub name: String,
    /// Full CSS `box-shadow` string for a single layer. Multi-layer
    /// (top-level comma outside parens) is rejected at lowering
    /// (`DESIGN-SHADOW-MULTI-LAYER`).
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
    /// `cubic-bezier(...)` quoted string or named CSS curve identifier.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZToken {
    pub name: String,
    pub value: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_design() -> Design {
        Design {
            name: "main".to_owned(),
            extends: None,
            colors: vec![ColorToken {
                name: "primary".to_owned(),
                states: vec![ColorState {
                    kind: ColorStateKind::Base,
                    value: "#7c3aed".to_owned(),
                    dark: Some("#a78bfa".to_owned()),
                }],
                span_ref: None,
            }],
            typography: Typography::default(),
            spaces: vec![ScaleToken {
                name: "sm".to_owned(),
                value: "0.5rem".to_owned(),
            }],
            radii: vec![],
            shadows: vec![],
            motion: Motion::default(),
            breakpoints: vec![],
            z_indices: vec![ZToken {
                name: "modal".to_owned(),
                value: 50,
            }],
            custom: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn design_round_trips_through_json() {
        let d = sample_design();
        let json = serde_json::to_value(&d).unwrap();
        let back: Design = serde_json::from_value(json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn color_state_kind_serializes_snake_case() {
        let value = serde_json::to_value(ColorStateKind::Foreground).unwrap();
        assert_eq!(value, json!("foreground"));
    }

    #[test]
    fn design_omits_empty_groups() {
        let d = sample_design();
        let value = serde_json::to_value(&d).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("radii"));
        assert!(!obj.contains_key("shadows"));
        assert!(!obj.contains_key("breakpoints"));
        assert!(!obj.contains_key("custom"));
        assert!(!obj.contains_key("extends"));
        assert!(!obj.contains_key("span_ref"));
    }

    #[test]
    fn custom_token_omits_dark_when_unset() {
        let c = CustomToken {
            name: "brand-green".to_owned(),
            base: "#16a34a".to_owned(),
            dark: None,
            span_ref: None,
        };
        let value = serde_json::to_value(&c).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("dark"));
        assert!(!obj.contains_key("span_ref"));
    }

    #[test]
    fn typography_default_is_empty() {
        let t = Typography::default();
        let value = serde_json::to_value(&t).unwrap();
        let obj = value.as_object().unwrap();
        assert!(obj.is_empty(), "Typography default should serialize empty");
    }
}
