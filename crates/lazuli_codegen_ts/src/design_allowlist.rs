//! `allowlist.json` emitter — JSON catalog of legal Tailwind utility classes
//! derived from `design.lzi`. Doctor's `design-token-undefined` rule reads
//! this to decide whether a `.tsx` class like `bg-primary` is valid; see
//! `docs/proposals/design-tokens.md` §6.1.
//!
//! Output shape (per the cell prompt):
//!   {
//!     "bg":          ["bg-primary", "bg-primary-hover", ...],
//!     "text":        ["text-primary-foreground", ...],
//!     "p":           ["p-1", "p-2", ...],
//!     "px"..."pl":   [...],
//!     "m":           [...],
//!     "mx"..."ml":   [...],
//!     "gap":         [...],
//!     "gap-x"/"gap-y": [...],
//!     "rounded":     ["rounded-sm", "rounded", ...],
//!     "shadow":      [...],
//!     "z":           ["z-docked", ...],
//!     "font":        ["font-sans", "font-mono", "font-regular", ...],
//!     "text-size":   ["text-xs", "text-base", ...]
//!   }
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
        // Tailwind convention: `bg-<name>` for the default/base, and
        // `bg-<name>-<state>` for explicit non-base states.
        // The DEFAULT entry (no suffix) maps to the Base state in the preset.
        let base_class = color.name.clone();
        bg.push(format!("bg-{}", base_class));
        text_color.push(format!("text-{}", base_class));
        border_color.push(format!("border-{}", base_class));
        ring_color.push(format!("ring-{}", base_class));

        for state in &color.states {
            // Skip the implicit DEFAULT — already covered by the bare `bg-<name>`.
            let state_name = match state.kind {
                super::ir::ColorStateKind::Base => continue,
                super::ir::ColorStateKind::Hover => "hover",
                super::ir::ColorStateKind::Active => "active",
                super::ir::ColorStateKind::Foreground => "foreground",
            };
            bg.push(format!("bg-{}-{}", base_class, state_name));
            text_color.push(format!("text-{}-{}", base_class, state_name));
            border_color.push(format!("border-{}-{}", base_class, state_name));
            ring_color.push(format!("ring-{}-{}", base_class, state_name));
        }
    }

    // -------- spacing utilities --------
    let space_names: Vec<String> = design.spaces.iter().map(|t| t.name.clone()).collect();
    let p = utilities("p", &space_names);
    let px = utilities("px", &space_names);
    let py = utilities("py", &space_names);
    let pt = utilities("pt", &space_names);
    let pr = utilities("pr", &space_names);
    let pb = utilities("pb", &space_names);
    let pl = utilities("pl", &space_names);
    let m = utilities("m", &space_names);
    let mx = utilities("mx", &space_names);
    let my = utilities("my", &space_names);
    let mt = utilities("mt", &space_names);
    let mr = utilities("mr", &space_names);
    let mb = utilities("mb", &space_names);
    let ml = utilities("ml", &space_names);
    let gap = utilities("gap", &space_names);
    let gap_x = utilities("gap-x", &space_names);
    let gap_y = utilities("gap-y", &space_names);

    // -------- radius / shadow / z / font / text-size --------
    let rounded = utilities_with_default("rounded", &design.radii.iter().map(|r| r.name.clone()).collect::<Vec<_>>());
    let shadow = utilities_with_default("shadow", &design.shadows.iter().map(|s| s.name.clone()).collect::<Vec<_>>());
    let z = utilities("z", &design.z_indices.iter().map(|t| t.name.clone()).collect::<Vec<_>>());

    // font: families + weights all live under the `font-*` prefix in Tailwind.
    let mut font: Vec<String> = Vec::new();
    for fam in &design.typography.families {
        font.push(format!("font-{}", fam.name));
    }
    for w in &design.typography.weights {
        font.push(format!("font-{}", w.name));
    }

    // text-size: typography scale names — emit as `text-<name>`.
    let text_size: Vec<String> = design
        .typography
        .scale
        .iter()
        .map(|s| format!("text-{}", s.name))
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

fn utilities(prefix: &str, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| format!("{}-{}", prefix, n))
        .collect()
}

/// Like `utilities`, but the token named `base` collapses to the bare prefix
/// (Tailwind's DEFAULT key). E.g. `radius.base` → `rounded`,
/// `shadow.base` → `shadow`.
fn utilities_with_default(prefix: &str, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| {
            if n == "base" {
                prefix.to_owned()
            } else {
                format!("{}-{}", prefix, n)
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
        // Single-base color → only the bare `bg-success` (no state suffixes).
        assert!(out.contains("\"bg-success\""));
        assert!(!out.contains("bg-success-hover"));
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
        assert!(out.contains("\"bg-primary\""));
        assert!(out.contains("\"bg-primary-hover\""));
        assert!(out.contains("\"bg-primary-foreground\""));
        assert!(out.contains("\"text-primary-foreground\""));
        assert!(out.contains("\"border-primary-hover\""));
    }

    #[test]
    fn radius_base_collapses_to_rounded_default() {
        let out = emit_allowlist_json(&minimal());
        // `radius.base` → bare `rounded` (no suffix).
        assert!(out.contains("\"rounded\""));
        assert!(!out.contains("rounded-base"));
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
        assert!(out.contains("\"shadow\""));
        assert!(out.contains("\"shadow-lg\""));
        assert!(!out.contains("shadow-base"));
    }

    #[test]
    fn font_lists_family_and_weight() {
        let out = emit_allowlist_json(&minimal());
        assert!(out.contains("\"font-sans\""));
        assert!(out.contains("\"font-bold\""));
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
        assert!(out.contains("\"text-base\""));
        assert!(out.contains("\"text-2xl\""));
    }

    #[test]
    fn spaces_propagate_across_all_spacing_prefixes() {
        let out = emit_allowlist_json(&minimal());
        for prefix in [
            "p", "px", "py", "pt", "pr", "pb", "pl", "m", "mx", "my", "mt", "mr", "mb", "ml", "gap",
        ] {
            assert!(
                out.contains(&format!("\"{}-4\"", prefix)),
                "missing `{prefix}-4` in allowlist"
            );
            assert!(
                out.contains(&format!("\"{}-8\"", prefix)),
                "missing `{prefix}-8` in allowlist"
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
        let docked = out.find("\"z-docked\"").expect("docked present");
        let modal = out.find("\"z-modal\"").expect("modal present");
        assert!(docked < modal, "z-docked should sort before z-modal");
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
