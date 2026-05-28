//! GAP-AUDIT-01 — `audit ... materialize @feature.<f>.<R>` codegen
//! golden tests. Exercise `emit_command`'s `Audit:` kv-row through
//! `emit_command_file`:
//!
//!   - audit WITHOUT materialize → the legacy `lazuli.AuditDefault`
//!     marker (byte-stable with pre-GAP-AUDIT-01 fixtures).
//!   - audit WITH materialize → a populated `&lazuli.AuditSpec{...}`
//!     carrying the snake-cased target OperationLog table so the runtime
//!     does the second INSERT (one record, two sinks).

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, typed_slot,
};
use lazuli_ir::{
    Assignment, AuditMaterialize, AuditSpec, BuiltinType, CommandEffect, CommandInput, CreateEffect,
    Expr, Path,
};

fn create_customer_cmd(name: &str) -> lazuli_ir::Command {
    let mut cmd = base_command(name);
    cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "name".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "name"])),
        }],
    });
    cmd
}

#[test]
fn audit_without_materialize_emits_audit_default_marker() {
    let mut feature = base_feature("customer");
    let mut cmd = create_customer_cmd("create");
    cmd.audit = Some(AuditSpec {
        subjects: vec!["actor".to_owned(), "target.id".to_owned()],
        emit_to: None,
        data_subject: None,
        record_before: false,
        record_after: false,
        retain_for: None,
        materialize: None,
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    // The kv block pads keys for alignment, so match the value tail
    // independent of the inter-column whitespace.
    assert!(
        out.contains("Audit:") && out.contains("lazuli.AuditDefault,"),
        "audit without materialize must emit the AuditDefault marker, got:\n{out}"
    );
    assert!(
        !out.contains("MaterializeTable"),
        "no MaterializeTable expected without a materialize clause:\n{out}"
    );
}

#[test]
fn audit_with_materialize_emits_operation_log_table() {
    let mut feature = base_feature("customer");
    let mut cmd = create_customer_cmd("create");
    cmd.audit = Some(AuditSpec {
        subjects: vec!["actor".to_owned(), "target.id".to_owned()],
        emit_to: Some("@event_group.job_audit".to_owned()),
        data_subject: None,
        record_before: true,
        record_after: true,
        retain_for: None,
        materialize: Some(AuditMaterialize {
            feature: "operation_audit_log".to_owned(),
            // PascalCase resource → snake_case table in the emitted INSERT.
            resource: "OperationLog".to_owned(),
        }),
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    // The kv block pads the `Audit:` key for alignment; assert the value
    // tail (the populated AuditSpec with the snake-cased table).
    assert!(
        out.contains("&lazuli.AuditSpec{MaterializeTable: \"operation_log\"},"),
        "materialize must emit a populated AuditSpec with the snake-cased \
         OperationLog table, got:\n{out}"
    );
    // The legacy default marker must NOT appear for a materialize command.
    assert!(
        !out.contains("lazuli.AuditDefault,"),
        "materialize command must not fall back to AuditDefault:\n{out}"
    );
}
