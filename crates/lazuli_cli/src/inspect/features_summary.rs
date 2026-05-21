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

use lazuli_ir::{ConventionOrigin, ConventionRef, EnvName, Feature, RateLimitSpec, Resource};
use std::collections::BTreeMap;

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

/// Map of `<Resource.name> -> has_any_owner_axis_field`. Built once per
/// feature; queried by both the resource-header renderer and the
/// command/query origin renderer so the `owner-scope` suffix is
/// surfaced consistently. The map only includes resources whose fields
/// actually carry `@owner_axis` — absence is treated as `false` (default
/// tenant-only scope, per the owner-scope proposal §7.2).
fn build_owner_scope_lookup(resources: &[Resource]) -> BTreeMap<&str, bool> {
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
fn origin_owner_scope(
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

/// `(conventions: <a>, <b>)` annotation for a resource. Appends a
/// trailing `, owner-scope` segment when at least one of the resource's
/// fields carries `@owner_axis(through: ...)` — see owner-scope proposal
/// §11.2. Empty string when the slot is empty (no annotation rendered at
/// all); the owner-scope suffix is suppressed in that case because the
/// inspect view scopes annotations to opted-in resources.
fn format_resource_conventions(conventions: &[ConventionRef], owner_scope: bool) -> String {
    if conventions.is_empty() {
        return String::new();
    }
    let mut names: Vec<String> = conventions.iter().map(|c| convention_name(c).to_owned()).collect();
    if owner_scope {
        names.push("owner-scope".to_owned());
    }
    format!(" (conventions: {})", names.join(", "))
}

/// Bracketed origin annotation for a command/query name. Empty when
/// the entry carries no `synth_origins` record (a pure author-written
/// command with no convention overlap). Synth-origin entries carry the
/// trailing `, owner-scope` segment when the originating resource has
/// any field with `@owner_axis` — surfaces the owner-scope mode at
/// per-command granularity per owner-scope proposal §11.2.
fn format_origin_annotation(origin: Option<&ConventionOrigin>, owner_scope: bool) -> String {
    match origin {
        None => String::new(),
        Some(ConventionOrigin::Synthesized(c)) => {
            if owner_scope {
                format!("[conv:{}, owner-scope]", convention_name(c))
            } else {
                format!("[conv:{}]", convention_name(c))
            }
        }
        Some(ConventionOrigin::AuthorOverride(_)) => {
            "[author override; convention skipped]".to_owned()
        }
    }
}

/// `crud`, `me`, etc. — single source of truth so the LSP catalog
/// list, the doctor diagnostic suggestion, and this rendering all
/// stay aligned. Adding a variant in `lazuli_ir::ConventionRef`
/// requires extending this match — the closed `match` makes that
/// failure mode load-bearing at compile time.
fn convention_name(c: &ConventionRef) -> &'static str {
    match c {
        ConventionRef::Crud => "crud",
        ConventionRef::Me => "me",
    }
}

/// `rate_limit: <default> [(default) | <limit> in <envs>...]` projection
/// for a single command/query/api row — see
/// `docs/proposals/ir-rate-limit-env-aware.md` §11.2.
///
/// Three shapes:
/// - `Option::None` returns `String::new()` (no suffix).
/// - `Some(spec)` with `by_env` empty: legacy compact shape,
///   `rate_limit: <default>` (no `(default)` marker; backward-compat
///   with single-line declarations).
/// - `Some(spec)` with one-or-more `by_env` entries: env-qualified
///   shape, `rate_limit: <default> (default) | <limit> in <envs>...`.
///
/// The literal string from `default` / `by_env[i].limit` is used as-is;
/// `"unlimited"` is surfaced verbatim (empty-string limits — the
/// lowering target of `"unlimited"` in Cell 1 — also render as
/// `unlimited` so the projection is round-trippable for humans).
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
pub fn format_rate_limit_suffix(spec: Option<&RateLimitSpec>) -> String {
    let Some(spec) = spec else {
        return String::new();
    };
    if spec.by_env.is_empty() {
        return format!("rate_limit: {}", display_limit(&spec.default));
    }
    let mut out = format!("rate_limit: {} (default)", display_limit(&spec.default));
    for entry in &spec.by_env {
        let envs = entry
            .envs
            .iter()
            .map(env_name_str)
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(" | {} in {}", display_limit(&entry.limit), envs));
    }
    out
}

/// Surface an `unlimited`-equivalent empty-string limit verbatim as
/// `unlimited` (the sentinel lowering from `"unlimited"` per
/// proposal §4.4). Non-empty literals pass through unchanged so the
/// inspect view shows the author's text exactly.
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
fn display_limit(literal: &str) -> &str {
    if literal.is_empty() {
        "unlimited"
    } else {
        literal
    }
}

/// Map `EnvName` to its lowercase wire-form string — same identifiers
/// the parser / runtime / doctor catalog agree on.
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
fn env_name_str(env: &EnvName) -> &'static str {
    match env {
        EnvName::Production => "production",
        EnvName::Staging => "staging",
        EnvName::Test => "test",
        EnvName::Dev => "dev",
        EnvName::Local => "local",
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, Field,
        FieldConstraints, ListQuery, LookupQuery, OwnerAxis, Policies, PolicyRef, QualifiedName,
        Query, Resource, TypeRef,
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
            owner_scope_sql: None,
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
            owner_scope_sql: None,
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
            owner_scope_sql: None,
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
        assert_eq!(
            out, expected,
            "§8 customer-me summary diverged from spec"
        );
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

    // ----------------------------------------------------------------
    // Cell O3 — owner-scope surface. Spec:
    // `docs/proposals/ir-resource-conventions-owner-scope.md` §11.2.
    // When a resource has `conventions [..]` AND any of its fields
    // carries `@owner_axis(through: <col>)`, the inspect header gains
    // an `, owner-scope` suffix, and every synth-origin command/query
    // is annotated `[conv:<bundle>, owner-scope]`.
    // ----------------------------------------------------------------

    /// Build a `host: Host required` FK field carrying
    /// `@owner_axis(through: user)`. Pattern lifted verbatim from the
    /// Hostpoint pilot's `Property.host` field (§1.5).
    fn owner_axis_host_field() -> Field {
        Field {
            name: "host".to_owned(),
            type_ref: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Host".to_owned(),
            }),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: Some(OwnerAxis {
                through_column: "user".to_owned(),
            }),
            span_ref: None,
        }
    }

    /// Build a tenant-only `name: Text required` field used to pad out
    /// the owner-scope fixtures so the resource has at least one
    /// non-FK field too.
    fn text_field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn property_resource_with_owner_axis(conventions: Vec<ConventionRef>) -> Resource {
        Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![owner_axis_host_field(), text_field("name")],
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
        feature.resources.push(property_resource_with_owner_axis(vec![
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

    // ----------------------------------------------------------------
    // IR Rate-Limit env-aware — Cell 3 inspect surface. Spec:
    // `docs/proposals/ir-rate-limit-env-aware.md` §11.2.
    // The `format_rate_limit_suffix` helper projects the three
    // shapes: no slot (omitted), legacy single-line, env-qualified.
    // ----------------------------------------------------------------

    use lazuli_ir::{RateLimitByEnv, RateLimitSpec};

    /// No `rate_limit` slot → empty string (the per-row renderer
    /// omits the suffix entirely). This is the backward-compat
    /// fixture for commands without a rate_limit declaration.
    #[test]
    fn format_rate_limit_suffix_none_omitted() {
        let out = super::format_rate_limit_suffix(None);
        assert_eq!(out, "", "absent rate_limit must render no suffix");
    }

    /// Legacy single-line shape (default only, no `by_env` entries)
    /// renders the compact form: `rate_limit: <literal>` with no
    /// `(default)` marker. Backward-compat with the 56 pilot-A
    /// declarations.
    #[test]
    fn format_rate_limit_suffix_legacy_default_only() {
        let spec = RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: Vec::new(),
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        assert_eq!(
            out, "rate_limit: 5 per 10 minutes per ip",
            "legacy single-line rate_limit should render the compact form"
        );
    }

    /// Env-qualified shape: default + one `by_env` entry covering
    /// three envs. Renders the §11.2 verbatim shape:
    /// `rate_limit: <default> (default) | <limit> in <envs>`.
    /// "unlimited" lowers to empty-string in the IR; the projection
    /// surfaces the word back to humans.
    #[test]
    fn format_rate_limit_suffix_env_qualified_register() {
        let spec = RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: vec![RateLimitByEnv {
                envs: vec![EnvName::Dev, EnvName::Staging, EnvName::Test],
                unknown_envs: Vec::new(),
                // empty string == lowered "unlimited" (§4.4).
                limit: String::new(),
                span_ref: None,
            }],
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        assert_eq!(
            out,
            "rate_limit: 5 per 10 minutes per ip (default) | unlimited in dev,staging,test",
            "env-qualified shape should match §11.2 verbatim line"
        );
    }

    /// Multiple env-qualified entries — `request_password_reset`
    /// style: strict in prod, looser in dev/staging, unlimited in
    /// test. Each entry renders as a `| <limit> in <envs>` slot.
    #[test]
    fn format_rate_limit_suffix_multiple_env_entries() {
        let spec = RateLimitSpec {
            default: "3 per 10 minutes per ip".to_owned(),
            by_env: vec![
                RateLimitByEnv {
                    envs: vec![EnvName::Dev, EnvName::Staging],
                    unknown_envs: Vec::new(),
                    limit: "60 per 10 minutes per ip".to_owned(),
                    span_ref: None,
                },
                RateLimitByEnv {
                    envs: vec![EnvName::Test],
                    unknown_envs: Vec::new(),
                    limit: String::new(),
                    span_ref: None,
                },
            ],
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        let expected = "rate_limit: 3 per 10 minutes per ip (default) | \
                        60 per 10 minutes per ip in dev,staging | \
                        unlimited in test";
        assert_eq!(
            out, expected,
            "multi-env shape should chain `| <limit> in <envs>` slots in source order"
        );
    }
}
