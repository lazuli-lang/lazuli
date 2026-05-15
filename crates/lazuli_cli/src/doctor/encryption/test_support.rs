//! Test-only fixture helpers shared by the encryption doctor rule
//! modules. Each rule's positive/negative tests construct an
//! `AppManifest` + `Feature` skeleton via these helpers so the
//! per-rule files stay focused on the rule logic.
//!
//! Only compiled under `#[cfg(test)]`.

use lazuli_ir::{
    AppManifest, CapabilityRef, Defaults, E2eeCapability, EncryptedCapability, EncryptionAlgorithm,
    EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, Event, EventField,
    EventKind, Feature, Field, FieldConstraints, Policies, Resource, TypeRef,
};

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
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
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
        span_ref: None,
    }
}

pub fn make_binding(scope: &str, template_literal: &str) -> EncryptionBinding {
    EncryptionBinding {
        scope: scope.into(),
        source: EncryptionSource::Env(EncryptionTemplate::parse(template_literal)),
        algorithm: EncryptionAlgorithm::Aes256Gcm,
        rotation: EncryptionRotation::Manual,
        span_ref: None,
    }
}

pub fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.into(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        defaults: Defaults::default(),
        uses: vec![],
        requirements: vec![],
        enums: vec![],
        resources: vec![],
        events: vec![],
        rules: vec![],
        policies: Policies::default(),
        commands: vec![],
        apis: vec![],
        records: vec![],
        queries: vec![],
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
        previous_names: vec![],
        span_ref: None,
    }
}

pub fn encrypted_field(name: &str, key_scope: &str) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: key_scope.into(),
        })),
        required: false,
        unique: false,
        default: None,
        derived_from: None,
        constraints: FieldConstraints::default(),
        previous_names: vec![],
        span_ref: None,
    }
}

pub fn e2ee_field(name: &str, key_scope: &str) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
            key: key_scope.into(),
        })),
        required: false,
        unique: false,
        default: None,
        derived_from: None,
        constraints: FieldConstraints::default(),
        previous_names: vec![],
        span_ref: None,
    }
}

pub fn resource_with_fields(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.into(),
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
    }
}

pub fn event_with_payload(name: &str, fields: Vec<EventField>) -> Event {
    Event {
        name: name.into(),
        kind: EventKind::Domain,
        payload: fields,
        payload_none: false,
        level: None,
        previous_names: vec![],
        span_ref: None,
    }
}

pub fn event_field(name: &str, type_ref: TypeRef) -> EventField {
    EventField {
        name: name.into(),
        type_ref,
        optional: false,
    }
}
