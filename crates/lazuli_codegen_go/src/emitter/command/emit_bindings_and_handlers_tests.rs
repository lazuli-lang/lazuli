//! Binding-source projection + handler-wrapper tests — exercise the
//! `emit_command` sub-pipeline that lowers individual `Assignment.value`
//! expressions (bare identifiers, enum literals) and the
//! `emit_command_handler_wrapper` sentinel mapping. Also covers the
//! `emits` / `invalidates` slot rendering. Lifted out of `emit.rs`
//! (wave R8-2c) so the parent file stays under the ≤500-LOC gold
//! standard.
//!
//! Coverage cluster:
//!   - Bare-identifier `Expr::Path(["new_tier"])` traces the source
//!     command-let in an inline comment alongside `FromConst(...)`.
//!   - `EnumLiteral` in a `Creates.assignments` slot renders as
//!     `FromConst(<TypePascal><VariantPascal>)`.
//!   - `Command.emits` lowers to `[]lazuli.EventEmit{...}` with
//!     `lazuli.FromExplicit` defaults.
//!   - `Command.invalidates` lowers to a `[]string{...}` flat list,
//!     same-feature shorthand resolves to the host feature.
//!   - `external_calls = [auth.verify_password]` wires the known
//!     `ErrPasswordMismatch` sentinel into the handler wrapper.
//!
//! Companion file: `emit_effect_dispatch_tests.rs` covers the per-effect
//! dispatch axis (Creates/Updates/Deletes/Returns/None).

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, typed_slot,
};
use lazuli_ir::{
    Assignment, BuiltinType, CommandEffect, CommandInput, CreateEffect, EnumLiteral, Expr,
    InvalidatesSpec, LetBinding, NamedArg, Path, QualifiedName,
};

#[test]
fn bare_identifier_binding_source_traces_command_let() {
    let mut feature = base_feature("customer");
    let mut cmd = base_command("create");
    cmd.input = CommandInput::Typed(vec![typed_slot("tier", BuiltinType::Text, true)]);
    cmd.lets = vec![LetBinding {
        name: "new_tier".to_owned(),
        value: Expr::Path(Path::from_segments(["input", "tier"])),
    }];
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "tier".to_owned(),
            value: Expr::Path(Path::from_segments(["new_tier"])),
        }],
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("\"tier\": lazuli.FromConst(\"new_tier\") /* let new_tier = input.tier */,")
    );
}

#[test]
fn emits_render_with_from_explicit_default() {
    let mut feature = base_feature("customer");
    let mut cmd = base_command("create");
    cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "name".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "name"])),
        }],
    });
    cmd.emits = vec!["customer_created".to_owned()];
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("Emits: []lazuli.EventEmit{"));
    assert!(out.contains("{Name: \"customer_created\", From: lazuli.FromExplicit},"));
}

#[test]
fn invalidates_render_as_string_list() {
    let mut feature = base_feature("customer");
    let mut cmd = base_command("create");
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: Vec::new(),
    });
    cmd.invalidates = vec![
        InvalidatesSpec {
            query: QualifiedName {
                feature: None,
                name: "list".to_owned(),
            },
            args: Vec::new(),
        },
        InvalidatesSpec {
            query: QualifiedName {
                feature: Some("billing".to_owned()),
                name: "ledger".to_owned(),
            },
            args: vec![NamedArg {
                name: "id".to_owned(),
                value: Expr::Path(Path::from_segments(["route", "id"])),
            }],
        },
    ];
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    // Cell B1: `.query.` infix dropped. Same-feature `query.list`
    // shorthand resolves to host feature `customer`.
    assert!(out.contains("Invalidates: []string{\"customer.list\", \"billing.ledger\"},"));
}

#[test]
fn emit_handler_wraps_known_sentinel() {
    let mut feature = base_feature("account");
    let mut cmd = base_command("login");
    cmd.effect = CommandEffect::None;
    cmd.external_calls = vec![lazuli_ir::ExternalCallRef {
        slot: "auth".to_owned(),
        op: "verify_password".to_owned(),
        args: Vec::new(),
        span_ref: None,
    }];
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("//lazuli:pattern command_pgx_insert v1"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/observability\""));
    assert!(out.contains("ctx.Context, endOp = observability.StartOp(ctx.Context)"));
    assert!(out.contains("\"context\""));
    assert!(out.contains("\"errors\""));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/auth\""));
    assert!(out.contains("func wrapErrorForHandler(ctx context.Context, err error) error"));
    assert!(out.contains("errors.Is(err, auth.ErrPasswordMismatch)"));
    assert!(out.contains("return &lazuli.FieldError{"));
    assert!(out.contains("Reason:    lazuli.FieldReasonMismatch,"));
}

#[test]
fn enum_literal_in_assignment_renders_qualified_from_const() {
    let mut feature = base_feature("customer");
    let mut cmd = base_command("create");
    cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "tier".to_owned(),
            value: Expr::Enum(EnumLiteral {
                type_name: Some(QualifiedName {
                    feature: None,
                    name: "CustomerTier".to_owned(),
                }),
                variant: "free".to_owned(),
            }),
        }],
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("\"tier\": lazuli.FromConst(CustomerTierFree),"));
}
