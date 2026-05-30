//! Flat-path token view + symmetric diff.
//!
//! `lazuli design diff --against <path>` compares the in-memory
//! `Design` IR against an external catalog by flattening each side
//! into a `BTreeMap<dotted-path, encoded-value>` and computing the
//! three-way set difference (added / removed / changed).
//!
//! The flat view is dialect-agnostic — both Figma and Style
//! Dictionary lower to the same `Design` IR before this code runs.
//! Color states with a single `Base` entry collapse to
//! `color.<name>`; multi-state tokens fan out to
//! `color.<name>.<state>` (matching the Figma sub-block shape).
//! Dark-mode variants are encoded as `<value>|dark=<dark>` so the
//! diff catches dark-only changes.

use std::collections::BTreeMap;

use super::figma::color_state_key;
use super::{ColorState, ColorStateKind, Design, DiffReport, TokenDiff};

pub(super) fn compute_diff(current: &Design, incoming: &Design) -> DiffReport {
    let lhs = flat_view(current);
    let rhs = flat_view(incoming);

    let mut report = DiffReport::default();
    for (path, value) in &rhs {
        match lhs.get(path) {
            None => report.added.push(path.clone()),
            Some(existing) if existing != value => report.changed.push(TokenDiff {
                path: path.clone(),
                from_value: existing.clone(),
                to_value: value.clone(),
            }),
            _ => {}
        }
    }
    for path in lhs.keys() {
        if !rhs.contains_key(path) {
            report.removed.push(path.clone());
        }
    }
    report.added.sort();
    report.removed.sort();
    report.changed.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

fn flat_view(design: &Design) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for tok in &design.colors {
        if tok.states.len() == 1 && tok.states[0].kind == ColorStateKind::Base {
            let state = &tok.states[0];
            out.insert(format!("color.{}", tok.name), encode_color(state));
        } else {
            for state in &tok.states {
                let key = format!("color.{}.{}", tok.name, color_state_key(state.kind));
                out.insert(key, encode_color(state));
            }
        }
    }
    for tok in &design.typography.families {
        out.insert(format!("typography.family.{}", tok.name), tok.value.clone());
    }
    for tok in &design.typography.scale {
        out.insert(
            format!("typography.scale.{}", tok.name),
            format!("{}|{}", tok.size, tok.line_height),
        );
    }
    for tok in &design.typography.weights {
        out.insert(
            format!("typography.weight.{}", tok.name),
            tok.value.to_string(),
        );
    }
    for tok in &design.typography.tracking {
        out.insert(
            format!("typography.tracking.{}", tok.name),
            tok.value.clone(),
        );
    }
    for tok in &design.spaces {
        out.insert(format!("space.{}", tok.name), tok.value.clone());
    }
    for tok in &design.radii {
        out.insert(format!("radius.{}", tok.name), tok.value.clone());
    }
    for tok in &design.shadows {
        out.insert(format!("shadow.{}", tok.name), tok.value.clone());
    }
    for tok in &design.motion.durations {
        out.insert(format!("motion.duration.{}", tok.name), tok.value.clone());
    }
    for tok in &design.motion.easings {
        out.insert(format!("motion.easing.{}", tok.name), tok.value.clone());
    }
    for tok in &design.breakpoints {
        out.insert(format!("breakpoint.{}", tok.name), tok.value.clone());
    }
    for tok in &design.z_indices {
        out.insert(format!("z.{}", tok.name), tok.value.to_string());
    }
    out
}

fn encode_color(state: &ColorState) -> String {
    match &state.dark {
        Some(dark) => format!("{}|dark={}", state.value, dark),
        None => state.value.clone(),
    }
}
