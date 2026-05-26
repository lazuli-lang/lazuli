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
mod tests;

