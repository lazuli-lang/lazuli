//! Runtime-emitter tests — kept verbatim from the original
//! `runtime/mod.rs` inline test module.

use super::*;
use lazuli_codegen_spec::{
    FieldKind, QueryKind, RuntimeCommand, RuntimeDeprecation, RuntimeEffect, RuntimeFeature,
    RuntimeField, RuntimeInput, RuntimeQuery, RuntimeResource, Tenancy,
};

fn publication_feature() -> ir::Feature {
    let mut feature = base_feature(vec![publication_resource(Some(publication_lifecycle()))]);
    feature.commands = vec![
        lifecycle_command("begin_publishing", ir::CommandInput::Empty),
        lifecycle_command("mark_published", ir::CommandInput::Empty),
        lifecycle_command(
            "mark_failed",
            ir::CommandInput::Typed(vec![ir::TypedSlot {
                name: "error_reason".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: true,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            }]),
        ),
        lifecycle_command("cancel", ir::CommandInput::Empty),
    ];
    feature
}

fn base_feature(resources: Vec<ir::Resource>) -> ir::Feature {
    ir::Feature {
        name: "publication".to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: ir::Defaults::default(),
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies: ir::Policies::default(),
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
        pollers: vec![],
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: Vec::new(),
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
    }
}

fn publication_resource(lifecycle: Option<ir::Lifecycle>) -> ir::Resource {
    ir::Resource {
        name: "publication".to_owned(),
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
        lifecycle,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    }
}

fn publication_lifecycle() -> ir::Lifecycle {
    ir::Lifecycle {
        discriminator_field: "status".to_owned(),
        generated_enum: "PublicationStatus".to_owned(),
        states: vec![
            lifecycle_state("scheduled", ir::LifecycleStateKind::Initial),
            lifecycle_state("publishing", ir::LifecycleStateKind::Intermediate),
            lifecycle_state("published", ir::LifecycleStateKind::Terminal),
            lifecycle_state("failed", ir::LifecycleStateKind::Terminal),
            lifecycle_state("cancelled", ir::LifecycleStateKind::Terminal),
        ],
        transitions: vec![
            lifecycle_transition("begin_publishing", "scheduled", "publishing"),
            lifecycle_transition("mark_published", "publishing", "published"),
            lifecycle_transition("mark_failed", "publishing", "failed"),
            lifecycle_transition("cancel", "scheduled", "cancelled"),
        ],
        invariants: Vec::new(),
        invariant_handlers: Vec::new(),
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn lifecycle_state(name: &str, kind: ir::LifecycleStateKind) -> ir::LifecycleState {
    ir::LifecycleState {
        name: name.to_owned(),
        kind,
        span_ref: None,
    }
}

fn lifecycle_transition(name: &str, from: &str, to: &str) -> ir::LifecycleTransition {
    ir::LifecycleTransition {
        name: name.to_owned(),
        from: vec![from.to_owned()],
        to: to.to_owned(),
        policy: None,
        audit: None,
        timestamps: None,
        emits: Vec::new(),
        requires: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn lifecycle_command(name: &str, input: ir::CommandInput) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::None,
        policy: ir::PolicyRef::Unresolved("allow".to_owned()),
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
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
        derived_from: None,
    }
}

#[test]
fn command_spec_emits_deprecated_metadata() {
    let feature = RuntimeFeature {
        name: "customer".to_owned(),
        source_path: "features/customer/customer.lzi".to_owned(),
        resources: vec![RuntimeResource {
            name: "customer".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![RuntimeField {
                name: "name".to_owned(),
                kind: FieldKind::Text,
            }],
        }],
        commands: vec![RuntimeCommand {
            short_name: "legacy_update".to_owned(),
            policy_name: "@policy.update".to_owned(),
            policy_atoms: Vec::new(),
            rate_limit: String::new(),
            validators: Vec::new(),
            effect: RuntimeEffect::UpdatesByID,
            inputs: vec![RuntimeInput {
                field_name: "Name".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: Vec::new(),
            invalidates: vec!["customer.list".to_owned()],
            deprecated: Some(RuntimeDeprecation {
                since: Some("2026-03-01".to_owned()),
                replacement: Some("customer.command.update_v2".to_owned()),
                sunset: Some("2026-12-31".to_owned()),
            }),
        }],
        queries: Vec::new(),
    };

    let out = emit_feature_ts(&feature);
    assert!(out.contains("deprecated: {"));
    assert!(out.contains("since: \"2026-03-01\","));
    assert!(out.contains("replacement: \"customer.command.update_v2\","));
    assert!(out.contains("sunset: \"2026-12-31\","));
}

#[test]
fn lifecycle_emits_camel_case_action_map() {
    let out = emit_lifecycle_action_maps_ts(&publication_feature());

    assert!(out.contains("export const publication = {"));
    assert!(out.contains(
        "beginPublishing: useLazuliCommand<{ id: ID }, void>(beginPublishingPublication),"
    ));
    assert!(out.contains(
        "markPublished: useLazuliCommand<{ id: ID }, void>(markPublishedPublication),"
    ));
    assert!(out.contains(
        "markFailed: useLazuliCommand<{ id: ID; errorReason: string }, void>(markFailedPublication),"
    ));
    assert!(out.contains("cancel: useLazuliCommand<{ id: ID }, void>(cancelPublication),"));
}

#[test]
fn transition_input_defaults_to_id() {
    let out = emit_lifecycle_action_maps_ts(&publication_feature());

    assert!(out.contains(
        "beginPublishing: useLazuliCommand<{ id: ID }, void>(beginPublishingPublication),"
    ));
}

#[test]
fn non_lifecycle_resource_does_not_emit_action_map() {
    let out = emit_lifecycle_action_maps_ts(&base_feature(vec![publication_resource(None)]));

    assert!(!out.contains("export const publication ="));
}

// -----------------------------------------------------------------------
// Verb-prefix dedup — see `lzx::query_ident` tests for the canonical
// case matrix. These runtime-side tests confirm `write_query` honours
// the same rule when the synth-emitted query short_name already
// begins with `lookup_` / `list_`.
// -----------------------------------------------------------------------

fn feature_with_query(resource_name: &str, query: RuntimeQuery) -> RuntimeFeature {
    RuntimeFeature {
        name: resource_name.to_owned(),
        source_path: format!("features/{}/{}.lzi", resource_name, resource_name),
        resources: vec![RuntimeResource {
            name: resource_name.to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: Vec::new(),
        }],
        commands: Vec::new(),
        queries: vec![query],
    }
}

fn lookup_query(short_name: &str) -> RuntimeQuery {
    RuntimeQuery {
        short_name: short_name.to_owned(),
        kind: QueryKind::Lookup,
        policy_name: String::new(),
        policy_atoms: Vec::new(),
        args: Vec::new(),
        cache: None,
        paginate: 0,
        filters: Vec::new(),
        search: None,
        lookup_by: Vec::new(),
    }
}

fn list_query(short_name: &str) -> RuntimeQuery {
    RuntimeQuery {
        short_name: short_name.to_owned(),
        kind: QueryKind::List,
        policy_name: String::new(),
        policy_atoms: Vec::new(),
        args: Vec::new(),
        cache: None,
        paginate: 0,
        filters: Vec::new(),
        search: None,
        lookup_by: Vec::new(),
    }
}

#[test]
fn write_query_dedups_lookup_prefix_in_var_name() {
    // me-synth: query `lookup_my_host` in feature `host` →
    // `lookupMyHost` (not `hostLookupMyHost` or `lookupHostByLookupMyHost`).
    let out = emit_feature_ts(&feature_with_query("host", lookup_query("lookup_my_host")));
    assert!(
        out.contains("export const lookupMyHost = defineQuery"),
        "expected `lookupMyHost`, got:\n{out}"
    );
    assert!(
        !out.contains("hostLookupMyHost"),
        "legacy doubled-prefix shape leaked:\n{out}"
    );

    // crud-synth: query `lookup_traveler` in feature `traveler` → `lookupTraveler`.
    let out = emit_feature_ts(&feature_with_query(
        "traveler",
        lookup_query("lookup_traveler"),
    ));
    assert!(
        out.contains("export const lookupTraveler = defineQuery"),
        "expected `lookupTraveler`, got:\n{out}"
    );
}

#[test]
fn write_query_dedups_list_prefix_in_var_name() {
    // crud-synth: query `list_travelers` in feature `traveler` → `listTravelers`
    // (not `listTravelers` from `list<R>s` collision with `list_<r>s`).
    let out = emit_feature_ts(&feature_with_query(
        "traveler",
        list_query("list_travelers"),
    ));
    assert!(
        out.contains("export const listTravelers = defineQuery"),
        "expected `listTravelers`, got:\n{out}"
    );
}

#[test]
fn write_query_pluralizes_list_resource_names() {
    let out = emit_feature_ts(&feature_with_query("category", list_query("list")));
    assert!(
        out.contains("export interface ListCategoriesArgs"),
        "expected `ListCategoriesArgs`, got:\n{out}"
    );
    assert!(
        out.contains("export const listCategories = defineQuery"),
        "expected `listCategories`, got:\n{out}"
    );
    assert!(
        out.contains("/** @deprecated use `ListCategoriesArgs` */"),
        "expected deprecated args alias, got:\n{out}"
    );
    assert!(
        out.contains("export type ListCategorysArgs = ListCategoriesArgs;"),
        "expected legacy args alias, got:\n{out}"
    );
    assert!(
        out.contains("/** @deprecated use `listCategories` */"),
        "expected deprecated const alias, got:\n{out}"
    );
    assert!(
        out.contains("export const listCategorys = listCategories;"),
        "expected legacy const alias, got:\n{out}"
    );

    let out = emit_feature_ts(&feature_with_query("property", list_query("list")));
    assert!(out.contains("export const listProperties = defineQuery"));
    assert!(!out.contains("listPropertys = defineQuery"));

    let out = emit_feature_ts(&feature_with_query("payment", list_query("list")));
    assert!(out.contains("export const listPayments = defineQuery"));
    assert!(!out.contains("listPaymentss = defineQuery"));

    let out = emit_feature_ts(&feature_with_query(
        "custom_service_category",
        list_query("list_custom_service_categorys"),
    ));
    assert!(out.contains("export const listCustomServiceCategories = defineQuery"));
    assert!(
        out.contains("export const listCustomServiceCategorys = listCustomServiceCategories;")
    );
}

#[test]
fn write_query_keeps_plural_resource_names_stable() {
    let out = emit_feature_ts(&feature_with_query("payments", list_query("list")));
    assert!(out.contains("export interface ListPaymentsArgs"));
    assert!(out.contains("export const listPayments = defineQuery"));
    assert!(out.contains("export type ListPaymentssArgs = ListPaymentsArgs;"));
    assert!(out.contains("export const listPaymentss = listPayments;"));
}

#[test]
fn write_query_dedups_list_resource_suffix() {
    let out = emit_feature_ts(&feature_with_query(
        "host",
        list_query("pending_basic_details_hosts"),
    ));
    assert!(
        out.contains("export const listPendingBasicDetailsHosts = defineQuery"),
        "expected deduped host suffix, got:\n{out}"
    );

    let out = emit_feature_ts(&feature_with_query(
        "service_transaction",
        list_query("mine_transactions_as_host"),
    ));
    assert!(
        out.contains("export const listMineHostServiceTransactions = defineQuery"),
        "expected embedded transaction noun cleanup, got:\n{out}"
    );
}

#[test]
fn write_query_aliases_legacy_lookup_shape_when_resource_is_in_short_name() {
    // Lookup without `lookup_` prefix now skips the duplicated resource
    // prefix when the short name already contains the resource tail.
    let out = emit_feature_ts(&feature_with_query("host", lookup_query("my_host")));
    assert!(
        out.contains("export const lookupMyHost = defineQuery"),
        "expected `lookupMyHost`, got:\n{out}"
    );
    assert!(
        out.contains("export const hostMyHost = lookupMyHost;"),
        "expected deprecated legacy lookup alias, got:\n{out}"
    );
}
