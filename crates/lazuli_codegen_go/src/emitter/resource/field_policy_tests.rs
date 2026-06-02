//! Tests for `field_policy.rs` — the `FieldReadPolicies` emission on the
//! `Resource[T]` value (W1-2 SEC-FIELDPOLICY-READ-NULL).

use super::test_support::{base_feature, hashed_field, simple_field, simple_resource};
use super::test_support::emit;
use lazuli_ir::{BuiltinType, FieldPolicies, FieldPolicy, QualifiedName};

/// Build a `User` resource with `id` (implicit) + `email` + a hashed
/// `password_hash` field, attach `policies fields User` for the requested
/// read atoms, and emit the resource file.
fn emit_user_with_read(field: &str, read_atoms: Vec<&str>) -> String {
    let mut feature = base_feature("account");
    feature.resources.push(simple_resource(
        "User",
        vec![
            simple_field("email", BuiltinType::Text, true),
            hashed_field("password_hash", true),
        ],
    ));
    feature.policies.fields.push(FieldPolicies {
        resource: QualifiedName {
            feature: None,
            name: "User".to_owned(),
        },
        fields: vec![FieldPolicy {
            field: field.to_owned(),
            read: Some(read_atoms.into_iter().map(str::to_owned).collect()),
            write: None,
            previous_names: Vec::new(),
        }],
    });
    emit(&feature).expect("resource file should emit")
}

#[test]
fn actor_system_read_emits_field_read_policy_entry() {
    let out = emit_user_with_read("password_hash", vec!["@actor.system"]);
    assert!(
        out.contains("FieldReadPolicies: map[string]lazuli.Policy{"),
        "expected FieldReadPolicies map, got:\n{out}"
    );
    assert!(
        out.contains(
            "\"password_hash\": {Name: \"@actor.system\", Atoms: []lazuli.PolicyAtom{{Namespace: \"actor\", Name: \"system\"}}},"
        ),
        "expected the password_hash @actor.system gate, got:\n{out}"
    );
}

#[test]
fn role_read_emits_runtime_evaluable_gate() {
    let out = emit_user_with_read("email", vec!["@role.ADMIN"]);
    assert!(out.contains("FieldReadPolicies: map[string]lazuli.Policy{"));
    assert!(
        out.contains(
            "\"email\": {Name: \"@role.ADMIN\", Atoms: []lazuli.PolicyAtom{{Namespace: \"role\", Name: \"ADMIN\"}}},"
        ),
        "got:\n{out}"
    );
}

#[test]
fn multi_atom_read_wraps_in_or_group() {
    let out = emit_user_with_read("email", vec!["@role.ADMIN", "@scope.same_org"]);
    assert!(
        out.contains(
            "Atoms: []lazuli.PolicyAtom{{Namespace: \"predicate\", Name: \"(\"}, {Namespace: \"role\", Name: \"ADMIN\"}, {Namespace: \"predicate\", Name: \"or\"}, {Namespace: \"scope\", Name: \"same_org\"}, {Namespace: \"predicate\", Name: \")\"}}"
        ),
        "expected OR-wrapped atom group, got:\n{out}"
    );
}

#[test]
fn non_runtime_evaluable_read_is_skipped_with_todo() {
    // `@actor.role.requires(ADMIN)` and `@actor.self` have no runtime
    // evaluation; the gate is skipped (no map entry for the column) and a
    // TODO is emitted so the deferral is visible.
    let out = emit_user_with_read(
        "password_hash",
        vec!["@actor.self", "@actor.role.requires(ADMIN)"],
    );
    assert!(
        !out.contains("FieldReadPolicies:"),
        "non-evaluable-only gate must NOT emit a map, got:\n{out}"
    );
}

#[test]
fn mixed_gate_emits_evaluable_and_todo_for_deferred() {
    // password_hash (@actor.system, evaluable) is emitted; tenant_id
    // (@actor.self | @actor.role.requires(ADMIN), the verbatim compound) is
    // deferred. The evaluable gate emits a map; the deferred one triggers
    // the TODO comment.
    let mut feature = base_feature("account");
    feature.resources.push(simple_resource(
        "User",
        vec![
            simple_field("tenant_id", BuiltinType::Text, true),
            hashed_field("password_hash", true),
        ],
    ));
    feature.policies.fields.push(FieldPolicies {
        resource: QualifiedName {
            feature: None,
            name: "User".to_owned(),
        },
        fields: vec![
            FieldPolicy {
                field: "password_hash".to_owned(),
                read: Some(vec!["@actor.system".to_owned()]),
                write: None,
                previous_names: Vec::new(),
            },
            FieldPolicy {
                field: "tenant_id".to_owned(),
                // The parser keeps a `|`-compound as ONE verbatim atom.
                read: Some(vec!["@actor.self | @actor.role.requires(ADMIN)".to_owned()]),
                write: None,
                previous_names: Vec::new(),
            },
        ],
    });
    let out = emit(&feature).expect("emit");
    assert!(out.contains("FieldReadPolicies: map[string]lazuli.Policy{"));
    assert!(out.contains("\"password_hash\": {Name: \"@actor.system\""));
    assert!(
        !out.contains("\"tenant_id\":"),
        "deferred compound gate must not be emitted, got:\n{out}"
    );
    assert!(
        out.contains("TODO(runtime): role-gated reads"),
        "expected deferred-gate TODO, got:\n{out}"
    );
}

#[test]
fn no_field_policies_emits_no_map() {
    let mut feature = base_feature("account");
    feature.resources.push(simple_resource(
        "User",
        vec![simple_field("email", BuiltinType::Text, true)],
    ));
    let out = emit(&feature).expect("emit");
    assert!(!out.contains("FieldReadPolicies:"));
}
