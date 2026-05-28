//! Test-only fixture helpers shared by the encryption doctor rule
//! modules. Each rule's positive/negative tests construct an
//! `AppManifest` + `Feature` skeleton via these helpers so the
//! per-rule files stay focused on the rule logic.
//!
//! Only compiled under `#[cfg(test)]`.

use lazuli_ir::{
    AppManifest, CapabilityRef, Defaults, E2eeCapability, EncryptedCapability, EncryptionAlgorithm,
    EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, Event, EventField,
    EventKind, Feature, Field, FieldConstraints, OutboxMode, Policies, Resource, TypeRef,
};

/// Build an empty `AppManifest` skeleton used by encryption-rule tests
/// as a starting point — every field zeroed / defaulted, ready for the
/// caller to push `encryption_bindings` etc. before invoking a rule.
///
/// ## Examples
///
/// ```ignore
/// let mut app = empty_app();
/// app.encryption_bindings.push(make_binding("@key.tenant", "CRYPT_KEY"));
/// ```
pub fn empty_app() -> AppManifest {
    AppManifest {
        name: "TestApp".into(),
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

/// Build an `EncryptionBinding` with sensible defaults (`Aes256Gcm`,
/// `Manual` rotation, `Env` source). `template_literal` is the env-var
/// name to surface in the binding's source.
///
/// ## Examples
///
/// ```ignore
/// let b = make_binding("@key.tenant", "CRYPT_KEY_TENANT_{tenant_id}");
/// ```
pub fn make_binding(scope: &str, template_literal: &str) -> EncryptionBinding {
    EncryptionBinding {
        scope: scope.into(),
        source: EncryptionSource::Env(EncryptionTemplate::parse(template_literal)),
        algorithm: EncryptionAlgorithm::Aes256Gcm,
        rotation: EncryptionRotation::Manual,
        rotation_profile: None,
        span_ref: None,
    }
}

/// Build an empty `Feature` skeleton — every collection zero-length,
/// every `Option` field `None`. Callers push the constructs the rule
/// under test exercises (resources, events, ...).
///
/// ## Examples
///
/// ```ignore
/// let mut feature = empty_feature("customer");
/// feature.resources.push(resource_with_fields("Customer", vec![]));
/// ```
pub fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.into(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        defaults: Defaults::default(),
        uses: vec![],
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: vec![],
        enums: vec![],
        resources: vec![],
        events: vec![],
        rules: vec![],
        policies: Policies::default(),
        errors: None,
        commands: vec![],
        apis: vec![],
        records: vec![],
        queries: vec![],
        resume_routers: vec![],
        workflows: vec![],
        jobs: vec![],
        webhooks: vec![],
        notifications: vec![],
        event_groups: vec![],
        tenant_migrations: vec![],
        translation: None,
        auth: None,
        surfaces: vec![],
        extensions: vec![],
        escape_routes: vec![],
        agents: vec![],
        pollers: vec![],
        reports: vec![],
        channels: vec![],
        caches: vec![],
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: vec![],
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

/// Build a `Field` typed as `@cap.Encrypted(key:@key.<scope>)` for use
/// inside test-fixture resources.
///
/// ## Examples
///
/// ```ignore
/// let f = encrypted_field("external_id", "@key.tenant");
/// ```
pub fn encrypted_field(name: &str, key_scope: &str) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: key_scope.into(),
        })),
        required: false,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: vec![],
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

/// Build a `Field` typed as `@cap.E2ee(key:@key.<scope>)` for use
/// inside test-fixture resources.
///
/// ## Examples
///
/// ```ignore
/// let f = e2ee_field("body", "@key.user");
/// ```
pub fn e2ee_field(name: &str, key_scope: &str) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
            key: key_scope.into(),
        })),
        required: false,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: vec![],
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

/// Build a `Resource` named `name` carrying the given fields. All
/// other knobs are defaulted (no tenancy, no soft-delete, no
/// lifecycle).
///
/// ## Examples
///
/// ```ignore
/// let r = resource_with_fields("Customer", vec![encrypted_field("ext_id", "@key.tenant")]);
/// ```
pub fn resource_with_fields(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.into(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: vec![],
        validate: None,
        validates: vec![],
        retention: None,
        previous_names: vec![],
        span_ref: None,
        lifecycle: None,
        invariants: vec![],

        lock: None,

        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        append_only: false,
    }
}

/// Build a domain `Event` named `name` carrying the given payload
/// fields. Outbox mode is `None`, level defaulted.
///
/// ## Examples
///
/// ```ignore
/// let e = event_with_payload("message_sent", vec![]);
/// ```
pub fn event_with_payload(name: &str, fields: Vec<EventField>) -> Event {
    Event {
        name: name.into(),
        kind: EventKind::Domain,
        payload: fields,
        payload_none: false,
        level: None,
        outbox: OutboxMode::None,
        previous_names: vec![],
        span_ref: None,
    }
}

/// Build an `EventField` with `optional: false` carrying the given
/// type reference (typically a `TypeRef::Capability(...)`).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_ir::TypeRef;
/// let _ = event_field("body", TypeRef::Capability(unimplemented!()));
/// ```
pub fn event_field(name: &str, type_ref: TypeRef) -> EventField {
    EventField {
        name: name.into(),
        type_ref,
        optional: false,
    }
}
