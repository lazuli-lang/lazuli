//! Scope-owner tests — exercise `@scope.owner` / `@scope.same_org`
//! atom lowering through `emit_command_file`. Lifted out of
//! `scope.rs` (wave R8-2c) so the parent file stays under the
//! ≤500-LOC gold standard while keeping every test co-located with
//! the production code it exercises (sibling, not orphaned).
//!
//! Coverage cluster: the analyzer-supplied policy atoms
//! (`@scope.owner`, `@scope.same_org`) drive an auto-injected
//! `FromCtx(...)` WHERE binding when the resource carries a matching
//! owner-like column. This file owns the happy-path and silent-skip
//! shapes for that cluster:
//!   - direct column match (`user_id`)
//!   - fallback through closed-catalog priority (`user`)
//!   - `@scope.same_org` → `org_id`
//!   - no `@scope.*` atom → baseline (no injection)
//!   - traversal via relation (`Property.host → Host.user_id`)
//!   - no matching column → silent skip (doctor handles surfacing)
//!
//! The companion files `scope_where_keys_tests.rs` and
//! `owner_scope_sql_tests.rs` cover the WHERE-key resolution and
//! `OwnerScopeSql` projection sub-concerns respectively.

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, scope_field,
    simple_resource, typed_slot,
};
use lazuli_ir::{
    BuiltinType, CommandEffect, CommandInput, DeleteEffect, Feature, Policies, PolicyRef, TypeRef,
    UpdateEffect,
};

fn feature_with_owner_scope_policy() -> Feature {
    let mut feature = base_feature("account");
    let mut resource = simple_resource("UserSession");
    resource.fields.push(scope_field("user_id"));
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "delete".to_owned(),
            atoms: vec!["@scope.owner".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    feature
}

#[test]
fn deletes_with_scope_owner_injects_user_id_where_binding() {
    let mut feature = feature_with_owner_scope_policy();
    let mut cmd = base_command("revoke_session");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("UserSession"),
    });
    cmd.policy = PolicyRef::Local("delete".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "still binds id from input:\n{out}"
    );
    assert!(
        out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
        "@scope.owner should inject user_id WHERE binding:\n{out}"
    );
    assert!(
        out.contains("// scope: @scope.owner resolved → user_id = ctx.user.id"),
        "scope comment should surface for reviewers:\n{out}"
    );
}

#[test]
fn updates_with_scope_owner_injects_user_where_binding_when_user_id_absent() {
    // Resource has `user` (not `user_id`) — closed-catalog falls
    // through to the second candidate per priority.
    let mut feature = base_feature("messaging");
    let mut resource = simple_resource("NotificationDelivery");
    resource.fields.push(scope_field("user"));
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "update".to_owned(),
            atoms: vec!["@scope.owner".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("mark_notification_read");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("NotificationDelivery"),
        assignments: Vec::new(),
    });
    cmd.policy = PolicyRef::Local("update".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
        "@scope.owner should resolve to `user` field when user_id absent:\n{out}"
    );
}

#[test]
fn updates_with_scope_same_org_injects_org_id_where_binding() {
    let mut feature = base_feature("billing");
    let mut resource = simple_resource("Charge");
    resource.fields.push(scope_field("org_id"));
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "update".to_owned(),
            atoms: vec!["@scope.same_org".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("flag_review");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Charge"),
        assignments: Vec::new(),
    });
    cmd.policy = PolicyRef::Local("update".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"org_id\": lazuli.FromCtx(\"user.org_id\"),"),
        "@scope.same_org should inject org_id WHERE binding:\n{out}"
    );
}

#[test]
fn no_scope_atom_emits_baseline_where_binding() {
    let mut feature = base_feature("account");
    let mut resource = simple_resource("UserSession");
    resource.fields.push(scope_field("user_id"));
    feature.resources.push(resource);
    // No @scope.* atom in the policy.
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "admin".to_owned(),
            atoms: vec!["@role.admin".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("purge");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("UserSession"),
    });
    cmd.policy = PolicyRef::Local("admin".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    // Baseline binds id from input. No scope injection.
    assert!(out.contains("\"id\": lazuli.FromInput(\"ID\"),"));
    assert!(
        !out.contains("FromCtx(\"user.id\")"),
        "no @scope.* atom → no auto-injected scope binding:\n{out}"
    );
}

#[test]
fn updates_with_scope_owner_traverses_relation_when_no_direct_column() {
    // Property has no direct owner column but `host: Host required`
    // references the Host resource which has `user_id`. Codegen
    // should emit FromCtxOwnedVia("Host", "user_id", "user.id").
    let mut feature = base_feature("catalog");

    let mut host = simple_resource("Host");
    host.fields.push(scope_field("user_id"));
    feature.resources.push(host);

    let mut property = simple_resource("Property");
    // `host` field referencing the Host resource.
    property.fields.push(lazuli_ir::Field {
        name: "host".to_owned(),
        type_ref: TypeRef::Unresolved("Host".to_owned()),
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
    });
    feature.resources.push(property);

    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "update".to_owned(),
            atoms: vec!["@scope.owner".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };

    let mut cmd = base_command("publish_property");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Property"),
        assignments: Vec::new(),
    });
    cmd.policy = PolicyRef::Local("update".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"host\": lazuli.FromCtxOwnedVia(\"Host\", \"user_id\", \"user.id\"),"),
        "@scope.owner should traverse host → Host.user_id when Property has no direct column:\n{out}"
    );
    assert!(
        out.contains("// scope: @scope.owner resolved via host → Host.user_id = ctx.user.id"),
        "scope comment should document the traversal:\n{out}"
    );
}

#[test]
fn scope_owner_without_matching_column_skips_silently() {
    // Resource has no owner-like column. Codegen must not invent a
    // binding; doctor surfaces the warning separately.
    let mut feature = base_feature("trust");
    let mut resource = simple_resource("Review");
    resource.fields.push(scope_field("status")); // unrelated field
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "update".to_owned(),
            atoms: vec!["@scope.owner".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("flag");
    cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Review"),
        assignments: Vec::new(),
    });
    cmd.policy = PolicyRef::Local("update".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        !out.contains("FromCtx(\"user.id\")"),
        "no matching column → no scope binding emitted:\n{out}"
    );
}
