//! `allowlist.json` emitter — JSON catalog of legal Tailwind utility classes
//! derived from `design.lzi`. Doctor's `design-token-undefined` rule reads
//! this to decide whether a `.tsx` class like `bg-primary` is valid; see
//! `docs/proposals/design-tokens.md` §6.1.
//!
//! Output shape (per `lazuli_doctor::design::helpers::Allowlist` — bare
//! suffixes, NOT full class names. Each bucket key encodes the prefix; the
//! values are the declared token suffixes that complete it.):
//!   {
//!     "bg":          ["primary", "primary-hover", ...],
//!     "text":        ["primary-foreground", ...],
//!     "p":           ["1", "2", ...],
//!     "px"..."pl":   [...],
//!     "m":           [...],
//!     "mx"..."ml":   [...],
//!     "gap":         [...],
//!     "gap-x"/"gap-y": [...],
//!     "rounded":     ["DEFAULT", "sm", "md", ...],
//!     "shadow":      ["DEFAULT", "sm", ...],
//!     "z":           ["docked", ...],
//!     "font":        ["sans", "mono", "regular", ...],
//!     "text-size":   ["xs", "base", ...]
//!   }
//!
//! The `base` token in `radius`/`shadow` maps to the Tailwind `DEFAULT`
//! slot, which `design-token-undefined` looks up when it sees a bare
//! `rounded` or `shadow` class.
//!
//! Determinism: every utility list sorts alphabetically. JSON is pretty
//! printed with two-space indent (no external crate — we render by hand to
//! stay in this crate's no-new-dep envelope).

use std::fmt::Write;

use super::ir::Design;

/// Emit `dist/ts-web/design/allowlist.json` for the given `Design`.
pub fn emit_allowlist_json(design: &Design) -> String {
    // -------- color utilities --------
    let mut bg: Vec<String> = Vec::new();
    let mut text_color: Vec<String> = Vec::new();
    let mut border_color: Vec<String> = Vec::new();
    let mut ring_color: Vec<String> = Vec::new();

    for color in &design.colors {
        // Bare suffixes — the bucket key (`bg`/`text`/...) already encodes
        // the Tailwind prefix. `bg-<name>` is recognised when `<name>` is in
        // the `bg` bucket; `bg-<name>-<state>` when `<name>-<state>` is in
        // the bucket.
        let base = color.name.clone();
        bg.push(base.clone());
        text_color.push(base.clone());
        border_color.push(base.clone());
        ring_color.push(base.clone());

        for state in &color.states {
            // Skip the implicit DEFAULT — already covered by the bare name.
            let state_name = match state.kind {
                super::ir::ColorStateKind::Base => continue,
                super::ir::ColorStateKind::Hover => "hover",
                super::ir::ColorStateKind::Active => "active",
                super::ir::ColorStateKind::Foreground => "foreground",
            };
            let suffix = format!("{}-{}", base, state_name);
            bg.push(suffix.clone());
            text_color.push(suffix.clone());
            border_color.push(suffix.clone());
            ring_color.push(suffix);
        }
    }

    // Custom tokens (9th meta-group). Each token populates the four
    // color buckets (`bg`/`text`/`border`/`ring`) with the bare token
    // name. No state suffixes — custom is intentionally flat per
    // `docs/proposals/design-tokens-custom.md` §5.4.
    for tok in &design.custom {
        bg.push(tok.name.clone());
        text_color.push(tok.name.clone());
        border_color.push(tok.name.clone());
        ring_color.push(tok.name.clone());
    }

    // -------- spacing utilities --------
    // All spacing prefixes (`p`, `px`, `m`, `gap`, ...) share the same set
    // of declared space names. We emit the bare token names per bucket;
    // the bucket key encodes the prefix.
    let space_names: Vec<String> = design.spaces.iter().map(|t| t.name.clone()).collect();
    let p = space_names.clone();
    let px = space_names.clone();
    let py = space_names.clone();
    let pt = space_names.clone();
    let pr = space_names.clone();
    let pb = space_names.clone();
    let pl = space_names.clone();
    let m = space_names.clone();
    let mx = space_names.clone();
    let my = space_names.clone();
    let mt = space_names.clone();
    let mr = space_names.clone();
    let mb = space_names.clone();
    let ml = space_names.clone();
    let gap = space_names.clone();
    let gap_x = space_names.clone();
    let gap_y = space_names.clone();

    // -------- radius / shadow / z / font / text-size --------
    // For `rounded`/`shadow`, the token named `base` maps to Tailwind's
    // `DEFAULT` slot — `design-token-undefined` looks up the bare
    // `rounded` / `shadow` class against the `DEFAULT` key.
    let rounded = suffixes_with_default(
        &design.radii.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    );
    let shadow = suffixes_with_default(
        &design.shadows.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
    );
    let z: Vec<String> = design.z_indices.iter().map(|t| t.name.clone()).collect();

    // font: families + weights both live under the `font-*` prefix in
    // Tailwind, so we collapse them into a single `font` bucket as bare
    // suffixes.
    let mut font: Vec<String> = Vec::new();
    for fam in &design.typography.families {
        font.push(fam.name.clone());
    }
    for w in &design.typography.weights {
        font.push(w.name.clone());
    }

    // text-size: typography scale names — bare suffixes under `text-size`
    // (Doctor's `match_prefix` maps `text-<name>` against either the
    // `text` color bucket or the `text-size` scale bucket; the bucket
    // membership decides which token kind is in play).
    let text_size: Vec<String> = design
        .typography
        .scale
        .iter()
        .map(|s| s.name.clone())
        .collect();

    // Sort each list deterministically (lexicographic).
    let mut groups: Vec<(&str, Vec<String>)> = vec![
        ("bg", bg),
        ("text", text_color),
        ("border", border_color),
        ("ring", ring_color),
        ("p", p),
        ("px", px),
        ("py", py),
        ("pt", pt),
        ("pr", pr),
        ("pb", pb),
        ("pl", pl),
        ("m", m),
        ("mx", mx),
        ("my", my),
        ("mt", mt),
        ("mr", mr),
        ("mb", mb),
        ("ml", ml),
        ("gap", gap),
        ("gap-x", gap_x),
        ("gap-y", gap_y),
        ("rounded", rounded),
        ("shadow", shadow),
        ("z", z),
        ("font", font),
        ("text-size", text_size),
    ];
    for (_, list) in groups.iter_mut() {
        list.sort();
        list.dedup();
    }

    // -------- render JSON by hand (two-space indent) --------
    let mut s = String::new();
    writeln!(s, "{{").ok();
    for (idx, (key, list)) in groups.iter().enumerate() {
        let comma = if idx + 1 == groups.len() { "" } else { "," };
        if list.is_empty() {
            writeln!(s, "  \"{}\": []{}", key, comma).ok();
            continue;
        }
        writeln!(s, "  \"{}\": [", key).ok();
        for (i, item) in list.iter().enumerate() {
            let inner_comma = if i + 1 == list.len() { "" } else { "," };
            writeln!(s, "    \"{}\"{}", json_escape(item), inner_comma).ok();
        }
        writeln!(s, "  ]{}", comma).ok();
    }
    writeln!(s, "}}").ok();
    s
}

/// Convert a list of token names into bare allowlist suffixes, mapping the
/// conventional `base` token to Tailwind's `DEFAULT` slot. Used by
/// `rounded` / `shadow` where Doctor (`design-token-undefined`) looks up
/// `"DEFAULT"` when it sees a bare `rounded` or `shadow` class with no
/// suffix.
fn suffixes_with_default(names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| {
            if n == "base" {
                "DEFAULT".to_owned()
            } else {
                n.clone()
            }
        })
        .collect()
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::ir::{
        ColorState, ColorStateKind, ColorToken, Design, FamilyToken, Motion, ScaleToken,
        ShadowToken, TextScaleToken, Typography, WeightToken, ZToken,
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
            "bg", "text", "border", "ring", "p", "px", "py", "pt", "pr", "pb", "pl", "m", "mx",
            "my", "mt", "mr", "mb", "ml", "gap", "gap-x", "gap-y", "rounded", "shadow", "z",
            "font", "text-size",
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
        assert!(rounded.contains(&"DEFAULT"), "rounded should contain DEFAULT: {rounded:?}");
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
        let parsed: serde_json::Value =
            serde_json::from_str(&out).expect("valid JSON");
        for prefix in ["bg", "text", "border", "ring"] {
            let bucket = parsed
                .get(prefix)
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("missing prefix `{prefix}`"));
            let entries: Vec<&str> =
                bucket.iter().filter_map(|v| v.as_str()).collect();
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
}
