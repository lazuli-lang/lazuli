//! `resolve_where_keys` tests — the production fn lives in `scope.rs`
//! (priority 1 = `command.route` slots → priority 2 = single typed
//! input slot → priority 3 = legacy `id` fallback). This file owns the
//! observable shapes that the resolver produces end-to-end through
//! `emit_command_file`, plus the `@scope.self` ctx-as-key suppression
//! and the bulk-mode no-key projection. Lifted out of `scope.rs`
//! (wave R8-2c) so the parent file stays under the ≤500-LOC gold
//! standard.
//!
//! Coverage cluster:
//!   - alt-key WHERE (single typed input slot that is NOT `id`)
//!   - route slot as WHERE key (single)
//!   - `@scope.self` ctx-as-key (suppresses route/input id binding)
//!   - bulk-mode delete (no route + no input → no `id` binding)
//!   - composite multi-route WHERE
//!   - `Returns` of user-defined resource → full struct generic
//!     (not the `lazuli.ID` FK collapse)
//!
//! Companion files: `scope_owner_tests.rs` (atom-driven injection),
//! `owner_scope_sql_tests.rs` (analyzer-supplied projection).

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, scope_field,
    simple_resource, typed_slot,
};
use lazuli_ir::{
    BuiltinType, CommandEffect, CommandInput, DeleteEffect, Policies, PolicyRef, ReturnsEffect,
    TypeRef, UpdateEffect,
};

// -------------------------------------------------------------------------
// Alt-key WHERE binding (Wave 8). When a delete/update command has no
// `route` and a single typed input slot whose name is NOT `id`, the
// codegen now uses that slot as the WHERE key (column + Go input
// field). Closes the the canonical pilot Phase 4 codegen gap surfaced 2026-05-17.
// -------------------------------------------------------------------------

#[test]
fn deletes_with_single_input_slot_uses_alt_key_when_not_id() {
    let mut feature = base_feature("messaging");
    let mut resource = simple_resource("WebPushSubscription");
    resource.fields.push(scope_field("endpoint"));
    resource.fields.push(scope_field("user"));
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
    let mut cmd = base_command("unregister_web_push");
    cmd.input = CommandInput::Typed(vec![typed_slot("endpoint", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("WebPushSubscription"),
    });
    cmd.policy = PolicyRef::Local("delete".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"endpoint\": lazuli.FromInput(\"Endpoint\"),"),
        "single-slot input `endpoint` should drive WHERE:\n{out}"
    );
    assert!(
        !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "no `id` binding should leak when input slot is `endpoint`:\n{out}"
    );
    assert!(
        out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
        "@scope.owner should still inject the ownership column:\n{out}"
    );
}

#[test]
fn updates_with_route_slot_uses_route_as_where_key() {
    let mut feature = base_feature("trust");
    let mut resource = simple_resource("Review");
    resource.fields.push(scope_field("status"));
    feature.resources.push(resource);
    let mut cmd = base_command("flag");
    cmd.route = vec![lazuli_ir::RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.input = CommandInput::Typed(vec![typed_slot("reason", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Review"),
        assignments: Vec::new(),
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    // Route drives the WHERE key. `reason` is the body slot, not a
    // WHERE key candidate.
    assert!(
        out.contains("\"id\": lazuli.FromInput(\"ID\")"),
        "route id should drive WHERE:\n{out}"
    );
    assert!(
        !out.contains("\"reason\": lazuli.FromInput(\"Reason\"),"),
        "non-route, non-key input should not leak into WHERE bindings:\n{out}"
    );
    // LAZ-route-id-codegen-go (Cell A1) — the route id slot must
    // ALSO be present on the Input struct so the FromInput("ID")
    // binding above resolves at dispatch.
    assert!(
        out.contains("ID     lazuli.ID `json:\"id\" validate:\"required\"`"),
        "route id slot must land on the Input struct as `ID lazuli.ID`:\n{out}"
    );
    assert!(
        out.contains("Reason string    `json:\"reason\" validate:\"required\"`"),
        "body Reason field must still be present:\n{out}"
    );
}

// -------------------------------------------------------------------------
// @scope.self — ctx-as-key WHERE binding (Wave 9 / the canonical pilot codegen gap G).
// Closes `account.choose_role` UPDATE WHERE id = ctx.user.id.
// -------------------------------------------------------------------------

#[test]
fn updates_with_scope_self_uses_ctx_user_id_as_where_key() {
    let mut feature = base_feature("account");
    let mut resource = simple_resource("User");
    resource.fields.push(scope_field("role"));
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "choose_role".to_owned(),
            atoms: vec!["@scope.self".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("choose_role");
    cmd.input = CommandInput::Typed(vec![typed_slot("role", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("User"),
        assignments: Vec::new(),
    });
    cmd.policy = PolicyRef::Local("choose_role".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    // @scope.self drives WHERE via ctx; the `role` input slot is
    // a body field, not a key.
    assert!(
        out.contains("\"id\": lazuli.FromCtx(\"user.id\"),"),
        "@scope.self should bind id from ctx.user.id:\n{out}"
    );
    assert!(
        !out.contains("\"id\": lazuli.FromInput(\""),
        "@scope.self must suppress the route/input id binding (no double-id):\n{out}"
    );
    assert!(
        out.contains("// scope: @scope.self resolved → id = ctx.user.id"),
        "scope comment should document the ctx-key pattern:\n{out}"
    );
}

// -------------------------------------------------------------------------
// Bulk delete — @scope.owner with no route AND no typed input
// (Wave 9 / the canonical pilot codegen gap H). Closes `account.logout` etc.
// -------------------------------------------------------------------------

#[test]
fn deletes_in_bulk_mode_drops_legacy_id_binding() {
    let mut feature = base_feature("account");
    let mut resource = simple_resource("UserSession");
    resource.fields.push(scope_field("user_id"));
    feature.resources.push(resource);
    feature.policies = Policies {
        categories: vec![lazuli_ir::PolicyCategory {
            name: "logout".to_owned(),
            atoms: vec!["@scope.owner".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        }],
        fields: Vec::new(),
        span_ref: None,
    };
    let mut cmd = base_command("logout");
    cmd.input = CommandInput::Empty;
    // No route slots either.
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("UserSession"),
    });
    cmd.policy = PolicyRef::Local("logout".to_owned());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "bulk delete must NOT emit legacy id-from-input binding:\n{out}"
    );
    assert!(
        out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
        "scope.owner should still inject the ownership binding:\n{out}"
    );
    assert!(
        out.contains("// bulk: no id/route key"),
        "bulk-mode comment should be visible for reviewers:\n{out}"
    );
}

#[test]
fn deletes_with_multi_route_emits_composite_where() {
    let mut feature = base_feature("customer_tags");
    let resource = simple_resource("CustomerTagAssignment");
    feature.resources.push(resource.clone());
    let mut cmd = base_command("remove_tag");
    cmd.route = vec![
        lazuli_ir::RouteSlot {
            name: "customer_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        },
        lazuli_ir::RouteSlot {
            name: "tag_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        },
    ];
    cmd.input = CommandInput::Empty;
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("CustomerTagAssignment"),
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("emits");
    assert!(
        out.contains("\"customer_id\": lazuli.FromInput(\"CustomerID\"),"),
        "first route slot should bind (note `id` acronym uppercases per is_acronym):\n{out}"
    );
    assert!(
        out.contains("\"tag_id\": lazuli.FromInput(\"TagID\"),"),
        "second route slot should bind:\n{out}"
    );
    // LAZ-route-id-codegen-go (Cell A1) — Empty-input + route slots
    // must STILL emit a synthetic Input struct carrying the route
    // fields. Without it, FromInput("CustomerID") / FromInput("TagID")
    // would resolve against `struct{}` and return 400 bad_request.
    assert!(
        out.contains("type RemoveCustomerTagAssignmentTagInput struct {"),
        "Empty input + route slots must still emit an Input struct:\n{out}"
    );
    assert!(
        out.contains("CustomerID lazuli.ID `json:\"customer_id\" validate:\"required\"`"),
        "first composite-route slot must surface on the Input struct:\n{out}"
    );
    assert!(
        out.contains("TagID      lazuli.ID `json:\"tag_id\" validate:\"required\"`"),
        "second composite-route slot must surface on the Input struct:\n{out}"
    );
}

/// `command me returns User` — the IR lowers to
/// `CommandEffect::Returns(ReturnsEffect { return_type: UserDefined("User") })`.
/// The emitted Output generic must be the full resource struct
/// (`Customer` same-feature, `<owner>gen.Customer` cross-feature),
/// NOT the `lazuli.ID` FK collapse used for resource-field positions.
/// Closes the `account.me` 500-internal at dispatch — the runtime's
/// `ReturnsFromRegistry[I, O]` type-asserts the registered fn as
/// `func(*Ctx, I) (O, error)`; with `O = lazuli.ID` and the
/// registered handler returning `(User, error)`, the assertion
/// fails and the runtime emits a 500 internal.
#[test]
fn returns_user_defined_resource_emits_full_struct_not_id() {
    let mut feature = base_feature("customer");
    let mut cmd = base_command("me");
    cmd.input = CommandInput::Empty;
    cmd.effect = CommandEffect::Returns(ReturnsEffect {
        return_type: TypeRef::UserDefined(local_qname("Customer")),
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    // Output generic in the Command[I, O] declaration is the full
    // struct (`Customer`) — NOT `lazuli.ID`. `command_var_name`
    // composes `meCustomer` from `verb=me, resource=Customer`.
    assert!(
        out.contains("var meCustomer = lazuli.Command[struct{}, Customer]{"),
        "Command[I, O] should pin O to the resource struct, got:\n{out}"
    );
    // Effect's ReturnsFromRegistry generic pins the same struct.
    assert!(
        out.contains("Effect: lazuli.ReturnsFromRegistry[struct{}, Customer]("),
        "ReturnsFromRegistry should pin O to Customer (not lazuli.ID), got:\n{out}"
    );
    assert!(
        !out.contains("ReturnsFromRegistry[struct{}, lazuli.ID]"),
        "regression: ReturnsFromRegistry must NOT collapse Customer to lazuli.ID:\n{out}"
    );
    // Handler comment matches the registered fn shape — the
    // emitted Wire comment names `Customer` as the return type.
    assert!(
        out.contains("(Customer, error)"),
        "handler signature comment should return Customer, got:\n{out}"
    );
}
