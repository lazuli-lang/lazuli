//! LAZ-route-id-codegen-go (Cell A1) — `route id: ID` on a command MUST
//! produce an `ID` (after the acronym path) field on the emitted Go
//! input struct. Without it, the synth Updates / Deletes Effect's
//! `Bindings{"id": FromInput("ID")}` resolves against nothing and the
//! runtime returns 400 bad_request for every dispatch.
//!
//! Bug origin: hostpoint Phase L playwright sweep 2026-05-21 surfaced
//! the gap on `SaveTravelerTravelerVehicleInput` (dist/go/traveler/
//! command.gen.go:1023) and four sibling structs. The cycle proposal
//! `codegen-correctness-cycle-2026-05-21.md` §3.A1 mandates the field
//! emission + a fixture covering BOTH the route-present and
//! route-empty branches.

mod builders;

use builders::{
    base_command, command_gen, empty_feature, local_qname, module_with, resource_with,
};
use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{
    Assignment, BuiltinType, CommandEffect, CommandInput, CommandKind, CreateEffect, DeleteEffect,
    Expr, FieldConstraints, Module, Path, RouteSlot, RouteSlotKind, TypeRef, TypedSlot,
    UpdateEffect,
};

// ---------------------------------------------------------------------------
// The fixture proper.
// ---------------------------------------------------------------------------

/// Build the canonical fixture: one feature `traveler` with a resource
/// `TravelerVehicle` plus the two commands the bug surfaces against.
fn traveler_fixture() -> Module {
    let mut feature = empty_feature("traveler");
    feature.resources.push(resource_with(
        "TravelerVehicle",
        "vehicle",
        TypeRef::Builtin(BuiltinType::Text),
    ));

    // Positive branch: `command save_traveler_vehicle route id: ID
    //                    input vehicle: TravelerVehicle required
    //                    updates TravelerVehicle ...`
    let mut save = base_command("save_traveler_vehicle");
    save.kind = CommandKind::Update;
    save.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: RouteSlotKind::Plain,
    }];
    save.input = CommandInput::Typed(vec![TypedSlot {
        name: "vehicle".to_owned(),
        type_ref: TypeRef::UserDefined(local_qname("TravelerVehicle")),
        required: true,
        constraints: FieldConstraints::default(),
        validate_skip: false,
    }]);
    save.effect = CommandEffect::Updates(UpdateEffect {
        resource: local_qname("TravelerVehicle"),
        assignments: vec![Assignment {
            field: "vehicle".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "vehicle"])),
        }],
    });
    feature.commands.push(save);

    // Negative branch: `command create_traveler input name: Text required
    //                    creates TravelerVehicle ...` — no route slot,
    //                    must emit unchanged.
    let mut create = base_command("create_traveler");
    create.kind = CommandKind::Create;
    create.input = CommandInput::Typed(vec![TypedSlot {
        name: "name".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Text),
        required: true,
        constraints: FieldConstraints::default(),
        validate_skip: false,
    }]);
    create.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("TravelerVehicle"),
        from_input: false,
        assignments: vec![Assignment {
            field: "vehicle".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "name"])),
        }],
    });
    feature.commands.push(create);

    module_with(feature)
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

/// Positive branch — `route id: ID` lands on the emitted Input struct.
/// The WHERE binding (`FromInput("ID")`) and the struct field must be
/// in lockstep; without the field, the runtime resolves against `nil`.
///
/// Generated struct name follows `command_input_struct_name`:
/// `save_traveler_vehicle` on `Updates(TravelerVehicle)` becomes
/// `Save` + `TravelerVehicle` + `TravelerVehicle` + `Input` (the resource
/// pascal name is repeated because the modifier words spell out the
/// same identifier). The shape mirrors `SaveTravelerTravelerVehicleInput`
/// from the canonical hostpoint pilot bug.
#[test]
fn route_id_lands_on_input_struct_for_save_command() {
    let module = traveler_fixture();
    let files = generate_v1(&module, &GoEmitOptions::default());
    let out = command_gen(&files, "traveler");

    // Struct name (verb + resource_pascal + modifier_words + Input).
    assert!(
        out.contains("type SaveTravelerVehicleTravelerVehicleInput struct {"),
        "expected `SaveTravelerVehicleTravelerVehicleInput` struct in:\n{out}"
    );

    // `route id: ID` -> `ID lazuli.ID` field with the json + validate
    // tags. The body's `vehicle: TravelerVehicle required` collapses to
    // `lazuli.ID` because `TravelerVehicle` resolves to the local
    // resource type (resource refs in body positions FK-collapse to
    // `lazuli.ID`). So both fields use `lazuli.ID`, which means the
    // type-column width is 9 and the name-column width is 7 (Vehicle).
    assert!(
        out.contains("ID      lazuli.ID `json:\"id\" validate:\"required\"`"),
        "route id slot must surface as an aligned `ID lazuli.ID` field:\n{out}"
    );

    // The body slot survives alongside the route field (collapsed to
    // lazuli.ID because TravelerVehicle is a local resource).
    assert!(
        out.contains("Vehicle lazuli.ID `json:\"vehicle\" validate:\"required\"`"),
        "body Vehicle field must remain on the struct:\n{out}"
    );

    // Ordering invariant — route slots precede body slots so the URL
    // path stays at the top of the JSON envelope.
    let id_pos = out
        .find("ID      lazuli.ID")
        .expect("ID route field must be emitted");
    let vehicle_pos = out
        .find("Vehicle lazuli.ID")
        .expect("Vehicle body field must be emitted");
    assert!(
        id_pos < vehicle_pos,
        "route slots must precede body slots in the Input struct:\n{out}"
    );

    // The Effect's WHERE binding stays as-is (uses `FromInput("ID")`).
    // Without the field above, this binding 400s; with the field, the
    // runtime walks the input struct via reflect to find `ID`. Updates
    // effects emit the bindings literal inline as one line (no trailing
    // comma between the WHERE map and the SET map).
    assert!(
        out.contains("lazuli.Bindings{\"id\": lazuli.FromInput(\"ID\")},"),
        "Updates Effect must keep the `id -> FromInput(\"ID\")` WHERE binding:\n{out}"
    );

    // Command value pins the new Input type as its `I` generic.
    assert!(
        out.contains(
            "var saveTravelerVehicleTravelerVehicle = lazuli.Command[SaveTravelerVehicleTravelerVehicleInput, TravelerVehicle]{"
        ),
        "Command[I, O] must use the route-augmented Input as I:\n{out}"
    );
}

/// Negative branch — commands with zero route slots emit byte-identical
/// Input structs (no synthetic `ID` field, no leading route block).
/// Locks the idempotency guarantee in §3.A1: "cells declaring zero
/// route_params emit unchanged".
#[test]
fn create_traveler_without_route_emits_unchanged_input_struct() {
    let module = traveler_fixture();
    let files = generate_v1(&module, &GoEmitOptions::default());
    let out = command_gen(&files, "traveler");

    // The create struct must NOT carry a route-only `ID` field. Slice
    // out the struct body and grep it. Struct name follows the same
    // `verb + resource_pascal + modifier_words + Input` shape.
    let create_struct_start = out
        .find("type CreateTravelerVehicleTravelerInput struct {")
        .expect("expected create Input struct");
    let create_struct_end = create_struct_start
        + out[create_struct_start..]
            .find("\n}")
            .expect("create struct must close with }");
    let create_struct = &out[create_struct_start..create_struct_end];
    assert!(
        !create_struct.contains("ID "),
        "create command (no route) must NOT gain a synthetic ID field:\n{create_struct}"
    );
    assert!(
        create_struct.contains("Name string"),
        "create command body field `Name string` must be present:\n{create_struct}"
    );
}

/// `CommandInput::Empty` + `route id: ID` — even with no body, the
/// route slot must surface so the Effect's WHERE binding resolves.
/// This is the shape `delete_X route id: ID deletes X` lowers to.
#[test]
fn empty_input_with_route_still_emits_input_struct() {
    let mut feature = empty_feature("billing");
    feature.resources.push(resource_with(
        "Charge",
        "amount",
        TypeRef::Builtin(BuiltinType::Integer),
    ));

    let mut cmd = base_command("delete_charge");
    cmd.kind = CommandKind::Delete;
    cmd.route = vec![RouteSlot {
        name: "id".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Id),
        from: None,
        kind: RouteSlotKind::Plain,
    }];
    cmd.input = CommandInput::Empty;
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("Charge"),
    });
    feature.commands.push(cmd);

    let module = module_with(feature);
    let files = generate_v1(&module, &GoEmitOptions::default());
    let out = command_gen(&files, "billing");

    // A synthetic Input struct (carrying only the route slot) MUST be
    // emitted — pre-fix the emitter falls back to `struct{}` and the
    // runtime 400s on every dispatch.
    assert!(
        out.contains("type DeleteChargeChargeInput struct {"),
        "Empty input + route slot must emit a synthetic Input struct:\n{out}"
    );
    assert!(
        out.contains("ID lazuli.ID `json:\"id\" validate:\"required\"`"),
        "route id slot must surface as the only field on the synthetic struct:\n{out}"
    );
    assert!(
        out.contains("var deleteChargeCharge = lazuli.Command[DeleteChargeChargeInput, Charge]{"),
        "Command[I, O] must reference the synthetic struct, not struct{{}}:\n{out}"
    );
    // Deletes effects render the bindings literal across multiple
    // lines; the substring covers the `id` row regardless of indent.
    assert!(
        out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
        "Deletes Effect must keep its `id -> FromInput(\"ID\")` binding:\n{out}"
    );
}

/// Multi-slot route (`route customer_id: ID, tag_id: ID`) — every slot
/// becomes a struct field in source order, with snake_case JSON tags
/// and PascalCase Go field names (acronym path uppercases `id`).
#[test]
fn composite_route_emits_each_slot_as_struct_field() {
    let mut feature = empty_feature("customer_tags");
    feature.resources.push(resource_with(
        "CustomerTagAssignment",
        "label",
        TypeRef::Builtin(BuiltinType::Text),
    ));

    let mut cmd = base_command("remove_tag");
    cmd.kind = CommandKind::Delete;
    cmd.route = vec![
        RouteSlot {
            name: "customer_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: RouteSlotKind::Plain,
        },
        RouteSlot {
            name: "tag_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: RouteSlotKind::Plain,
        },
    ];
    cmd.input = CommandInput::Empty;
    cmd.effect = CommandEffect::Deletes(DeleteEffect {
        resource: local_qname("CustomerTagAssignment"),
    });
    feature.commands.push(cmd);

    let module = module_with(feature);
    let files = generate_v1(&module, &GoEmitOptions::default());
    let out = command_gen(&files, "customer_tags");

    assert!(
        out.contains("type RemoveCustomerTagAssignmentTagInput struct {"),
        "multi-route command must still emit an Input struct:\n{out}"
    );
    assert!(
        out.contains("CustomerID lazuli.ID `json:\"customer_id\" validate:\"required\"`"),
        "first route slot must land on the struct:\n{out}"
    );
    assert!(
        out.contains("TagID      lazuli.ID `json:\"tag_id\" validate:\"required\"`"),
        "second route slot must land on the struct:\n{out}"
    );
    // Order matches source declaration.
    let customer_pos = out.find("CustomerID lazuli.ID").unwrap();
    let tag_pos = out.find("TagID      lazuli.ID").unwrap();
    assert!(
        customer_pos < tag_pos,
        "composite route fields must follow source declaration order:\n{out}"
    );
}
