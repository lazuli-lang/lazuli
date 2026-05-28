//! GAP-AUDIT-01 — round-trip the `AuditSpec.materialize` slot
//! (`AuditMaterialize`) through serde so the wire shape stays stable
//! across analyzer / codegen / doctor consumers, and confirm the
//! additive `skip_serializing_if = "Option::is_none"` keeps pre-GAP-AUDIT-01
//! snapshots byte-for-byte stable.

use lazuli_ir::{AuditMaterialize, AuditSpec};

fn base_audit() -> AuditSpec {
    AuditSpec {
        subjects: vec!["actor".to_owned(), "target.id".to_owned()],
        emit_to: Some("@event_group.job_audit".to_owned()),
        data_subject: None,
        record_before: true,
        record_after: true,
        retain_for: None,
        materialize: None,
    }
}

#[test]
fn audit_materialize_round_trips_through_serde() {
    let mut spec = base_audit();
    spec.materialize = Some(AuditMaterialize {
        feature: "operation_audit_log".to_owned(),
        resource: "OperationLog".to_owned(),
    });
    let json = serde_json::to_string(&spec).expect("serialize AuditSpec");
    assert!(
        json.contains("\"feature\":\"operation_audit_log\""),
        "serialized payload must carry the materialize feature verbatim: {json}",
    );
    assert!(
        json.contains("\"resource\":\"OperationLog\""),
        "serialized payload must carry the materialize resource verbatim: {json}",
    );
    let back: AuditSpec = serde_json::from_str(&json).expect("deserialize AuditSpec");
    assert_eq!(spec, back);
    let back_m = back.materialize.expect("materialize survives round-trip");
    assert_eq!(back_m.feature, "operation_audit_log");
    assert_eq!(back_m.resource, "OperationLog");
}

#[test]
fn audit_materialize_omitted_when_none() {
    // Additive: a pre-GAP-AUDIT-01 audit spec must not surface the
    // `materialize` slot in the JSON shape, and an old snapshot without
    // it deserializes with `None`.
    let spec = base_audit();
    let json = serde_json::to_string(&spec).expect("serialize AuditSpec");
    assert!(
        !json.contains("materialize"),
        "absent materialize must skip serialization: {json}",
    );
    let back: AuditSpec = serde_json::from_str(&json).expect("deserialize AuditSpec");
    assert_eq!(spec, back);
    assert!(back.materialize.is_none());
}
