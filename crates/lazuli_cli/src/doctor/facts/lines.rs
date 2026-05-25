//! Source-line lookup utilities shared by the IR-driven fact builders.
//!
//! Each helper turns a `(source, names | patterns)` pair into a
//! `BTreeMap<String, usize>` of `name → 1-based line number` or a
//! `Vec<String>` of every occurrence. The doctor uses these to anchor
//! diagnostics at the header line a feature, command, query, api, or
//! event_group lives on — the IR carries the typed shape, the source
//! line lookup keeps the surface anchor precise.
//!
//! Why these live next to the IR fact builders (`facts/feature_ir.rs`)
//! and not in `scanners.rs`: every consumer is an IR-driven
//! `populate_*_from_ir` call that already needs to walk the typed
//! shape and only needs the line to anchor the diagnostic. Keeping
//! them adjacent makes the substitution rule obvious — "if you have
//! IR and want a line, call one of these".
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};

/// Generic construct line lookup. Used by IR-driven fact builders to
/// anchor diagnostics at the surface line of a `command`, `job`, …
/// header. Returns `name → 1-based line number`.
pub(crate) fn collect_construct_lines(
    source: &str,
    prefix: &str,
    names: BTreeSet<&str>,
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if names.is_empty() {
        return out;
    }
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if names.contains(name) {
            out.entry(name.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// OpenAPI/Cache bucket cycles — line lookup for `query.list`,
/// `query.lookup`, `query.sql`, `query.view` headers. Mirrors
/// `collect_construct_lines` but the parser folds the kind into the
/// header keyword.
pub(crate) fn collect_query_lines(
    source: &str,
    queries: &[lazuli_ir::Query],
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if queries.is_empty() {
        return out;
    }
    let names: BTreeSet<&str> = queries
        .iter()
        .map(|q| match q {
            lazuli_ir::Query::List(l) => l.name.as_str(),
            lazuli_ir::Query::Lookup(l) => l.name.as_str(),
            lazuli_ir::Query::Sql(s) => s.name.as_str(),
        })
        .collect();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after = trimmed
            .strip_prefix("query.list ")
            .or_else(|| trimmed.strip_prefix("query.lookup "))
            .or_else(|| trimmed.strip_prefix("query.sql "))
            .or_else(|| trimmed.strip_prefix("query.view "));
        let Some(rest) = after else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if names.contains(name) {
            out.entry(name.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// OpenAPI bucket cycle — collect every `api <name>` declaration in
/// the source by text-pattern. The doctor diagnostic then subtracts
/// names that were lifted to `feature.apis` IR to know which entries
/// are still text-pattern (i.e. the OpenAPI emitter falls back to a
/// stub for them).
pub(crate) fn collect_text_pattern_api_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("api ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// i18n bucket cycle — find the first line containing the bare keyword
/// (preceded only by whitespace). Used for `translation` line anchoring.
pub(crate) fn find_keyword_line(source: &str, keyword: &str) -> Option<usize> {
    for (idx, line) in source.lines().enumerate() {
        if line.trim() == keyword {
            return Some(idx + 1);
        }
    }
    None
}

/// Phase L Tier 3 — line lookup for `event_group <pattern>` headers.
/// Same as `collect_construct_lines` but matches the pattern token.
pub(crate) fn collect_event_group_lines(
    source: &str,
    patterns: BTreeSet<&str>,
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if patterns.is_empty() {
        return out;
    }
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("event_group ") else {
            continue;
        };
        let pattern = rest.split_whitespace().next().unwrap_or("");
        if patterns.contains(pattern) {
            out.entry(pattern.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// Phase L Tier 3 — derive the feature's tenancy axis from the lifted
/// IR `Defaults` block. Returns the axis name (`org`, `team`, custom)
/// or `None` when the feature declares `tenancy none` / inherits.
///
/// Phase L Tier 4a — `parse_feature_skeletons` now lifts
/// `defaults.tenancy`; this is a typed read of
/// `feature.defaults.tenancy`. The legacy "axis unknown → only check
/// presence" fallback that tier 3 diagnostics rode on is retired.
pub(crate) fn tenancy_axis_for(feature: &lazuli_ir::Feature) -> Option<String> {
    match feature.defaults.tenancy.as_ref()? {
        lazuli_ir::Tenancy::Org => Some("org".to_owned()),
        lazuli_ir::Tenancy::Team => Some("team".to_owned()),
        lazuli_ir::Tenancy::Custom(name) => Some(name.clone()),
        // `tenancy none` is an explicit opt-out — there is no axis to
        // cross-check against.
        lazuli_ir::Tenancy::None => None,
    }
}
