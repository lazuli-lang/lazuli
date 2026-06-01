//! Lifecycle + ConventionRef serde round-trips. Previously lived inline as
//! `mod lifecycle_tests` in `lazuli_ir/src/lib.rs`; promoted to an
//! integration test by Wave R3-F Stage 3 to shrink the lib root toward the
//! ≤ 1000 LOC gold-standard target.
//!
//! These tests only touch the public `lazuli_ir` ABI surface — they belong
//! at the integration layer.

use lazuli_ir::{
    BuiltinType, ConventionRef, Lifecycle, LifecycleInvariant, LifecycleState, LifecycleStateKind,
    LifecycleTransition, Resource,
};

#[test]
fn lifecycle_round_trips_through_json() {
    let lc = Lifecycle {
        discriminator_field: "status".to_owned(),
        generated_enum: "PublicationStatus".to_owned(),
        states: vec![
            LifecycleState {
                name: "scheduled".to_owned(),
                kind: LifecycleStateKind::Initial,
                span_ref: None,
            },
            LifecycleState {
                name: "published".to_owned(),
                kind: LifecycleStateKind::Terminal,
                span_ref: None,
            },
        ],
        transitions: vec![LifecycleTransition {
            name: "publish".to_owned(),
            from: vec!["scheduled".to_owned()],
            to: "published".to_owned(),
            policy: None,
            audit: None,
            timestamps: Some("published_at".to_owned()),
            emits: vec!["publication_published".to_owned()],
            requires: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }],
        invariants: vec![LifecycleInvariant::TerminalImmutable],
        invariant_handlers: vec![],
        previous_names: vec![],
        span_ref: None,
    };
    let json = serde_json::to_string(&lc).unwrap();
    let back: Lifecycle = serde_json::from_str(&json).unwrap();
    assert_eq!(lc, back);
    assert!(json.contains("\"kind\":\"terminal_immutable\""));
}

#[test]
fn lifecycle_invariant_single_state_per_scope_tagged_correctly() {
    let inv = LifecycleInvariant::SingleStatePerScope {
        state: "gold".to_owned(),
        scope_field: "item_id".to_owned(),
    };
    let json = serde_json::to_string(&inv).unwrap();
    assert!(json.contains("\"kind\":\"single_state_per_scope\""));
    assert!(json.contains("\"state\":\"gold\""));
    assert!(json.contains("\"scope_field\":\"item_id\""));
}

#[test]
fn semantic_plugin_type_round_trips_through_json() {
    // B3 — see `docs/proposals/semantic-types-plugin-locales.md`.
    let plugin = BuiltinType::SemanticPluginType {
        plugin: "@lazuli/plugin-scalars-br".to_owned(),
        name: "BrazilianCPF".to_owned(),
        carrier: Box::new(BuiltinType::Text),
        validator: "ValidateCPF".to_owned(),
        go_module: String::new(),
        ts_package: String::new(),
        error_code: String::new(),
        message_key: String::new(),
        ts_validator: String::new(),
    };
    let json = serde_json::to_string(&plugin).expect("serialize");
    let back: BuiltinType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(plugin, back);
    // Round-trip-stable. The variant name landing in the JSON keeps
    // the inspect serde-default path readable for cold readers.
    assert!(json.contains("SemanticPluginType"));
    assert!(json.contains("BrazilianCPF"));
    assert!(json.contains("ValidateCPF"));
}

#[test]
fn resource_with_no_lifecycle_omits_field_in_json() {
    let r = Resource {
        name: "Publication".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![],
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
        conventions: vec![],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    };
    let json = serde_json::to_string(&r).unwrap();
    assert!(
        !json.contains("\"lifecycle\""),
        "skip_serializing_if = Option::is_none should drop the field"
    );
}

#[test]
fn convention_ref_crud_serializes_snake_case() {
    // §4.2 requires `#[serde(rename_all = "snake_case")]` so the
    // variant name on the wire matches the .lzi keyword (`crud`).
    let json = serde_json::to_string(&ConventionRef::Crud).unwrap();
    assert_eq!(json, "\"crud\"");

    let back: ConventionRef = serde_json::from_str("\"crud\"").unwrap();
    assert_eq!(back, ConventionRef::Crud);
}

#[test]
fn convention_ref_me_serializes_snake_case() {
    // `ir-resource-conventions-me.md` §4.2 — the `Me` variant
    // serializes as `"me"` on the wire to match the .lzi
    // keyword and the parser catalog identifier.
    let json = serde_json::to_string(&ConventionRef::Me).unwrap();
    assert_eq!(json, "\"me\"");

    let back: ConventionRef = serde_json::from_str("\"me\"").unwrap();
    assert_eq!(back, ConventionRef::Me);
}

#[test]
fn resource_conventions_round_trip_with_crud() {
    // Resource.conventions is the slot Cell C1 adds. Round-trip
    // through serde to lock the JSON shape before Cell C2 (parser)
    // starts producing it.
    let r = Resource {
        name: "Customer".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![],
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
        conventions: vec![ConventionRef::Crud],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    };
    let json = serde_json::to_string(&r).unwrap();
    assert!(
        json.contains("\"conventions\":[\"crud\"]"),
        "expected the populated conventions list to serialize as snake_case identifiers, got: {json}"
    );
    let back: Resource = serde_json::from_str(&json).unwrap();
    assert_eq!(back.conventions, vec![ConventionRef::Crud]);
}

#[test]
fn resource_conventions_absent_field_deserializes_empty() {
    // `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
    // means existing fixtures (pre-Cell-C1) that lack the field
    // continue to deserialize cleanly into an empty Vec.
    let legacy_json = r#"{
        "name": "Legacy",
        "soft_delete": false,
        "fields": []
    }"#;
    let r: Resource = serde_json::from_str(legacy_json).unwrap();
    assert!(r.conventions.is_empty());

    // Round-trip the empty Vec drops the key entirely.
    let json = serde_json::to_string(&r).unwrap();
    assert!(
        !json.contains("\"conventions\""),
        "skip_serializing_if = Vec::is_empty should drop the field; got: {json}"
    );
}
