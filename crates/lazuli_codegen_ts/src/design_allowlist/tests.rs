//! Allowlist-JSON emitter tests — kept verbatim from the original
//! `design_allowlist.rs` inline test module.

use super::*;
use crate::design::ir::{
    ColorState, ColorStateKind, ColorToken, Design, FamilyToken, Motion, ScaleToken, ShadowToken,
    TextScaleToken, Typography, WeightToken, ZToken,
};
use lazuli_ir::CustomToken;

fn minimal() -> Design {
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
                value: "Inter, system-ui".to_owned(),
            }],
            scale: vec![TextScaleToken {
                name: "base".to_owned(),
                size: "1rem".to_owned(),
                line_height: "1.5rem".to_owned(),
            }],
            weights: vec![WeightToken {
                name: "bold".to_owned(),
                value: 700,
            }],
            tracking: vec![],
        },
        spaces: vec![
            ScaleToken {
                name: "4".to_owned(),
                value: "1rem".to_owned(),
            },
            ScaleToken {
                name: "8".to_owned(),
                value: "2rem".to_owned(),
            },
        ],
        radii: vec![ScaleToken {
            name: "base".to_owned(),
            value: "0.25rem".to_owned(),
        }],
        shadows: vec![],
        motion: Motion::default(),
        breakpoints: vec![],
        z_indices: vec![],
        custom: vec![],
        span_ref: None,
    }
}

#[test]
fn minimal_emits_all_top_level_keys() {
    let out = emit_allowlist_json(&minimal());
    assert!(out.starts_with("{\n"));
    for key in [
        "bg",
        "text",
        "border",
        "ring",
        "p",
        "px",
        "py",
        "pt",
        "pr",
        "pb",
        "pl",
        "m",
        "mx",
        "my",
        "mt",
        "mr",
        "mb",
        "ml",
        "gap",
        "gap-x",
        "gap-y",
        "rounded",
        "shadow",
        "z",
        "font",
        "text-size",
    ] {
        assert!(out.contains(&format!("\"{}\"", key)), "missing key {key}");
    }
    // Single-base color → only the bare `success` suffix (no state
    // suffixes). The bucket key (`bg`) encodes the `bg-` prefix.
    assert!(out.contains("\"success\""));
    assert!(!out.contains("success-hover"));
}

#[test]
fn multi_state_color_emits_each_state_class() {
    let mut d = minimal();
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
    let out = emit_allowlist_json(&d);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let bg: Vec<&str> = parsed["bg"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(bg.contains(&"primary"));
    assert!(bg.contains(&"primary-hover"));
    assert!(bg.contains(&"primary-foreground"));
    let text: Vec<&str> = parsed["text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(text.contains(&"primary-foreground"));
    let border: Vec<&str> = parsed["border"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(border.contains(&"primary-hover"));
}

#[test]
fn radius_base_collapses_to_rounded_default() {
    let out = emit_allowlist_json(&minimal());
    // `radius.base` → `DEFAULT` slot — Doctor looks up "DEFAULT" when
    // it sees a bare `rounded` class.
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let rounded: Vec<&str> = parsed["rounded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        rounded.contains(&"DEFAULT"),
        "rounded should contain DEFAULT: {rounded:?}"
    );
    assert!(!rounded.contains(&"base"), "literal `base` must not leak");
}

#[test]
fn empty_motion_easings_no_crash() {
    let out = emit_allowlist_json(&minimal());
    // Sanity check — emission completes for the minimal fixture (motion is empty).
    assert!(out.ends_with("}\n") || out.ends_with("}"));
}

#[test]
fn shadow_base_collapses_to_bare_shadow() {
    let mut d = minimal();
    d.shadows.push(ShadowToken {
        name: "base".to_owned(),
        value: "0 1px 3px #000".to_owned(),
    });
    d.shadows.push(ShadowToken {
        name: "lg".to_owned(),
        value: "0 10px 15px #000".to_owned(),
    });
    let out = emit_allowlist_json(&d);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let shadow: Vec<&str> = parsed["shadow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(shadow.contains(&"DEFAULT"));
    assert!(shadow.contains(&"lg"));
    assert!(!shadow.contains(&"base"));
}

#[test]
fn font_lists_family_and_weight() {
    let out = emit_allowlist_json(&minimal());
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let font: Vec<&str> = parsed["font"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(font.contains(&"sans"));
    assert!(font.contains(&"bold"));
}

#[test]
fn text_size_uses_scale_names() {
    let mut d = minimal();
    d.typography.scale.push(TextScaleToken {
        name: "2xl".to_owned(),
        size: "1.5rem".to_owned(),
        line_height: "2rem".to_owned(),
    });
    let out = emit_allowlist_json(&d);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let text_size: Vec<&str> = parsed["text-size"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(text_size.contains(&"base"));
    assert!(text_size.contains(&"2xl"));
}

#[test]
fn spaces_propagate_across_all_spacing_prefixes() {
    let out = emit_allowlist_json(&minimal());
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    for prefix in [
        "p", "px", "py", "pt", "pr", "pb", "pl", "m", "mx", "my", "mt", "mr", "mb", "ml", "gap",
    ] {
        let bucket: Vec<&str> = parsed[prefix]
            .as_array()
            .unwrap_or_else(|| panic!("missing bucket `{prefix}`"))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            bucket.contains(&"4"),
            "missing `4` in `{prefix}` bucket: {bucket:?}"
        );
        assert!(
            bucket.contains(&"8"),
            "missing `8` in `{prefix}` bucket: {bucket:?}"
        );
    }
}

#[test]
fn sort_is_lexicographic_within_each_group() {
    let mut d = minimal();
    // Insert in scrambled order; allowlist must sort.
    d.z_indices.push(ZToken {
        name: "modal".to_owned(),
        value: 1300,
    });
    d.z_indices.push(ZToken {
        name: "docked".to_owned(),
        value: 10,
    });
    let out = emit_allowlist_json(&d);
    let docked = out.find("\"docked\"").expect("docked present");
    let modal = out.find("\"modal\"").expect("modal present");
    assert!(docked < modal, "docked should sort before modal");
}

#[test]
fn custom_tokens_expand_to_four_utility_prefixes() {
    // Z2 — each `custom` token populates the bg/text/border/ring buckets.
    let mut d = minimal();
    d.custom.push(CustomToken {
        name: "chat-bubble-mine".to_owned(),
        base: "#dcf8c6".to_owned(),
        dark: None,
        span_ref: None,
    });
    d.custom.push(CustomToken {
        name: "map-marker-active".to_owned(),
        base: "#ff5722".to_owned(),
        dark: Some("#7c2410".to_owned()),
        span_ref: None,
    });
    let out = emit_allowlist_json(&d);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    for prefix in ["bg", "text", "border", "ring"] {
        let bucket = parsed
            .get(prefix)
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("missing prefix `{prefix}`"));
        let entries: Vec<&str> = bucket.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            entries.contains(&"chat-bubble-mine"),
            "{prefix} missing chat-bubble-mine: {entries:?}"
        );
        assert!(
            entries.contains(&"map-marker-active"),
            "{prefix} missing map-marker-active: {entries:?}"
        );
    }
}

#[test]
fn output_is_valid_json_round_trip() {
    let out = emit_allowlist_json(&minimal());
    // serde_json is in the deps for this crate (lib.rs uses it); use it
    // to verify the emitted text is parseable.
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let obj = parsed.as_object().expect("top-level object");
    assert!(obj.contains_key("bg"));
    assert!(obj.contains_key("text-size"));
    // dedup invariant: no duplicates in `font`.
    let font = obj.get("font").and_then(|v| v.as_array()).unwrap();
    let mut copy: Vec<String> = font
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    copy.sort();
    copy.dedup();
    assert_eq!(copy.len(), font.len(), "font has duplicates");
}
