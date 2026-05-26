//! Tests for `parse_app_registry` — package registry, integrations,
//! bindings sugar, webhook events (canonical + legacy), and
//! `secret_rotation` profiles. Lives alongside `registry.rs`.

#![cfg(test)]

use super::parse_app_registry;

#[test]
fn parses_package_registry() {
    let source = r#"
registry
  env
    group mercadopago
      server MERCADOPAGO_ACCESS_TOKEN: Secret required in production
  capabilities
    payment_gateway mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
      environments sandbox, production
      credentials platform
        access_token env.MERCADOPAGO_ACCESS_TOKEN
"#;

    let registry = parse_app_registry(source).unwrap();

    assert_eq!(registry.env[0].group.as_deref(), Some("mercadopago"));
    assert_eq!(registry.capabilities[0].name, "payment_gateway");
    assert_eq!(registry.packs[0].name, "payments");
    assert_eq!(registry.packs[0].source, "@runtime/payments");
    assert_eq!(registry.packs[0].version.as_deref(), Some("0.1.0"));
    assert_eq!(registry.packs[0].provides[0].kind, "feature");
    assert_eq!(registry.packs[0].provides[0].name, "payments");
    assert_eq!(registry.packs[0].requirements[0].kind, "integration");
    assert_eq!(registry.packs[0].requirements[0].name, "gateway");
    assert_eq!(registry.packs[0].requirements[0].contract, "PaymentGateway");
    assert_eq!(registry.integrations[0].name, "mercadopago");
    assert_eq!(registry.integrations[0].kind, "PaymentGateway");
    assert_eq!(
        registry.integrations[0].adapter_provenance.as_deref(),
        Some("runtime")
    );
    assert_eq!(
        registry.integrations[0]
            .credentials
            .as_ref()
            .and_then(|credentials| credentials.bindings.first())
            .map(|binding| binding.source.as_str()),
        Some("env.MERCADOPAGO_ACCESS_TOKEN")
    );
}

#[test]
fn parses_registry_bindings_sugar_lowers_to_integration_credentials() {
    // B1 (W3-blockers) — `bindings` is registry-level sugar over
    // `integrations`. The simplified shape (endpoint + auth keys)
    // lowers to the canonical `credentials platform` + bindings.
    let source = r#"
registry
  bindings
    object_store: ObjectStore
      adapter @lazuli/plugin-object-store
      endpoint env.S3_ENDPOINT
      auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
"#;

    let registry = parse_app_registry(source).expect("registry");
    assert_eq!(registry.integrations.len(), 1);
    let integration = &registry.integrations[0];
    assert_eq!(integration.name, "object_store");
    assert_eq!(integration.kind, "ObjectStore");
    assert_eq!(
        integration.adapter.as_deref(),
        Some("@lazuli/plugin-object-store")
    );
    assert_eq!(integration.adapter_provenance.as_deref(), Some("plugin"));

    let credentials = integration
        .credentials
        .as_ref()
        .expect("sugar must synthesize implicit `credentials platform`");
    assert_eq!(credentials.scope, "platform");

    // Sugar lowers to three credential bindings in declaration order:
    // endpoint (from `endpoint`), access_key_id + secret_access_key
    // (from positional `auth keys`).
    let by_name: std::collections::BTreeMap<&str, &str> = credentials
        .bindings
        .iter()
        .map(|binding| (binding.name.as_str(), binding.source.as_str()))
        .collect();
    assert_eq!(by_name.get("endpoint"), Some(&"env.S3_ENDPOINT"));
    assert_eq!(by_name.get("access_key_id"), Some(&"env.S3_ACCESS_KEY_ID"));
    assert_eq!(
        by_name.get("secret_access_key"),
        Some(&"env.S3_SECRET_ACCESS_KEY")
    );
}

#[test]
fn registry_bindings_additive_with_integrations_block() {
    // The legacy `integrations` block must still parse alongside the
    // new `bindings` block — additive, not breaking.
    let source = r#"
registry
  integrations
    payment_gateway: PaymentGateway
      adapter @lazuli/plugin-mercadopago
  bindings
    object_store: ObjectStore
      adapter @lazuli/plugin-object-store
      endpoint env.S3_ENDPOINT
      auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
"#;

    let registry = parse_app_registry(source).expect("registry");
    assert_eq!(registry.integrations.len(), 2);
    assert_eq!(registry.integrations[0].name, "payment_gateway");
    assert_eq!(registry.integrations[1].name, "object_store");
    // Legacy integration carries no synthesized credentials.
    assert!(registry.integrations[0].credentials.is_none());
    // Sugar integration carries the synthesized `platform` scope.
    assert_eq!(
        registry.integrations[1]
            .credentials
            .as_ref()
            .map(|credentials| credentials.scope.as_str()),
        Some("platform")
    );
}

#[test]
fn parses_webhook_event_registry_kind_with_payload_and_version() {
    let source = r#"
registry MyApp
  webhook_event customer.created
    payload
      customer_id: ID
      email: @semantic.Email
      created_at: DateTime
    version 1
    deprecated false
"#;

    let registry = parse_app_registry(source).unwrap();
    let event = &registry.webhook_events[0];

    assert_eq!(event.name, "customer.created");
    assert_eq!(event.version, 1);
    assert_eq!(event.previous_version, None);
    assert!(!event.deprecated);
    assert_eq!(event.payload.len(), 3);
    assert_eq!(event.payload[1].name, "email");
    assert_eq!(event.payload[1].type_text, "@semantic.Email");
    assert!(event.payload[1].required);
}

#[test]
fn parses_webhook_event_registry_kind_with_previous_version() {
    let source = r#"
registry
  webhook_event customer.archived
    payload
      customer_id: ID
      reason: Text
    version 2
    previous_version 1
"#;

    let registry = parse_app_registry(source).unwrap();
    let event = &registry.webhook_events[0];

    assert_eq!(event.name, "customer.archived");
    assert_eq!(event.version, 2);
    assert_eq!(event.previous_version, Some(1));
}

#[test]
fn parses_webhook_event_registry_kind_with_deprecated_true() {
    let source = r#"
registry
  webhook_event customer.deleted
    payload
      customer_id: ID
    version 3
    previous_version 2
    deprecated true
"#;

    let registry = parse_app_registry(source).unwrap();
    let event = &registry.webhook_events[0];

    assert_eq!(event.name, "customer.deleted");
    assert!(event.deprecated);
}

#[test]
fn parses_legacy_webhook_events_block_as_registry_payload() {
    let source = r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required
      email: @semantic.Email @pii.contact optional
"#;

    let registry = parse_app_registry(source).unwrap();
    let event = &registry.webhook_events[0];

    assert_eq!(event.name, "crm_customer_upsert");
    assert_eq!(event.version, 1);
    assert_eq!(event.payload.len(), 2);
    assert_eq!(event.payload[1].capabilities, ["@pii.contact"]);
    assert!(!event.payload[1].required);
}

// -------------------------------------------------------------
// Roadmap §1.10 — `registry.secret_rotation` parser tests.
// Three+ cases per primitive: single profile parses, multiple
// profiles round-trip, encryption.key binding picks up the
// referenced profile name.
// -------------------------------------------------------------

#[test]
fn parses_registry_secret_rotation_default_profile() {
    let source = r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#;
    let registry = parse_app_registry(source).expect("registry");
    assert_eq!(registry.secret_rotations.len(), 1);
    let profile = &registry.secret_rotations[0];
    assert_eq!(profile.name, "default");
    assert_eq!(profile.cadence, "90d");
    assert_eq!(profile.overlap, "24h");
    assert!(profile.auto_rollback);
}

#[test]
fn parses_registry_secret_rotation_multiple_profiles() {
    let source = r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true

  secret_rotation tenant_keys
    cadence 30d
    overlap 0h
    auto_rollback false
"#;
    let registry = parse_app_registry(source).expect("registry");
    assert_eq!(registry.secret_rotations.len(), 2);
    assert_eq!(registry.secret_rotations[0].name, "default");
    assert_eq!(registry.secret_rotations[1].name, "tenant_keys");
    assert_eq!(registry.secret_rotations[1].cadence, "30d");
    assert_eq!(registry.secret_rotations[1].overlap, "0h");
    assert!(!registry.secret_rotations[1].auto_rollback);
}

#[test]
fn parses_registry_secret_rotation_absent_yields_empty_catalog() {
    let source = r#"
registry
  env
    server CRYPT_KEY: Secret required
"#;
    let registry = parse_app_registry(source).expect("registry");
    assert!(registry.secret_rotations.is_empty());
}
