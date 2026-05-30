//! Mobile design-tokens emitter tests. Kept verbatim from the original
//! `design_tokens_mobile.rs` test module.

use super::*;
use crate::design::ir::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken,
};

fn minimal() -> Design {
    Design {
        name: "tiny".to_owned(),
        extends: None,
        colors: vec![ColorToken {
            name: "primary".to_owned(),
            states: vec![ColorState {
                kind: ColorStateKind::Base,
                value: "#7c3aed".to_owned(),
                dark: None,
            }],
            span_ref: None,
        }],
        typography: Typography {
            families: vec![FamilyToken {
                name: "sans".to_owned(),
                value: "Inter, system-ui, sans-serif".to_owned(),
            }],
            scale: vec![TextScaleToken {
                name: "base".to_owned(),
                size: "1rem".to_owned(),
                line_height: "1.5rem".to_owned(),
            }],
            weights: vec![WeightToken {
                name: "regular".to_owned(),
                value: 400,
            }],
            tracking: vec![],
        },
        spaces: vec![ScaleToken {
            name: "4".to_owned(),
            value: "1rem".to_owned(),
        }],
        radii: vec![],
        shadows: vec![],
        motion: Motion::default(),
        breakpoints: vec![ScaleToken {
            name: "md".to_owned(),
            value: "768px".to_owned(),
        }],
        z_indices: vec![],
        custom: vec![],
        span_ref: None,
    }
}

#[test]
fn minimal_converts_rem_to_px_at_16() {
    let out = emit_tokens_mobile_ts(&minimal(), 16);
    // 1rem * 16 = 16
    assert!(out.contains("\"4\": 16,"));
    assert!(out.contains("base: { fontSize: 16, lineHeight: 24 }"));
    // Primary font uses first family only.
    assert!(out.contains("sans: \"Inter\","));
    // weight stays as string per RN convention.
    assert!(out.contains("regular: \"400\","));
    // breakpoint must NOT appear in mobile output.
    assert!(!out.contains("breakpoint:"));
    // z must NOT appear in mobile output.
    assert!(!out.contains("z: "));
}

#[test]
fn rem_base_18_changes_pixel_values() {
    let out = emit_tokens_mobile_ts(&minimal(), 18);
    assert!(out.contains("\"4\": 18,"));
    assert!(out.contains("fontSize: 18"));
    assert!(out.contains("lineHeight: 27")); // 1.5rem * 18 = 27
}

#[test]
fn shadow_parses_to_rn_object() {
    let mut d = minimal();
    d.shadows.push(ShadowToken {
        name: "base".to_owned(),
        value: "0 1px 3px 0 rgb(0 0 0 / 0.1)".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("base: {"));
    assert!(out.contains("shadowColor: \"#000000\","));
    assert!(out.contains("shadowOffset: { width: 0, height: 1 },"));
    assert!(out.contains("shadowOpacity: 0.1,"));
    assert!(out.contains("shadowRadius: 3,"));
    assert!(out.contains("elevation: "));
}

#[test]
fn dark_color_emits_light_dark_states() {
    let mut d = minimal();
    d.colors.push(ColorToken {
        name: "background".to_owned(),
        states: vec![ColorState {
            kind: ColorStateKind::Base,
            value: "#ffffff".to_owned(),
            dark: Some("#09090b".to_owned()),
        }],
        span_ref: None,
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("background: {"));
    assert!(out.contains("base: { light: \"#ffffff\", dark: \"#09090b\" },"));
}

#[test]
fn empty_design_emits_skeleton_no_crash() {
    let d = Design {
        name: "void".to_owned(),
        extends: None,
        colors: vec![],
        typography: Typography::default(),
        spaces: vec![],
        radii: vec![],
        shadows: vec![],
        motion: Motion::default(),
        breakpoints: vec![],
        z_indices: vec![],
        custom: vec![],
        span_ref: None,
    };
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("export const tokens = {"));
    assert!(out.contains("color: {"));
    assert!(out.contains("} as const;"));
}

#[test]
fn multi_layer_shadow_emits_todo_comment_and_placeholder() {
    let mut d = minimal();
    d.shadows.push(ShadowToken {
        name: "elevated".to_owned(),
        value: "0 1px 2px #000, 0 4px 6px #111".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("TODO(design-shadow-multi-layer)"));
    // Placeholder still emits the key so the file remains valid TS.
    assert!(out.contains("elevated: {"));
    assert!(out.contains("shadowOpacity: 0"));
}

#[test]
fn duration_strips_ms() {
    let mut d = minimal();
    d.motion.durations.push(ScaleToken {
        name: "fast".to_owned(),
        value: "150ms".to_owned(),
    });
    d.motion.durations.push(ScaleToken {
        name: "slow".to_owned(),
        value: "350ms".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("fast: 150,"));
    assert!(out.contains("slow: 350,"));
}

#[test]
fn easing_cubic_bezier_kept_as_string() {
    let mut d = minimal();
    d.motion.easings.push(EasingToken {
        name: "out".to_owned(),
        value: "cubic-bezier(0, 0, 0.2, 1)".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("out: \"cubic-bezier(0, 0, 0.2, 1)\","));
}

#[test]
fn hex_shadow_color_normalizes_to_six_digit_hex() {
    let mut d = minimal();
    d.shadows.push(ShadowToken {
        name: "soft".to_owned(),
        value: "0 2px 4px #abcdef".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("shadowColor: \"#abcdef\","));
    assert!(out.contains("shadowOffset: { width: 0, height: 2 },"));
    assert!(out.contains("shadowRadius: 4,"));
    // Hex without alpha → opacity 1.0.
    assert!(out.contains("shadowOpacity: 1.0,"));
}

#[test]
fn px_value_in_radius_strips_unit() {
    let mut d = minimal();
    d.radii.push(ScaleToken {
        name: "full".to_owned(),
        value: "9999px".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("full: 9999,"));
}

#[test]
fn rgba_with_comma_syntax_parses() {
    let mut d = minimal();
    d.shadows.push(ShadowToken {
        name: "alpha".to_owned(),
        value: "0 1px 2px rgba(0, 0, 0, 0.25)".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("shadowColor: \"#000000\","));
    assert!(out.contains("shadowOpacity: 0.25,"));
}

#[test]
fn tracking_unit_stripped_to_number() {
    let mut d = minimal();
    d.typography.tracking.push(TrackingToken {
        name: "tight".to_owned(),
        value: "-0.025em".to_owned(),
    });
    let out = emit_tokens_mobile_ts(&d, 16);
    assert!(out.contains("tight: -0.025,"));
}
