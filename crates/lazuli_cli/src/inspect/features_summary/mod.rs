//! `lazuli inspect features` summary renderer — Cell C4 surface for the
//! `conventions [crud]` annotation.
//!
//! Produces the human-readable per-feature digest shown in
//! `docs/proposals/ir-resource-conventions-crud.md` §11. Each feature
//! lists its resources, commands, and queries; resources whose
//! `conventions` slot includes a bundle get a `(conventions: <bundle>)`
//! annotation; commands/queries whose origin is the synth pass get a
//! `[conv:<bundle>]` annotation; author-overridden synth names get
//! `[author override; convention skipped]`. Author-only commands
//! (no convention origin) carry no annotation.
//!
//! The renderer is pure (no I/O): callers pass a slice of `Feature`
//! values produced by the analyzer + C3's synthesis pass. Tests below
//! exercise the §8 Customer + §9 worked-override examples verbatim.

use lazuli_ir::Feature;

mod annotations;
mod owner_scope;
mod rate_limit;

#[cfg(test)]
mod test_fixtures;

use annotations::{
    format_origin_annotation, format_resource_conventions, render_name_row,
};
use owner_scope::{build_owner_scope_lookup, origin_owner_scope};

// ABI restore — `format_rate_limit_suffix` was `pub` on the original
// `features_summary.rs`; downstream callers (Cell 1 will wire it into
// `render_one_feature`) name it as `inspect::features_summary::format_rate_limit_suffix`.
pub use rate_limit::format_rate_limit_suffix;

/// Render a `lazuli inspect features` text digest for one or more
/// lowered features. The output shape matches the boxed listings in
/// `docs/proposals/ir-resource-conventions-crud.md` §11 exactly.
///
/// Features without any `conventions` slot still appear, so the
/// rendering is stable across whether `crud` is opted into or not.
pub fn render_features_summary(features: &[Feature]) -> String {
    let mut out = String::new();
    for (idx, feature) in features.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        render_one_feature(feature, &mut out);
    }
    out
}

fn render_one_feature(feature: &Feature, out: &mut String) {
    out.push_str(&format!("feature {}\n", feature.name));

    // Pre-build a per-resource owner-scope flag so command/query rows
    // can look up the owning resource's annotation in O(1). Per the
    // owner-scope proposal §11.2, a resource is "owner-scoped" when at
    // least one of its fields carries `@owner_axis(through: ...)`.
    let owner_scope_by_resource = build_owner_scope_lookup(&feature.resources);

    // Resources section. Resources without a `conventions` slot still
    // surface — the inspect view is the full feature snapshot, not a
    // conventions-only filter.
    out.push_str("  resources:\n");
    for resource in &feature.resources {
        let owner_scope = owner_scope_by_resource
            .get(resource.name.as_str())
            .copied()
            .unwrap_or(false);
        let annotation = format_resource_conventions(&resource.conventions, owner_scope);
        out.push_str(&format!("    {}{}\n", resource.name, annotation));
    }

    // Commands section. Width-pad the name column so origin brackets
    // align — the longest name in the section sets the column width.
    if !feature.commands.is_empty() {
        out.push_str("  commands:\n");
        let width = feature
            .commands
            .iter()
            .map(|c| c.name.len())
            .max()
            .unwrap_or(0);
        for command in &feature.commands {
            let origin = feature.synth_origins.get(command.name.as_str());
            let owner_scope =
                origin_owner_scope(origin, &feature.resources, &owner_scope_by_resource);
            let annotation = format_origin_annotation(origin, owner_scope);
            out.push_str(&render_name_row(&command.name, width, &annotation));
        }
    }

    // Queries section. Same width-pad logic.
    if !feature.queries.is_empty() {
        out.push_str("  queries:\n");
        let width = feature
            .queries
            .iter()
            .map(|q| q.name().len())
            .max()
            .unwrap_or(0);
        for query in &feature.queries {
            let origin = feature.synth_origins.get(query.name());
            let owner_scope =
                origin_owner_scope(origin, &feature.resources, &owner_scope_by_resource);
            let annotation = format_origin_annotation(origin, owner_scope);
            out.push_str(&render_name_row(query.name(), width, &annotation));
        }
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_fixtures::{
        customer_resource, empty_feature, list_query, lookup_query, minimal_command,
    };
    use lazuli_ir::{ConventionOrigin, ConventionRef};

    /// §8 Customer example: full crud synth, no author overrides.
    #[test]
    fn renders_section_8_customer_synth_output() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(customer_resource(vec![ConventionRef::Crud]));
        for n in ["create_customer", "update_customer", "delete_customer"] {
            feature.commands.push(minimal_command(n));
            feature.synth_origins.insert(
                n.to_owned(),
                ConventionOrigin::Synthesized(ConventionRef::Crud),
            );
        }
        feature.queries.push(lookup_query("lookup_customer"));
        feature.queries.push(list_query("list_customers"));
        feature.synth_origins.insert(
            "lookup_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "list_customers".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );

        let out = render_features_summary(&[feature]);
        let expected = "\
feature customer
  resources:
    Customer (conventions: crud)
  commands:
    create_customer    [conv:crud]
    update_customer    [conv:crud]
    delete_customer    [conv:crud]
  queries:
    lookup_customer    [conv:crud]
    list_customers     [conv:crud]
";
        assert_eq!(out, expected, "§8 customer summary diverged from spec");
    }

    /// §9 worked override: author writes `update_customer`, the synth
    /// skips that name. Inspect shows the author entry with the
    /// `[author override; convention skipped]` marker; the other 4
    /// synth entries render normally.
    #[test]
    fn renders_section_9_worked_override() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(customer_resource(vec![ConventionRef::Crud]));

        // create / delete are synthesized; update_customer is author-written.
        feature.commands.push(minimal_command("create_customer"));
        feature.commands.push(minimal_command("update_customer"));
        feature.commands.push(minimal_command("delete_customer"));

        feature.synth_origins.insert(
            "create_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "update_customer".to_owned(),
            ConventionOrigin::AuthorOverride(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "delete_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );

        feature.queries.push(lookup_query("lookup_customer"));
        feature.queries.push(list_query("list_customers"));
        feature.synth_origins.insert(
            "lookup_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "list_customers".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );

        let out = render_features_summary(&[feature]);
        // Commands column width is the max command name length (15 chars).
        // §11 quotes the override row verbatim, so we check the bracketed
        // tag substring rather than asserting full file alignment.
        assert!(
            out.contains("create_customer    [conv:crud]"),
            "expected create_customer synth row, got:\n{out}"
        );
        assert!(
            out.contains("update_customer    [author override; convention skipped]"),
            "expected author-override row, got:\n{out}"
        );
        assert!(
            out.contains("delete_customer    [conv:crud]"),
            "expected delete_customer synth row, got:\n{out}"
        );
        assert!(
            out.contains("lookup_customer    [conv:crud]"),
            "expected lookup_customer synth row, got:\n{out}"
        );
        assert!(
            out.contains("list_customers     [conv:crud]"),
            "expected list_customers synth row, got:\n{out}"
        );
    }

    /// Resources without a `conventions` slot must NOT emit the
    /// `(conventions: ...)` annotation — the renderer's snapshot is
    /// stable across pre-conventions fixtures.
    #[test]
    fn omits_annotation_when_resource_has_no_conventions() {
        let mut feature = empty_feature("blank");
        feature.resources.push(customer_resource(vec![]));
        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("Customer\n"),
            "expected bare Customer row, got:\n{out}"
        );
        assert!(
            !out.contains("conventions:"),
            "annotation leaked into non-opted-in resource:\n{out}"
        );
    }

    /// Author-written commands with no convention origin emit no
    /// brackets — only the bare name. This isolates the synth tag from
    /// arbitrary feature commands.
    #[test]
    fn pure_author_commands_have_no_annotation() {
        let mut feature = empty_feature("blank");
        feature.commands.push(minimal_command("custom_handler"));
        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("    custom_handler\n"),
            "expected unannotated row, got:\n{out}"
        );
        assert!(
            !out.contains("[conv:"),
            "convention tag leaked into pure-author command:\n{out}"
        );
    }

    // ----------------------------------------------------------------
    // Cell M3 — me bundle. Spec:
    // `docs/proposals/ir-resource-conventions-me.md` §8 (Customer
    // with `conventions [me]`) + §6.1 (composition `conventions
    // [crud, me]`).
    // ----------------------------------------------------------------

    /// §8 Customer with `conventions [me]` only: header should
    /// annotate `(conventions: me)` and the single synthesized
    /// `lookup_my_customer` query carries `[conv:me]`.
    #[test]
    fn renders_section_8_customer_me_synth_output() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(customer_resource(vec![ConventionRef::Me]));
        feature.queries.push(lookup_query("lookup_my_customer"));
        feature.synth_origins.insert(
            "lookup_my_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Me),
        );

        let out = render_features_summary(&[feature]);
        let expected = "\
feature customer
  resources:
    Customer (conventions: me)
  queries:
    lookup_my_customer    [conv:me]
";
        assert_eq!(out, expected, "§8 customer-me summary diverged from spec");
    }

    /// §6.1 worked composition: `conventions [crud, me]` yields 6
    /// entries (5 from crud + 1 from me) with no collisions. The
    /// resource header lists bundles in declaration order
    /// (`crud, me`); each synthesized command/query carries the
    /// originating bundle's tag.
    #[test]
    fn renders_section_6_1_composition_crud_plus_me() {
        let mut feature = empty_feature("customer");
        feature.resources.push(customer_resource(vec![
            ConventionRef::Crud,
            ConventionRef::Me,
        ]));
        for n in ["create_customer", "update_customer", "delete_customer"] {
            feature.commands.push(minimal_command(n));
            feature.synth_origins.insert(
                n.to_owned(),
                ConventionOrigin::Synthesized(ConventionRef::Crud),
            );
        }
        feature.queries.push(lookup_query("lookup_customer"));
        feature.queries.push(list_query("list_customers"));
        feature.queries.push(lookup_query("lookup_my_customer"));
        feature.synth_origins.insert(
            "lookup_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "list_customers".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Crud),
        );
        feature.synth_origins.insert(
            "lookup_my_customer".to_owned(),
            ConventionOrigin::Synthesized(ConventionRef::Me),
        );

        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("Customer (conventions: crud, me)"),
            "expected composed-bundle header `(conventions: crud, me)`, got:\n{out}"
        );
        // Commands column width = 15 (`update_customer`, `delete_customer`, `create_customer`).
        assert!(
            out.contains("create_customer    [conv:crud]"),
            "expected create_customer synth row, got:\n{out}"
        );
        assert!(
            out.contains("delete_customer    [conv:crud]"),
            "expected delete_customer synth row, got:\n{out}"
        );
        // Queries column width = 18 (`lookup_my_customer`). The shorter
        // names get trailing padding before the bracketed annotation.
        assert!(
            out.contains("lookup_customer       [conv:crud]"),
            "expected lookup_customer synth row (queries col width 18), got:\n{out}"
        );
        assert!(
            out.contains("list_customers        [conv:crud]"),
            "expected list_customers synth row (queries col width 18), got:\n{out}"
        );
        assert!(
            out.contains("lookup_my_customer    [conv:me]"),
            "expected lookup_my_customer me-synth row, got:\n{out}"
        );
    }

    /// §6 worked override applied to the me bundle: author wrote
    /// `lookup_my_customer`, the synth skipped. Inspect shows the
    /// `[author override; convention skipped]` marker — same string
    /// as the crud-side override.
    #[test]
    fn renders_me_author_override() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(customer_resource(vec![ConventionRef::Me]));
        feature.queries.push(lookup_query("lookup_my_customer"));
        feature.synth_origins.insert(
            "lookup_my_customer".to_owned(),
            ConventionOrigin::AuthorOverride(ConventionRef::Me),
        );

        let out = render_features_summary(&[feature]);
        assert!(
            out.contains("Customer (conventions: me)"),
            "expected (conventions: me) header, got:\n{out}"
        );
        assert!(
            out.contains("lookup_my_customer    [author override; convention skipped]"),
            "expected author-override row, got:\n{out}"
        );
    }

}
