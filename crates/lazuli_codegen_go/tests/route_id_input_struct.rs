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

use std::collections::BTreeMap;

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{
    AppManifest, Assignment, BuiltinType, Command, CommandEffect, CommandInput, CommandKind,
    CreateEffect, Defaults, DeleteEffect, Expr, Feature, Field, FieldConstraints, Module, Path,
    Policies, PolicyRef, QualifiedName, Resource, RouteSlot, RouteSlotKind, TypeRef, TypedSlot,
    UpdateEffect,
};

// ---------------------------------------------------------------------------
// Boilerplate IR builders (kept here to keep this file self-contained).
// Mirrors the helpers in tests/emit_v1.rs.
// ---------------------------------------------------------------------------

fn empty_feature(name: &str) -> Feature {
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
        synth_origins: BTreeMap::new(),
    }
}

fn minimal_app_manifest(name: &str) -> AppManifest {
    AppManifest {
        name: name.to_owned(),
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

fn module_with(feature: Feature) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app_manifest("test_app")),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}

fn local_qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

fn simple_field(name: &str, type_ref: TypeRef) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        span_ref: None,
    }
}

fn resource_with(name: &str, field_name: &str, field_ty: TypeRef) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![simple_field(field_name, field_ty)],
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
    }
}

fn base_command(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        public_contract: None,
        kind: CommandKind::Create,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        triggers: vec![],
        synthesized_from_cap_file: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    }
}

fn command_gen(files: &[lazuli_codegen_go::GeneratedFile], feature: &str) -> String {
    files
        .iter()
        .find(|file| file.path == format!("{feature}/command.gen.go"))
        .unwrap_or_else(|| panic!("expected {feature}/command.gen.go"))
        .contents
        .clone()
}

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
