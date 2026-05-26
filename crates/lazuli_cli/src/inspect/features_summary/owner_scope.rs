//! Owner-scope flag plumbing for the inspect features summary.
//!
//! The `@owner_axis(through: <col>)` field decorator promotes a
//! resource from the default tenant-only scope to a per-owner axis —
//! see `docs/proposals/ir-resource-conventions-owner-scope.md` §7.2.
//! The renderer needs O(1) access to "does this resource carry an
//! owner-axis field?" both when rendering the resource header and
//! when annotating each synth-origin command/query. This module owns
//! the lookup table and the per-origin resolver.

use lazuli_ir::{ConventionOrigin, Resource};
use std::collections::BTreeMap;

/// Map of `<Resource.name> -> has_any_owner_axis_field`. Built once per
/// feature; queried by both the resource-header renderer and the
/// command/query origin renderer so the `owner-scope` suffix is
/// surfaced consistently. The map only includes resources whose fields
/// actually carry `@owner_axis` — absence is treated as `false` (default
/// tenant-only scope, per the owner-scope proposal §7.2).
pub(super) fn build_owner_scope_lookup(resources: &[Resource]) -> BTreeMap<&str, bool> {
    let mut map = BTreeMap::new();
    for resource in resources {
        let has_owner_axis = resource
            .fields
            .iter()
            .any(|field| field.owner_axis.is_some());
        map.insert(resource.name.as_str(), has_owner_axis);
    }
    map
}

/// Resolve the owner-scope flag for a single command/query origin.
/// Synth-origin entries inherit the flag from the resource the synth
/// pass attaches the bundle to — for crud and me, that's the resource
/// whose `conventions [..]` slot drives the synth. Author-overridden
/// or pure-author entries always render without the suffix.
pub(super) fn origin_owner_scope(
    origin: Option<&ConventionOrigin>,
    resources: &[Resource],
    owner_scope_by_resource: &BTreeMap<&str, bool>,
) -> bool {
    let Some(ConventionOrigin::Synthesized(_)) = origin else {
        return false;
    };
    // The synth pass attaches one bundle per resource opted-in via
    // `conventions [..]`. There is at most one resource per feature
    // (in the current pilot) — we surface owner-scope iff any
    // opted-in resource on the feature carries `@owner_axis`.
    resources
        .iter()
        .filter(|r| !r.conventions.is_empty())
        .any(|r| {
            owner_scope_by_resource
                .get(r.name.as_str())
                .copied()
                .unwrap_or(false)
        })
}
