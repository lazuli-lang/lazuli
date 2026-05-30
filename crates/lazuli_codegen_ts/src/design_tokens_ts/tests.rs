//! `tokens.ts` emitter tests — kept verbatim from the original
//! `design_tokens_ts.rs` inline test module.

use super::*;
use crate::design::ir::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};
use lazuli_ir::CustomToken;

fn minimal_design() -> Design {
    Design {
        name: "tiny".to_owned(),
        extends: None,
        colors: vec![ColorToken {
            name: "success".to_owned(),
            states: vec![ColorState {
                kind: ColorStateKind::Base,
                value: "#16a34a".to_owned(),
                dark: None,
            }],
            span_ref: None,
        }],
        typography: Typography {
            families: vec![FamilyToken {
                name: "sans".to_owned(),
                value: "Inter, sans-serif".to_owned(),
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
        breakpoints: vec![],
        z_indices: vec![],
        custom: vec![],
        span_ref: None,
    }
}

#[test]
fn minimal_fixture_emits_single_color_flat() {
    let out = emit_tokens_ts(&minimal_design());
    assert!(out.contains("success: \"#16a34a\","));
    assert!(out.contains("sans: \"Inter, sans-serif\","));
    // "4" must stay quoted because it starts with a digit.
    assert!(out.contains("\"4\": \"1rem\","));
    assert!(out.contains("export type ColorToken = keyof typeof tokens.color;"));
}

#[test]
fn multi_state_color_emits_nested_object() {
    let mut d = minimal_design();
    d.colors.push(ColorToken {
        name: "primary".to_owned(),
        states: vec![
            ColorState {
                kind: ColorStateKind::Base,
                value: "#7c3aed".to_owned(),
                dark: None,
            },
            ColorState {
                kind: ColorStateKind::Hover,
                value: "#6d28d9".to_owned(),
                dark: None,
            },
            ColorState {
                kind: ColorStateKind::Foreground,
                value: "#ffffff".to_owned(),
                dark: None,
            },
        ],
        span_ref: None,
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("primary: {"));
    assert!(out.contains("base: \"#7c3aed\","));
    assert!(out.contains("hover: \"#6d28d9\","));
    assert!(out.contains("foreground: \"#ffffff\","));
    // Must NOT have light/dark nesting for this token.
    assert!(!out.contains("light: \"#7c3aed\""));
}

#[test]
fn dark_variant_promotes_states_to_light_dark_objects() {
    let mut d = minimal_design();
    d.colors.push(ColorToken {
        name: "background".to_owned(),
        states: vec![
            ColorState {
                kind: ColorStateKind::Base,
                value: "#ffffff".to_owned(),
                dark: Some("#09090b".to_owned()),
            },
            ColorState {
                // No dark — should stay as light only (single value) inside the dark-shape.
                kind: ColorStateKind::Foreground,
                value: "#09090b".to_owned(),
                dark: None,
            },
        ],
        span_ref: None,
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("background: {"));
    assert!(out.contains("base: { light: \"#ffffff\", dark: \"#09090b\" },"));
    // The state with no dark stays as a single string in the multi-state-with-dark shape.
    assert!(out.contains("foreground: \"#09090b\","));
}

#[test]
fn empty_groups_do_not_crash() {
    let d = Design {
        name: "empty".to_owned(),
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
    let out = emit_tokens_ts(&d);
    // Should still have the wrapper + all group keys present, just empty bodies.
    assert!(out.contains("color: {"));
    assert!(out.contains("typography: {"));
    assert!(out.contains("space: {"));
    assert!(out.contains("} as const;"));
}

#[test]
fn z_index_emits_as_number_not_string() {
    let mut d = minimal_design();
    d.z_indices.push(ZToken {
        name: "modal".to_owned(),
        value: 1300,
    });
    let out = emit_tokens_ts(&d);
    // Number literal — no quotes.
    assert!(out.contains("modal: 1300,"));
    assert!(!out.contains("\"1300\""));
}

#[test]
fn shadow_string_with_inner_quotes_does_not_escape_badly() {
    let mut d = minimal_design();
    d.shadows.push(ShadowToken {
        name: "sm".to_owned(),
        value: "0 1px 2px 0 rgb(0 0 0 / 0.05)".to_owned(),
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("sm: \"0 1px 2px 0 rgb(0 0 0 / 0.05)\","));
}

#[test]
fn motion_easing_with_cubic_bezier_preserved() {
    let mut d = minimal_design();
    d.motion.easings.push(EasingToken {
        name: "out".to_owned(),
        value: "cubic-bezier(0, 0, 0.2, 1)".to_owned(),
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("out: \"cubic-bezier(0, 0, 0.2, 1)\","));
}

#[test]
fn quoted_scale_name_is_preserved() {
    let mut d = minimal_design();
    d.typography.scale.push(TextScaleToken {
        name: "2xl".to_owned(),
        size: "1.5rem".to_owned(),
        line_height: "2rem".to_owned(),
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("\"2xl\": { size: \"1.5rem\", lineHeight: \"2rem\" },"));
}

#[test]
fn custom_token_emits_as_camelcased_sibling_under_color() {
    // Z2 — `custom` lowers as a sibling under `tokens.color`, kebab→camel.
    let mut d = minimal_design();
    d.custom.push(CustomToken {
        name: "chat-bubble-mine".to_owned(),
        base: "#dcf8c6".to_owned(),
        dark: None,
        span_ref: None,
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("chatBubbleMine: { base: \"#dcf8c6\" },"));
}

#[test]
fn custom_token_with_dark_emits_base_dark_pair() {
    let mut d = minimal_design();
    d.custom.push(CustomToken {
        name: "chat-bubble-mine".to_owned(),
        base: "#dcf8c6".to_owned(),
        dark: Some("#005c4b".to_owned()),
        span_ref: None,
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("chatBubbleMine: { base: \"#dcf8c6\", dark: \"#005c4b\" },"));
}

#[test]
fn custom_kebab_to_camel_handles_single_segment() {
    // Single-word custom names pass through unchanged.
    let mut d = minimal_design();
    d.custom.push(CustomToken {
        name: "highlight".to_owned(),
        base: "#fff".to_owned(),
        dark: None,
        span_ref: None,
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("highlight: { base: \"#fff\" },"));
}

#[test]
fn tracking_negative_em_value_preserved_as_string() {
    let mut d = minimal_design();
    d.typography.tracking.push(TrackingToken {
        name: "tight".to_owned(),
        value: "-0.025em".to_owned(),
    });
    let out = emit_tokens_ts(&d);
    assert!(out.contains("tight: \"-0.025em\","));
}
