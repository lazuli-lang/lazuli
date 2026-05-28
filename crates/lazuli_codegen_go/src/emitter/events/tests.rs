//! Inline tests for `events::emit_events_file`. Split out of
//! `mod.rs` to keep production under 500 LOC per the Rails-style
//! file budget.

use super::*;
use crate::emitter::cross_feature::CrossFeatureIndex;
use lazuli_ir::{
    AppManifest, BuiltinType, Defaults, EventField, EventKind, Field, Module, OutboxMode,
    Policies, Resource, TypeRef,
};

fn base_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies {
            categories: Vec::new(),
            fields: Vec::new(),
            span_ref: None,
        },
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

fn minimal_app() -> AppManifest {
    AppManifest {
        name: "test".to_owned(),
        title: None,
        version: None,
        lazuli_version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        error_pages: Vec::new(),
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
        cookie: None,
        proxy: None,
        limits: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        observability: None,
        locale: None,
        encryption_bindings: Vec::new(),
        route_guard: None,
        actor_query: None,
        span_ref: None,
    }
}

fn module_with_feature(feature: Feature) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app()),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}

fn emit(feature: &Feature) -> Option<String> {
    let module = module_with_feature(feature.clone());
    let index = CrossFeatureIndex::build(&module);
    emit_events_file("examples/x.lzi", feature, "lazuli/test", &index)
}

fn event_group(pattern: &str, events: Vec<&str>) -> EventGroup {
    let events: Vec<String> = events.into_iter().map(str::to_owned).collect();
    let events_outbox = vec![OutboxMode::None; events.len()];
    EventGroup {
        pattern: pattern.to_owned(),
        on_resource: Some("Customer".to_owned()),
        raw_payload: vec![
            "customer_id = id".to_owned(),
            "by_id = ctx.user.id when @actor.user".to_owned(),
        ],
        raw_audit: None,
        events,
        events_outbox,
        // B5 framework gap 1 — legacy fixtures author no typed
        // variants; `variants: Vec::new()` keeps the codegen on
        // the legacy `Feature.events` lookup path.
        variants: Vec::new(),
        span_ref: None,
    }
}

fn simple_resource(name: &str) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "email".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }],
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: vec![],
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    }
}

fn typed_event(name: &str, payload: Vec<EventField>) -> Event {
    Event {
        name: name.to_owned(),
        kind: EventKind::Domain,
        payload,
        payload_none: false,
        level: None,
        outbox: OutboxMode::None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn event_field(name: &str, builtin: BuiltinType, optional: bool) -> EventField {
    EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(builtin),
        optional,
    }
}

#[test]
fn empty_feature_returns_none() {
    let feature = base_feature("customer");
    assert!(emit(&feature).is_none());
}

#[test]
fn event_group_emits_runtime_todos_descriptors_and_inferred_payload() {
    let mut feature = base_feature("customer");
    feature.resources.push(simple_resource("Customer"));
    feature
        .event_groups
        .push(event_group("customer_*", vec!["zebra", "created"]));

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
    assert!(out.contains("package customergen"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
    assert!(out.contains("var customerCustomerEvents = lazuli.EventGroup{"));
    assert!(out.contains("// TODO(runtime): lazuli.EventGroup is missing"));
    assert!(out.contains("// TODO(runtime): lazuli.EventDescriptor is missing"));
    assert!(out.contains("Pattern:  \"customer_*\","));
    assert!(out.contains("Resource: \"Customer\","));

    let created_pos = out.find("Name: \"customer_created\"").unwrap();
    let zebra_pos = out.find("Name: \"customer_zebra\"").unwrap();
    assert!(created_pos < zebra_pos);
    assert!(out.contains("type CustomerCreatedPayload struct {"));
    assert!(out.contains("CustomerID lazuli.ID  `json:\"customer_id\"`"));
    assert!(out.contains("ByID       *lazuli.ID `json:\"by_id,omitempty\"`"));
}

#[test]
fn typed_event_payload_merges_with_matching_group_payload() {
    let mut feature = base_feature("customer");
    feature.resources.push(simple_resource("Customer"));
    feature
        .event_groups
        .push(event_group("customer_*", vec!["created"]));
    feature.events.push(typed_event(
        "customer_created",
        vec![
            event_field("email", BuiltinType::SemanticEmail, false),
            event_field("source", BuiltinType::Text, true),
        ],
    ));

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("type CustomerCreatedPayload struct {"));
    assert!(out.contains("CustomerID lazuli.ID    `json:\"customer_id\"`"));
    assert!(out.contains("Email      lazuli.Email `json:\"email\"`"));
    assert!(out.contains("Source     *string      `json:\"source,omitempty\"`"));
}

#[test]
fn standalone_typed_event_emits_payload_struct_without_group_literal() {
    let mut feature = base_feature("session");
    feature.events.push(typed_event(
        "customer_logged_in",
        vec![
            event_field("customer_id", BuiltinType::Id, false),
            event_field("provider", BuiltinType::Text, false),
        ],
    ));

    let out = emit(&feature).expect("must emit");
    assert!(!out.contains("lazuli.EventGroup"));
    assert!(out.contains("type CustomerLoggedInPayload struct {"));
    assert!(out.contains("CustomerID lazuli.ID `json:\"customer_id\"`"));
    assert!(out.contains("Provider   string    `json:\"provider\"`"));
}

/// B5 framework gap 1 — when the event_group carries typed
/// variants the emitter writes per-variant payload struct shapes
/// straight from `EventGroup.variants[i].fields`, no longer
/// requiring `Feature.events` to be populated. This is the
/// the canonical pilot failure mode: the legacy lookup returned the
/// envelope-only struct.
#[test]
fn typed_variants_under_group_drive_per_variant_payload_shapes() {
    let mut feature = base_feature("payments");
    feature.resources.push(simple_resource("Charge"));
    let mut group = event_group("charge_*", vec!["confirmed", "failed"]);
    group.variants = vec![
        lazuli_ir::EventVariant {
            name: "confirmed".to_owned(),
            kind: lazuli_ir::EventVariantKind::Committed,
            outbox: OutboxMode::Guaranteed,
            fields: vec![
                event_field("amount", BuiltinType::Decimal, false),
                event_field("provider_payment_id", BuiltinType::Text, false),
            ],
            span_ref: None,
        },
        lazuli_ir::EventVariant {
            name: "failed".to_owned(),
            kind: lazuli_ir::EventVariantKind::Committed,
            outbox: OutboxMode::Guaranteed,
            fields: vec![
                event_field("reason", BuiltinType::Text, false),
                event_field("provider_error_code", BuiltinType::Text, true),
            ],
            span_ref: None,
        },
    ];
    feature.event_groups.push(group);
    let out = emit(&feature).expect("must emit");
    // Confirmed gets typed fields.
    assert!(
        out.contains("type ChargeConfirmedPayload struct {"),
        "ChargeConfirmedPayload missing:\n{}",
        out
    );
    assert!(
        out.contains("ProviderPaymentID") && out.contains("`json:\"provider_payment_id\"`"),
        "ProviderPaymentID payload field missing:\n{out}"
    );
    // Failed gets its OWN typed fields (different from confirmed).
    assert!(out.contains("type ChargeFailedPayload struct {"));
    assert!(out.contains("Reason"));
    assert!(out.contains("ProviderErrorCode"));
}

#[test]
fn deterministic_across_runs_and_sorts_groups_by_pattern() {
    let mut feature = base_feature("customer");
    feature
        .event_groups
        .push(event_group("zebra_*", vec!["created"]));
    feature
        .event_groups
        .push(event_group("alpha_*", vec!["created"]));

    let a = emit(&feature).expect("must emit");
    let b = emit(&feature).expect("must emit");
    assert_eq!(a, b);

    let alpha_pos = a.find("EventGroup: customer.alpha_*").unwrap();
    let zebra_pos = a.find("EventGroup: customer.zebra_*").unwrap();
    assert!(alpha_pos < zebra_pos);
}
