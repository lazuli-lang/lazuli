//! `ir-resource-conventions-owner-scope` Cell O1 — round-trip the
//! `Field.owner_axis` slot through serde so the wire shape stays stable
//! across analyzer / codegen consumers.
//!
//! Previously lived inline as `mod owner_axis_tests` in
//! `lazuli_ir/src/lib.rs`; promoted to an integration test by Wave R3-F
//! Stage 3 to shrink the lib root toward the ≤ 1000 LOC gold-standard
//! target.

use lazuli_ir::{BuiltinType, Field, FieldConstraints, OwnerAxis, TypeRef};

#[test]
fn owner_axis_round_trips_through_serde() {
    let axis = OwnerAxis {
        through_column: "user".to_owned(),
    };
    let json = serde_json::to_string(&axis).expect("serialize OwnerAxis");
    assert!(
        json.contains("\"through_column\":\"user\""),
        "serialized payload must carry the through_column verbatim: {json}",
    );
    let back: OwnerAxis = serde_json::from_str(&json).expect("deserialize OwnerAxis");
    assert_eq!(axis, back);
}

#[test]
fn field_owner_axis_omitted_when_none() {
    // §7.2 — `skip_serializing_if = "Option::is_none"` keeps pre-O1
    // IR snapshots byte-for-byte stable. A field with no axis must
    // not surface the slot in the JSON shape.
    let field = Field {
        name: "name".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Text),
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
    };
    let json = serde_json::to_string(&field).expect("serialize Field");
    assert!(
        !json.contains("owner_axis"),
        "absent owner_axis must skip serialization: {json}",
    );
    let back: Field = serde_json::from_str(&json).expect("deserialize Field");
    assert_eq!(field, back);
}
