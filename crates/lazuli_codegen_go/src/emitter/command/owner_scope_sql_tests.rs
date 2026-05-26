//! `Command.owner_scope_sql` projection tests — cell
//! `codegen-os-projection`. The analyzer composes `OwnerScopeSql` per
//! `ir-resource-conventions-owner-scope.md` §7.3; this codegen cell
//! pastes the carrier through `FromCtxOwnedVia` (DELETE/UPDATE) and
//! `CreatesWithOwnerCheck` (CREATE) so the emitted SQL matches §8.1 /
//! §8.5.A verbatim after the existing tenant predicates. Lifted out of
//! `scope.rs` (wave R8-2c) so the parent file stays under the
//! ≤500-LOC gold standard.
//!
//! Coverage cluster:
//!   - DELETE with `owner_scope_sql` → FromCtxOwnedVia binding (§8.1)
//!   - DELETE without `owner_scope_sql` → tenant-only shape unchanged
//!   - UPDATE with `owner_scope_sql` → FromCtxOwnedVia + optional SET
//!   - UPDATE partial-write: required → `FromInput`, optional → `FromInputOptional`
//!   - CREATE partial-write mirror (same axis on the INSERT side)
//!   - CREATE with `cte_owner_check` → `CreatesWithOwnerCheck` + `OwnerCheckSpec`
//!   - CREATE without `cte_owner_check` → regular `lazuli.Creates(...)`
//!   - PascalCase `fk_target` snake-cased into emitted `FromCtxOwnedVia`
//!
//! Companion files: `scope_owner_tests.rs` (atom-driven injection),
//! `scope_where_keys_tests.rs` (route/input-key resolution).

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, scope_field,
    simple_resource, typed_slot,
};
use lazuli_ir::{
    Assignment, BuiltinType, CommandEffect, CommandInput, CommandKind, CreateEffect, DeleteEffect,
    Expr, Path, RouteSlot, TypeRef, UpdateEffect,
};

fn owner_scope_sql_property() -> lazuli_ir::OwnerScopeSql {
    // Mirrors the analyzer's cell-O2 output for Hostpoint's
    // `Property.host: Host required @owner_axis(through: user)`.
    lazuli_ir::OwnerScopeSql {
        field_name: "host".to_owned(),
        fk_target: "Host".to_owned(),
        through_column: "user".to_owned(),
        where_predicate: "host IN (SELECT id FROM \"host\" WHERE \"user\" = ctx.User.ID)"
            .to_owned(),
        cte_owner_check: None,
    }
}

#[test]
fn delete_with_owner_scope_sql_emits_owned_via_binding() {
    // Spec §8.1: synth `delete_property` lowers to
    // `DELETE FROM "property" WHERE id = $1 AND org_id = $2 AND
    //   host IN (SELECT id FROM "host" WHERE "user" = $3)`.
    // Codegen projection: existing `id` binding from route +
    // tenant via baseScopeConditions + FromCtxOwnedVia for the
    // ownership chain. We assert the emitted Go contains the
    // owned-via binding row in the Deletes effect's Where map.
    let mut feature = base_feature("catalog");
    let mut resource = simple_resource("Property");
    resource.fields.push(scope_field("host"));
    feature.resources.push(resource);

    let mut cmd = base_command("delete_property");
    cmd.kind = CommandKind::Delete;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("Property"),
    });
    cmd.owner_scope_sql = Some(owner_scope_sql_property());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
        "DELETE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
    );
    assert!(
        out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "existing route-key id binding must remain:\n{out}"
    );
    assert!(
        out.contains("// scope: @owner_axis resolved via host"),
        "scope-binding comment must document the owner-axis traversal:\n{out}"
    );
}

#[test]
fn delete_without_owner_scope_sql_emits_unchanged_tenant_only_shape() {
    // Resources without `@owner_axis` carry `owner_scope_sql: None`.
    // The emitted Go must be identical to today's tenant-only DELETE
    // shape — no FromCtxOwnedVia binding leaks into the Where map.
    let mut feature = base_feature("billing");
    feature.resources.push(simple_resource("Charge"));

    let mut cmd = base_command("delete_charge");
    cmd.kind = CommandKind::Delete;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("Charge"),
    });
    cmd.owner_scope_sql = None;
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        !out.contains("FromCtxOwnedVia"),
        "DELETE without owner_scope_sql must NOT emit owned-via:\n{out}"
    );
    assert!(
        !out.contains("@owner_axis"),
        "no owner-axis annotation should appear in emitted code when carrier is None:\n{out}"
    );
    assert!(
        out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "baseline route-key binding must be present:\n{out}"
    );
}

#[test]
fn update_with_owner_scope_sql_emits_owned_via_binding() {
    // Spec §8.2: synth `update_property` lowers to
    // `UPDATE "property" SET ... WHERE id = $1 AND org_id = $4 AND
    //   host IN (SELECT id FROM "host" WHERE "user" = $5)`.
    let mut feature = base_feature("catalog");
    let mut resource = simple_resource("Property");
    resource.fields.push(scope_field("host"));
    resource.fields.push(scope_field("name"));
    feature.resources.push(resource);

    let mut cmd = base_command("update_property");
    cmd.kind = CommandKind::Update;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, false)]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Property"),
        assignments: vec![Assignment {
            field: "name".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "name"])),
        }],
    });
    cmd.owner_scope_sql = Some(owner_scope_sql_property());
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
        "UPDATE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
    );
    // SET-side binding: `name` is an optional input slot (above) so
    // the emitter now picks `FromInputOptional` so the runtime
    // skips the column when the wire payload omits it (partial-
    // update semantics). Required slots keep emitting plain
    // `FromInput`.
    assert!(
        out.contains("\"name\": lazuli.FromInputOptional(\"name\"),"),
        "SET-side optional input must emit FromInputOptional:\n{out}"
    );
}

/// Partial-write axis: an UPDATE command whose typed input mixes
/// required + optional slots must emit `FromInput` for the
/// required ones and `FromInputOptional` for the optional ones, so
/// the runtime keeps the existing column value when the wire
/// payload omits an optional field. Regression for the hostpoint
/// 2026-05-22 settings-save outage.
#[test]
fn update_emits_from_input_optional_for_optional_input_slots() {
    let mut feature = base_feature("widget");
    let mut resource = simple_resource("Widget");
    resource.fields.push(scope_field("name"));
    resource.fields.push(scope_field("color"));
    feature.resources.push(resource);

    let mut cmd = base_command("update_widget");
    cmd.kind = CommandKind::Update;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.input = CommandInput::Typed(vec![
        typed_slot("name", BuiltinType::Text, true),   // required
        typed_slot("color", BuiltinType::Text, false), // optional
    ]);
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Widget"),
        assignments: vec![
            Assignment {
                field: "name".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "name"])),
            },
            Assignment {
                field: "color".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "color"])),
            },
        ],
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("\"name\": lazuli.FromInput(\"name\"),"),
        "required input slot must emit plain FromInput:\n{out}"
    );
    assert!(
        out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
        "optional input slot must emit FromInputOptional:\n{out}"
    );
}

/// Mirror of the above for CREATE — required slots stay
/// `FromInput`, optional slots become `FromInputOptional` so the
/// INSERT skips columns whose wire field was nil and lets the
/// column default take effect.
#[test]
fn create_emits_from_input_optional_for_optional_input_slots() {
    let mut feature = base_feature("widget");
    let mut resource = simple_resource("Widget");
    resource.fields.push(scope_field("name"));
    resource.fields.push(scope_field("color"));
    feature.resources.push(resource);

    let mut cmd = base_command("create_widget");
    cmd.kind = CommandKind::Create;
    cmd.input = CommandInput::Typed(vec![
        typed_slot("name", BuiltinType::Text, true),
        typed_slot("color", BuiltinType::Text, false),
    ]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Widget"),
        from_input: false,
        assignments: vec![
            Assignment {
                field: "name".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "name"])),
            },
            Assignment {
                field: "color".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "color"])),
            },
        ],
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("\"name\": lazuli.FromInput(\"name\"),"),
        "required input slot must emit plain FromInput:\n{out}"
    );
    assert!(
        out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
        "optional input slot must emit FromInputOptional:\n{out}"
    );
}

#[test]
fn create_with_cte_owner_check_emits_creates_with_owner_check() {
    // Spec §8.5.A: synth `create_property` lowers to
    //   WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $<fk>
    //     AND "user" = ctx.User.ID)
    //   INSERT INTO "property" (...) SELECT ... FROM owner_check
    //   RETURNING ...
    // Codegen projection: switch from `lazuli.Creates(...)` to
    // `lazuli.CreatesWithOwnerCheck(..., OwnerCheckSpec{...})`. The
    // runtime composes the CTE prefix from the spec fields; codegen
    // only emits the carrier.
    let mut feature = base_feature("catalog");
    let mut resource = simple_resource("Property");
    resource.fields.push(scope_field("host"));
    resource.fields.push(scope_field("name"));
    feature.resources.push(resource);

    let mut cmd = base_command("create_property");
    cmd.kind = CommandKind::Create;
    cmd.input = CommandInput::Typed(vec![
        typed_slot("host", BuiltinType::Id, true),
        typed_slot("name", BuiltinType::Text, true),
    ]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Property"),
        from_input: false,
        assignments: vec![
            Assignment {
                field: "host".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "host"])),
            },
            Assignment {
                field: "name".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "name"])),
            },
        ],
    });
    let mut scope = owner_scope_sql_property();
    scope.cte_owner_check = Some(
        "WITH owner_check AS (SELECT 1 FROM \"host\" WHERE id = $host AND \"user\" = ctx.User.ID)"
            .to_owned(),
    );
    cmd.owner_scope_sql = Some(scope);
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains(
            "Effect: lazuli.CreatesWithOwnerCheck(&propertyResource, lazuli.Bindings{"
        ),
        "CREATE with cte_owner_check should emit CreatesWithOwnerCheck:\n{out}"
    );
    assert!(
        out.contains("lazuli.OwnerCheckSpec{"),
        "OwnerCheckSpec literal must be emitted:\n{out}"
    );
    assert!(
        out.contains("FKColumn:      \"host\","),
        "OwnerCheckSpec.FKColumn must point at the FK field:\n{out}"
    );
    assert!(
        out.contains("RelatedTable:  \"host\","),
        "OwnerCheckSpec.RelatedTable must be the snake-cased FK target:\n{out}"
    );
    assert!(
        out.contains("ThroughColumn: \"user\","),
        "OwnerCheckSpec.ThroughColumn must match the @owner_axis through: value:\n{out}"
    );
    assert!(
        !out.contains("Effect: lazuli.Creates(&propertyResource"),
        "tenant-only Creates form should NOT appear when CTE is active:\n{out}"
    );
}

#[test]
fn create_without_cte_owner_check_emits_regular_creates() {
    // When `owner_scope_sql.cte_owner_check` is None (or the slot
    // itself is None), the CREATE emit falls back to the tenant-only
    // `lazuli.Creates(...)` shape — no CTE wrapper.
    let mut feature = base_feature("billing");
    feature.resources.push(simple_resource("Charge"));

    let mut cmd = base_command("create_charge");
    cmd.input = CommandInput::Typed(vec![typed_slot("amount", BuiltinType::Integer, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Charge"),
        from_input: false,
        assignments: vec![Assignment {
            field: "amount".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "amount"])),
        }],
    });
    cmd.owner_scope_sql = None;
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("Effect: lazuli.Creates(&chargeResource, lazuli.Bindings{"),
        "CREATE without cte_owner_check must use the regular Creates form:\n{out}"
    );
    assert!(
        !out.contains("CreatesWithOwnerCheck"),
        "tenant-only CREATE must NOT use CreatesWithOwnerCheck:\n{out}"
    );
    assert!(
        !out.contains("OwnerCheckSpec"),
        "tenant-only CREATE must NOT emit OwnerCheckSpec:\n{out}"
    );
}

#[test]
fn owner_scope_sql_snake_cases_pascal_fk_target() {
    // The analyzer's `OwnerScopeSql.fk_target` carries PascalCase
    // (`"Host"`, `"BookingProposal"`), matching the IR's resource
    // name shape. Codegen lowers to snake_case when projecting to
    // `FromCtxOwnedVia` so the runtime's `quoteIdent` round-trips
    // with the migrated SQL table name (`booking_proposal`).
    let mut feature = base_feature("operations");
    let mut resource = simple_resource("Transaction");
    resource.fields.push(scope_field("proposal"));
    feature.resources.push(resource);

    let mut cmd = base_command("cancel_transaction");
    cmd.kind = CommandKind::Update;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: lazuli_ir::RouteSlotKind::Plain,
    }];
    cmd.input = CommandInput::Empty;
    cmd.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("Transaction"),
        assignments: Vec::new(),
    });
    cmd.owner_scope_sql = Some(lazuli_ir::OwnerScopeSql {
        field_name: "proposal".to_owned(),
        fk_target: "BookingProposal".to_owned(),
        through_column: "user".to_owned(),
        where_predicate:
            "proposal IN (SELECT id FROM \"booking_proposal\" WHERE \"user\" = ctx.User.ID)"
                .to_owned(),
        cte_owner_check: None,
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains(
            "\"proposal\": lazuli.FromCtxOwnedVia(\"booking_proposal\", \"user\", \"user.id\"),"
        ),
        "PascalCase fk_target must be snake-cased in the emitted FromCtxOwnedVia:\n{out}"
    );
}
