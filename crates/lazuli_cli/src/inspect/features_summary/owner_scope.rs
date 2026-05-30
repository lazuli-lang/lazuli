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

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // ----------------------------------------------------------------
    // Cell O3 — owner-scope surface. Spec:
    // `docs/proposals/ir-resource-conventions-owner-scope.md` §11.2.
    // When a resource has `conventions [..]` AND any of its fields
    // carries `@owner_axis(through: <col>)`, the inspect header gains
    // an `, owner-scope` suffix, and every synth-origin command/query
    // is annotated `[conv:<bundle>, owner-scope]`.
    //
    // The tests reach into the orchestrator (`render_features_summary`)
    // rather than calling `build_owner_scope_lookup` directly — the
    // owner-scope flag is observed via its effect on the rendered text,
    // which is the contract the inspect CLI exposes.
    // ----------------------------------------------------------------

    use super::super::render_features_summary;
    use super::super::test_fixtures::{
        empty_feature, list_query, lookup_query, minimal_command, property_resource_with_owner_axis,
    };
    use lazuli_ir::{ConventionOrigin, ConventionRef};

    /// §11.2 — Property resource has `conventions [crud]` AND
    /// `host: Host required @owner_axis(through: user)`. The header
    /// becomes `Property (conventions: crud, owner-scope)` and every
    /// synth-origin command/query gets `[conv:crud, owner-scope]`.
    #[test]
    fn renders_owner_scope_annotation_when_field_has_owner_axis() {
        let mut feature = empty_feature("catalog");
        feature
            .resources
            .push(property_resource_with_owner_axis(vec![ConventionRef::Crud]));
        for n in ["create_property", "update_property", "delete_property"] {
            feature.commands.push(minimal_command(n));
            feature.synth_origins.insert(
                n.to_owned(),
                ConventionOrigin::Synthesized(ConventionRef::Crud),
            );
        }
        feature.queries.push(lookup_query("lookup_property"));
        feature.queries.push(list_query("list_propertys"));
        feature.synth_origins.insert(
            "lookup_property".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "list_propertys".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );

        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("Property (conventions: crud, owner-scope)"),
            "expected owner-scope-augmented header, got:\n{out}"
        );
        // Commands column width = 15 (`update_property`, `delete_property`,
        // `create_property` are all 15 chars).
        assert!(
            out.contains("create_property    [conv:crud, owner-scope]"),
            "expected create_property owner-scope synth row, got:\n{out}"
        );
        assert!(
            out.contains("update_property    [conv:crud, owner-scope]"),
            "expected update_property owner-scope synth row, got:\n{out}"
        );
        assert!(
            out.contains("delete_property    [conv:crud, owner-scope]"),
            "expected delete_property owner-scope synth row, got:\n{out}"
        );
        // Queries column width = 15 (`lookup_property`, `list_propertys`).
        assert!(
            out.contains("lookup_property    [conv:crud, owner-scope]"),
            "expected lookup_property owner-scope synth row, got:\n{out}"
        );
        assert!(
            out.contains("list_propertys     [conv:crud, owner-scope]"),
            "expected list_propertys owner-scope synth row, got:\n{out}"
        );
    }

    /// §11.2 composition: `conventions [crud, me]` + `@owner_axis` on
    /// one of the resource's FK fields. Header carries every bundle
    /// name plus the `owner-scope` suffix; every synth-origin row from
    /// either bundle picks up the owner-scope tag.
    #[test]
    fn renders_composed_crud_me_owner_scope() {
        let mut feature = empty_feature("catalog");
        feature
            .resources
            .push(property_resource_with_owner_axis(vec![
                ConventionRef::Crud,
                ConventionRef::Me,
            ]));
        for n in ["create_property", "update_property", "delete_property"] {
            feature.commands.push(minimal_command(n));
            feature.synth_origins.insert(
                n.to_owned(),
                ConventionOrigin::Synthesized(ConventionRef::Crud),
            );
        }
        feature.queries.push(lookup_query("lookup_property"));
        feature.queries.push(list_query("list_propertys"));
        feature.queries.push(lookup_query("lookup_my_property"));
        feature.synth_origins.insert(
            "lookup_property".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "list_propertys".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "lookup_my_property".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Me),
        );

        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("Property (conventions: crud, me, owner-scope)"),
            "expected composed `(conventions: crud, me, owner-scope)` header, got:\n{out}"
        );
        // crud-origin commands keep the crud tag with the owner-scope
        // suffix. The me-origin query picks up the me tag with the
        // same suffix.
        assert!(
            out.contains("create_property    [conv:crud, owner-scope]"),
            "expected create_property crud+owner-scope row, got:\n{out}"
        );
        assert!(
            out.contains("lookup_my_property    [conv:me, owner-scope]"),
            "expected lookup_my_property me+owner-scope row, got:\n{out}"
        );
    }
}
