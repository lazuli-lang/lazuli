//! Cache-invalidation target list builder for runtime commands.
//!
//! When a command lands in the runtime it carries an `invalidates`
//! manifest the SDK uses to drop stale query cache entries. The list
//! has two sources: author-declared entries on the `RuntimeCommand` and
//! the auto-derived set of every same-feature query (a mutation
//! against the feature's single resource invalidates every read of
//! that resource).
//!
//! Both sources normalize to the post-B1 wire key
//! `<feature>.<query_name>` (cell B1 of codegen-correctness-cycle-
//! 2026-05-21 dropped the historical `.query.` infix because the
//! `/q/` HTTP prefix already disambiguates kind), and duplicates are
//! dropped after normalization so an explicit author entry retains
//! priority without losing derived coverage.

use std::collections::BTreeSet;

use lazuli_codegen_spec::{RuntimeCommand, RuntimeEffect, RuntimeFeature};

pub(super) fn merged_invalidates(
    feature: &RuntimeFeature,
    command: &RuntimeCommand,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut merged = Vec::new();

    for target in &command.invalidates {
        if let Some(normalized) = normalize_invalidates_target(&feature.name, target) {
            push_unique_invalidates(&mut merged, &mut seen, normalized);
        }
    }
    for target in derive_invalidates(feature, command) {
        push_unique_invalidates(&mut merged, &mut seen, target);
    }

    merged
}

fn push_unique_invalidates(out: &mut Vec<String>, seen: &mut BTreeSet<String>, target: String) {
    if seen.insert(target.clone()) {
        out.push(target);
    }
}

// TEMP-stub for IA2 coordination: RuntimeCommand.invalidates can arrive
// already normalized (`feature.query`) or with the pre-normalization
// `query.<name>` / `<feature>.query.<name>` markers. Collapse every form
// to the post-B1 `<feature>.<query_name>` SDK wire key at emission time.
fn normalize_invalidates_target(host_feature: &str, target: &str) -> Option<String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(query) = trimmed.strip_prefix("query.") {
        return Some(format!("{}.{}", host_feature, query));
    }

    if let Some((feature, query)) = trimmed.split_once(".query.") {
        return Some(format!("{}.{}", feature, query));
    }

    if trimmed.contains('.') {
        Some(trimmed.to_owned())
    } else {
        Some(format!("{}.{}", host_feature, trimmed))
    }
}

/// Derive the cache-invalidation target list for a command from the
/// feature it lives in.
///
/// Every `RuntimeEffect` variant today (`CreatesFromInput`, `UpdatesByID`,
/// `DeletesByID`) is a mutation against the feature's single resource
/// (`RuntimeFeature.resources.first()`), so the companion query set —
/// every `RuntimeQuery` declared on the same feature — is the natural
/// invalidation target.
///
/// The qualified-name format mirrors `write_query`'s registry key
/// (`<feature>.<short_name>`). Cell B1 (codegen-correctness-cycle-
/// 2026-05-21) dropped the historical `.query.` infix in lockstep
/// because the `/q/` HTTP prefix already disambiguates kind.
///
/// Future cells that introduce `RuntimeEffect::Returns` or other
/// non-mutating shapes will need to early-return `Vec::new()` for
/// those kinds; today every variant maps to a mutation.
fn derive_invalidates(feature: &RuntimeFeature, command: &RuntimeCommand) -> Vec<String> {
    match command.effect {
        RuntimeEffect::CreatesFromInput
        | RuntimeEffect::UpdatesByID
        | RuntimeEffect::DeletesByID => feature
            .queries
            .iter()
            .map(|q| format!("{}.{}", feature.name, q.short_name))
            .collect(),
    }
}
