//! Regression guard for the `route id: ID` -> TS input interface
//! contract.
//!
//! Cell A2 of the codegen-correctness-cycle-2026-05-21
//! (LAZ-route-id-codegen-ts).
//!
//! Context: the sibling cell A1 fixes `lazuli_codegen_go` so the
//! generated Go input struct includes an `Id` field for a command
//! that declares `route id: ID`. The TS mirror is driven by
//! `RuntimeCommand.inputs` (per `lazuli_codegen_spec::RuntimeCommand`
//! docstring: "Includes route keys (e.g. `id`) and `input` block
//! fields. Order matches the DSL.").
//!
//! The TS emitter at `crates/lazuli_codegen_ts/src/runtime.rs` walks
//! `command.inputs` in order and special-cases `field_name == "ID"`
//! to emit `id: ID;` (camelCase + ID type). This test locks both
//! halves of that contract from regression:
//!
//! 1. When the runtime spec carries an `ID` slot at index 0 (the
//!    shape A1's analyzer lowering produces for `route id: ID`), the
//!    emitted TS interface has `id: ID;` as the FIRST field.
//! 2. Subsequent input slots are emitted AFTER the `id`, in spec
//!    order, with camelCase keys.
//!
//! If a future refactor changes the spec contract (e.g. moves route
//! params into a separate `route_params` field on `RuntimeCommand`),
//! this test fails fast, signalling that the TS emitter needs an
//! explicit route-loop matching what the CLI's IR-driven emitter
//! (`emit_feature_sdk_ts` -> `command_sdk_slots`) already does.

use lazuli_codegen_spec::{
    FieldKind, RuntimeCommand, RuntimeEffect, RuntimeFeature, RuntimeField, RuntimeInput,
    RuntimeResource, Tenancy,
};
use lazuli_codegen_ts::emit_feature_ts;

fn save_traveler_vehicle_feature() -> RuntimeFeature {
    RuntimeFeature {
        name: "traveler".to_owned(),
        source_path: "features/traveler/traveler.lzi".to_owned(),
        resources: vec![RuntimeResource {
            name: "traveler".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![RuntimeField {
                name: "name".to_owned(),
                kind: FieldKind::Text,
            }],
        }],
        commands: vec![RuntimeCommand {
            short_name: "save_vehicle".to_owned(),
            policy_name: "@policy.update".to_owned(),
            policy_atoms: Vec::new(),
            rate_limit: String::new(),
            validators: Vec::new(),
            effect: RuntimeEffect::UpdatesByID,
            // Spec-construction lowers `route id: ID input vehicle:
            // Text required` into a single ordered `inputs` slice
            // where the route slot comes first. A1's Go fix relies
            // on this exact shape; the TS emitter consumes the same
            // slice and emits `id` then `vehicle`.
            inputs: vec![
                RuntimeInput {
                    field_name: "ID".to_owned(),
                    kind: FieldKind::Integer,
                },
                RuntimeInput {
                    field_name: "Vehicle".to_owned(),
                    kind: FieldKind::Text,
                },
            ],
            emits: Vec::new(),
            invalidates: Vec::new(),
            deprecated: None,
        }],
        queries: Vec::new(),
    }
}

/// Strip whitespace-only lines and trim each line. Used for
/// order-of-field assertions where indentation noise would obscure
/// intent.
fn iface_body(emitted: &str, iface_name: &str) -> String {
    let header = format!("export interface {iface_name} {{");
    let start = emitted
        .find(&header)
        .unwrap_or_else(|| panic!("missing interface header `{iface_name}` in:\n{emitted}"));
    let after_header = &emitted[start + header.len()..];
    let end = after_header
        .find('}')
        .expect("interface block must close with `}`");
    after_header[..end].to_owned()
}

#[test]
#[allow(non_snake_case)]
fn route_id_emits_first_field_as_id_ID() {
    let feature = save_traveler_vehicle_feature();
    let out = emit_feature_ts(&feature);

    assert!(
        out.contains("export interface SaveTravelerVehicleInput {"),
        "expected `SaveTravelerVehicleInput` interface; got:\n{out}"
    );

    let body = iface_body(&out, "SaveTravelerVehicleInput");

    // Both fields present.
    assert!(
        body.contains("id: ID;"),
        "missing `id: ID;` field — route_id was dropped from TS input. body:\n{body}"
    );
    assert!(
        body.contains("vehicle: string;"),
        "missing `vehicle: string;` field; body:\n{body}"
    );

    // Order: `id` must precede `vehicle` (regression guard against
    // re-ordering, sorting, or moving route params after inputs).
    let id_pos = body
        .find("id: ID;")
        .expect("id field must exist (asserted above)");
    let vehicle_pos = body
        .find("vehicle: string;")
        .expect("vehicle field must exist (asserted above)");
    assert!(
        id_pos < vehicle_pos,
        "expected `id: ID;` BEFORE `vehicle: string;` in input interface; got body:\n{body}"
    );
}

#[test]
#[allow(non_snake_case)]
fn route_id_input_uses_camel_case_id_not_ID_key() {
    // `RuntimeInput { field_name: "ID" }` is the spec-construction
    // form; on the TS side it MUST render as the lower-case `id` key
    // (per `case-mapper.ts` boundary contract). This guards the
    // special-case at `runtime.rs:149-153` from being deleted.
    let feature = save_traveler_vehicle_feature();
    let out = emit_feature_ts(&feature);
    let body = iface_body(&out, "SaveTravelerVehicleInput");

    assert!(
        !body.contains("ID: ID;"),
        "the input KEY must be camelCase `id`, not pascal `ID`; body:\n{body}"
    );
    assert!(
        body.contains("id: ID;"),
        "expected camelCase `id: ID;`; body:\n{body}"
    );
}
