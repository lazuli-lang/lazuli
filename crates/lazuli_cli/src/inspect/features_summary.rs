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

use lazuli_ir::{ConventionOrigin, ConventionRef, Feature};

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

    // Resources section. Resources without a `conventions` slot still
    // surface — the inspect view is the full feature snapshot, not a
    // conventions-only filter.
    out.push_str("  resources:\n");
    for resource in &feature.resources {
        let annotation = format_resource_conventions(&resource.conventions);
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
            let annotation = format_origin_annotation(origin);
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
            let annotation = format_origin_annotation(origin);
            out.push_str(&render_name_row(query.name(), width, &annotation));
        }
    }
}

/// Render one `<indent><name><pad>[<annotation>]` row, omitting the
/// trailing space + bracket when the annotation is empty (pure
/// author-written entry without a convention origin).
fn render_name_row(name: &str, width: usize, annotation: &str) -> String {
    if annotation.is_empty() {
        format!("    {name}\n")
    } else {
        let pad = width.saturating_sub(name.len());
        let spaces = " ".repeat(pad);
        format!("    {name}{spaces}    {annotation}\n")
    }
}

/// `(conventions: <a>, <b>)` annotation for a resource. Empty string
/// when the slot is empty (no annotation rendered at all).
fn format_resource_conventions(conventions: &[ConventionRef]) -> String {
    if conventions.is_empty() {
        return String::new();
    }
    let names: Vec<&'static str> = conventions.iter().map(convention_name).collect();
    format!(" (conventions: {})", names.join(", "))
}

/// Bracketed origin annotation for a command/query name. Empty when
/// the entry carries no `synth_origins` record (a pure author-written
/// command with no convention overlap).
fn format_origin_annotation(origin: Option<&ConventionOrigin>) -> String {
    match origin {
        None => String::new(),
        Some(ConventionOrigin::Synthesized(c)) => format!("[conv:{}]", convention_name(c)),
        Some(ConventionOrigin::AuthorOverride(_)) => {
            "[author override; convention skipped]".to_owned()
        }
    }
}

/// `crud` etc. — single source of truth so the LSP catalog list, the
/// doctor diagnostic suggestion, and this rendering all stay aligned.
///
/// `Me` is anchored here as a side-effect of inlining the M1 IR
/// prereqs from `ir-resource-conventions-me.md` into Cell M2's
/// worktree (M2 of the same proposal). The user-facing inspect-surface
/// polish (e.g., `[conv:me]` annotation rendering on synthesized
/// queries) lands in Cell M3 — this entry returns just the bundle
/// name so the match stays exhaustive.
fn convention_name(c: &ConventionRef) -> &'static str {
    match c {
        ConventionRef::Crud => "crud",
        ConventionRef::Me => "me",
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, ListQuery,
        LookupQuery, Policies, PolicyRef, Query, Resource,
    };
    use std::collections::BTreeMap;

    /// Build a baseline empty feature with the required slots filled
    /// minimally — used by the §8 and §9 fixtures below.
    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: BTreeMap::new(),
            span_ref: None,
        }
    }

    fn customer_resource(conventions: Vec<ConventionRef>) -> Resource {
        Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions,
        }
    }

    /// Minimal `Command` value — just enough to give the renderer a
    /// `name` to print. Effect/input are the inert variants.
    fn minimal_command(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Create,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
        }
    }

    fn list_query(name: &str) -> Query {
        Query::List(ListQuery {
            name: name.to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        })
    }

    fn lookup_query(name: &str) -> Query {
        Query::Lookup(LookupQuery {
            name: name.to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        })
    }

    /// §8 Customer example: full crud synth, no author overrides.
    #[test]
    fn renders_section_8_customer_synth_output() {
        let mut feature = empty_feature("customer");
        feature.resources.push(customer_resource(vec![ConventionRef::Crud]));
        for n in ["create_customer", "update_customer", "delete_customer"] {
            feature.commands.push(minimal_command(n));
            feature
                .synth_origins
                .insert(n.to_owned(), ConventionOrigin::Synthesized(ConventionRef::Crud));
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
        feature.resources.push(customer_resource(vec![ConventionRef::Crud]));

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
}
