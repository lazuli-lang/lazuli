//! Design token lowering — the visual vocabulary of a feature.
//!
//! ## Role in the pipeline
//!
//! Lifts a `syntax::DesignDeclAst` (the parsed `design <Theme> ...`
//! block) onto `ir::Design`. The IR carries nine closed-catalog
//! groups: colors, typography (families + scale + weights + tracking),
//! spaces, radii, shadows, motion (durations + easings), breakpoints,
//! z-indices, plus the `custom` meta-group (proposal
//! `docs/proposals/design-tokens-custom.md`).
//!
//! Lowering does the cheapest possible validation per group:
//!
//! * `colors` — closed state catalog (`base` | `hover` | `active` |
//!   `foreground`) + hex validation for both base and `.dark` slots.
//! * `weight` — `u16` parse (typography weights).
//! * `shadow` — reject top-level commas (single-layer only in v0;
//!   multi-layer composition stays declarative via token composition
//!   at the component level).
//! * `z` — `i32` parse.
//! * `custom` — hex validation deferred to doctor
//!   (`design-custom-invalid-value`) so the proposal-pending
//!   diagnostic ships as a doctor rule rather than an analyzer
//!   rejection; lowering preserves the verbatim value.
//! * `extends` — reserved for Cut B (post-pilot); v0 always rejects.
//!
//! Other groups (`spaces`, `radii`, `breakpoints`, `motion.durations`,
//! `motion.easings`, `typography.scale`, `typography.tracking`,
//! `typography.families`) are mechanical 1:1 field copies — they
//! carry no per-token validation here because the parser already
//! rejects malformed shapes upstream.
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::DesignDeclAst`, `ColorTokenAst`,
//!   `WeightTokenAst`, `ShadowTokenAst`, `ZTokenAst`,
//!   `CustomTokenAst`.
//! * Output: `lazuli_ir::Design`, `ColorToken`, `ColorState`,
//!   `Typography`, `WeightToken`, `ShadowToken`, `ZToken`,
//!   `CustomToken`, `ScaleToken`, `EasingToken`.
//! * Diagnostics: `DesignExtendsCutB`, `DesignColorStateUnknown`,
//!   `DesignColorHexInvalid`, `DesignWeightInvalid`,
//!   `DesignShadowMultiLayer`, `DesignZInvalid`.
//!
//! ## ABI guarantee
//!
//! `lower_design` is `pub` (consumed by `lazuli_cli` via the canonical
//! `lazuli_analyzer::lower_design` path); per-token helpers stay
//! `pub(crate)`.

use crate::AnalyzeError;
use crate::helpers::{has_top_level_comma, is_valid_design_hex, span_of};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

// =============================================================================
// L0 #2 — Design tokens lowering.
//
// `lower_design` validates each closed-catalog group:
//   * color states map to `ir::ColorStateKind` (closed catalog of 4).
//   * hex literals match `#[0-9a-fA-F]{3,8}` (3, 4, 6, or 8 hex digits).
//   * shadow values reject top-level commas (multi-layer composition).
//   * weight values parse as `u16`.
//   * z values parse as `i32`.
//   * `extends` is reserved for Cut B — v0 always rejects.
// =============================================================================

pub fn lower_design(ast: &syntax::DesignDeclAst) -> Result<ir::Design, AnalyzeError> {
    if let Some(target) = ast.extends.as_deref() {
        return Err(AnalyzeError::DesignExtendsCutB {
            target: target.to_owned(),
        });
    }

    let colors = ast
        .colors
        .iter()
        .map(lower_design_color_token)
        .collect::<Result<Vec<_>, _>>()?;

    let typography = ir::Typography {
        families: ast
            .typography
            .families
            .iter()
            .map(|f| ir::FamilyToken {
                name: f.name.clone(),
                value: f.value.clone(),
            })
            .collect(),
        scale: ast
            .typography
            .scale
            .iter()
            .map(|s| ir::TextScaleToken {
                name: s.name.clone(),
                size: s.size.clone(),
                line_height: s.line_height.clone(),
            })
            .collect(),
        weights: ast
            .typography
            .weights
            .iter()
            .map(lower_design_weight)
            .collect::<Result<Vec<_>, _>>()?,
        tracking: ast
            .typography
            .tracking
            .iter()
            .map(|t| ir::TrackingToken {
                name: t.name.clone(),
                value: t.value.clone(),
            })
            .collect(),
    };

    let spaces = ast
        .spaces
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let radii = ast
        .radii
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let shadows = ast
        .shadows
        .iter()
        .map(lower_design_shadow)
        .collect::<Result<Vec<_>, _>>()?;
    let motion = ir::Motion {
        durations: ast
            .motion
            .durations
            .iter()
            .map(|s| ir::ScaleToken {
                name: s.name.clone(),
                value: s.value.clone(),
            })
            .collect(),
        easings: ast
            .motion
            .easings
            .iter()
            .map(|e| ir::EasingToken {
                name: e.name.clone(),
                value: e.value.clone(),
            })
            .collect(),
    };
    let breakpoints = ast
        .breakpoints
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let z_indices = ast
        .z_indices
        .iter()
        .map(lower_design_z)
        .collect::<Result<Vec<_>, _>>()?;

    // L0 #2 — `custom` 9th meta-group per
    // `docs/proposals/design-tokens-custom.md`. Validate hex values here;
    // collision + reserved-name policy is enforced by doctor (the analyzer
    // stays surface-thin to keep proposal-pending diagnostics in doctor).
    let custom = ast
        .custom
        .iter()
        .map(lower_design_custom_token)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ir::Design {
        name: ast.name.clone(),
        extends: None,
        colors,
        typography,
        spaces,
        radii,
        shadows,
        motion,
        breakpoints,
        z_indices,
        custom,
        span_ref: Some(span_of(ast.span)),
    })
}

pub(crate) fn lower_design_custom_token(
    token: &syntax::CustomTokenAst,
) -> Result<ir::CustomToken, AnalyzeError> {
    // Hex validation for the `custom` 9th meta-group is intentionally
    // delegated to doctor (`design-custom-invalid-value`) so the
    // proposal-pending diagnostic surface ships as a doctor rule rather
    // than a hard analyzer rejection. See
    // `docs/proposals/design-tokens-custom.md` §4. Lowering preserves the
    // verbatim value so doctor can produce an actionable error message.
    Ok(ir::CustomToken {
        name: token.name.clone(),
        base: token.value.clone(),
        dark: token.dark.clone(),
        span_ref: Some(span_of(token.span)),
    })
}

pub(crate) fn lower_design_color_token(
    token: &syntax::ColorTokenAst,
) -> Result<ir::ColorToken, AnalyzeError> {
    let mut states = Vec::with_capacity(token.states.len());
    for state in &token.states {
        let kind = match state.kind.as_str() {
            "base" => ir::ColorStateKind::Base,
            "hover" => ir::ColorStateKind::Hover,
            "active" => ir::ColorStateKind::Active,
            "foreground" => ir::ColorStateKind::Foreground,
            other => {
                return Err(AnalyzeError::DesignColorStateUnknown {
                    token: token.name.clone(),
                    state: other.to_owned(),
                });
            }
        };
        if !is_valid_design_hex(&state.value) {
            return Err(AnalyzeError::DesignColorHexInvalid {
                token: token.name.clone(),
                state: state.kind.clone(),
                value: state.value.clone(),
            });
        }
        if let Some(dark) = state.dark.as_deref() {
            if !is_valid_design_hex(dark) {
                return Err(AnalyzeError::DesignColorHexInvalid {
                    token: token.name.clone(),
                    state: format!("{}.dark", state.kind),
                    value: dark.to_owned(),
                });
            }
        }
        states.push(ir::ColorState {
            kind,
            value: state.value.clone(),
            dark: state.dark.clone(),
        });
    }
    Ok(ir::ColorToken {
        name: token.name.clone(),
        states,
        span_ref: Some(span_of(token.span)),
    })
}

pub(crate) fn lower_design_weight(
    weight: &syntax::WeightTokenAst,
) -> Result<ir::WeightToken, AnalyzeError> {
    let parsed =
        weight
            .value
            .trim()
            .parse::<u16>()
            .map_err(|_| AnalyzeError::DesignWeightInvalid {
                name: weight.name.clone(),
                value: weight.value.clone(),
            })?;
    Ok(ir::WeightToken {
        name: weight.name.clone(),
        value: parsed,
    })
}

pub(crate) fn lower_design_shadow(
    shadow: &syntax::ShadowTokenAst,
) -> Result<ir::ShadowToken, AnalyzeError> {
    if has_top_level_comma(&shadow.value) {
        return Err(AnalyzeError::DesignShadowMultiLayer {
            name: shadow.name.clone(),
        });
    }
    Ok(ir::ShadowToken {
        name: shadow.name.clone(),
        value: shadow.value.clone(),
    })
}

pub(crate) fn lower_design_z(z: &syntax::ZTokenAst) -> Result<ir::ZToken, AnalyzeError> {
    let parsed = z
        .value
        .trim()
        .parse::<i32>()
        .map_err(|_| AnalyzeError::DesignZInvalid {
            name: z.name.clone(),
            value: z.value.clone(),
        })?;
    Ok(ir::ZToken {
        name: z.name.clone(),
        value: parsed,
    })
}
