//! Cell E3 command-kind emit tests — a feature carrying a Command
//! with a typed input + Creates effect surfaces an extra
//! `<feature>/command.gen.go` with the typed `<Verb><Resource>Input`
//! struct plus the `lazuli.Command[I, O]` value.

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{
    Assignment, BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Expr,
    Field, FieldConstraints, Path, PolicyRef, QualifiedName, RateLimitSpec, Resource, TypeRef,
    TypedSlot,
};

use super::builders::minimal_module;

#[test]
fn command_kind_emits_typed_input_struct_and_command_value() {
    // Cell E3 integration smoke: a feature carrying one Command with
    // a typed input + Creates effect surfaces an extra
    // `<feature>/command.gen.go` alongside the resource file. The
    // shape mirrors `dist/go/customer/customer.gen.go:48-68` (proven
    // by the runtime spike). Per proposal §3.2 we land the typed
    // `<Verb><Resource>Input` struct plus the
    // `lazuli.Command[I, O]` value with `Effect: lazuli.Creates(...)`.
    let mut module = minimal_module("marketplace", "customer");

    // Resource the command targets — same feature.
    module.features[0].resources.push(Resource {
        name: "Customer".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![
            Field {
                name: "name".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Text),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            },
            Field {
                name: "email".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            },
        ],
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
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    });

    // Command — `customer.create` with typed input + Creates effect.
    module.features[0].commands.push(Command {
        name: "create".to_owned(),
        public_contract: None,
        kind: CommandKind::Create,
        route: Vec::new(),
        input: CommandInput::Typed(vec![
            TypedSlot {
                name: "name".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Text),
                required: true,
                constraints: FieldConstraints::default(),
                validate_skip: false,
            },
            TypedSlot {
                name: "email".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
                required: true,
                constraints: FieldConstraints::default(),
                validate_skip: false,
            },
        ]),
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::Creates(CreateEffect {
            resource: QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            },
            from_input: false,
            assignments: vec![
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
                Assignment {
                    field: "email".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "email"])),
                },
            ],
        }),
        policy: PolicyRef::Atom("@role.admin".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: vec!["customer_created".to_owned()],
        rate_limit: Some(RateLimitSpec::from_default("30 per hour per ip".to_owned())),
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
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
        derived_from: None,
    });

    let files = generate_v1(&module, &GoEmitOptions::default());
    let command_file = files
        .iter()
        .find(|f| f.path == "customer/command.gen.go")
        .expect("expected customer/command.gen.go alongside the resource file");

    // Banner + package directive.
    assert!(
        command_file
            .contents
            .contains("// Code generated by lazuli; DO NOT EDIT."),
        "missing banner in command.gen.go:\n{}",
        command_file.contents
    );
    assert!(
        command_file.contents.contains("\npackage customergen\n"),
        "expected `package customergen` in command.gen.go:\n{}",
        command_file.contents
    );

    // Input struct shape — typed slots with JSON tags.
    assert!(
        command_file
            .contents
            .contains("type CreateCustomerInput struct {"),
        "expected `type CreateCustomerInput struct` in command.gen.go:\n{}",
        command_file.contents
    );
    assert!(
        command_file.contents.contains("Email lazuli.Email"),
        "expected `Email lazuli.Email` field row in CreateCustomerInput:\n{}",
        command_file.contents
    );

    // Command value shape — generic over input/output, with Creates effect.
    assert!(
        command_file
            .contents
            .contains("var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{"),
        "expected `var createCustomer = lazuli.Command[CreateCustomerInput, Customer]` in command.gen.go:\n{}",
        command_file.contents
    );
    assert!(
        command_file
            .contents
            .contains("Effect: lazuli.Creates(&customerResource, lazuli.Bindings{"),
        "expected `Effect: lazuli.Creates(...)` block in command.gen.go:\n{}",
        command_file.contents
    );
    assert!(
        command_file
            .contents
            .contains("\"email\": lazuli.FromInput(\"email\"),"),
        "expected `email` binding row in command.gen.go:\n{}",
        command_file.contents
    );
    assert!(
        command_file
            .contents
            .contains("{Name: \"customer_created\", From: lazuli.FromExplicit},"),
        "expected emits row in command.gen.go:\n{}",
        command_file.contents
    );
    // Policy.Atom rendering — `@role.admin` splits into namespace/name.
    assert!(
        command_file
            .contents
            .contains("lazuli.PolicyAtom{{Namespace: \"role\", Name: \"admin\"}}"),
        "expected `PolicyAtom{{Namespace: \"role\", Name: \"admin\"}}` in command.gen.go:\n{}",
        command_file.contents
    );
}

#[test]
fn reorder_command_emits_batch_reorder_effect() {
    // W4 GAP-REORDER-01 — `reorder JobStep by position` emits a
    // `lazuli.Reorder(&jobStepResource, "position")` effect (wire-thin).
    let mut module = minimal_module("ops", "jobs");
    module.features[0].resources.push(Resource {
        name: "JobStep".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "position".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Integer),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }],
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
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    });

    let mut cmd = Command {
        name: "reorder_steps".to_owned(),
        public_contract: None,
        kind: CommandKind::Reorder,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: lazuli_ir::CommandEffect::Reorders(lazuli_ir::ReorderEffect {
            resource: QualifiedName {
                feature: None,
                name: "JobStep".to_owned(),
            },
            position_field: "position".to_owned(),
        }),
        policy: PolicyRef::Atom("@role.admin".to_owned()),
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
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
        derived_from: None,
    };
    cmd.kind = CommandKind::Reorder;
    module.features[0].commands.push(cmd);

    let files = generate_v1(&module, &GoEmitOptions::default());
    let command_file = files
        .iter()
        .find(|f| f.path == "jobs/command.gen.go")
        .expect("expected jobs/command.gen.go");
    assert!(
        command_file
            .contents
            .contains("Effect: lazuli.Reorder(&jobStepResource, \"position\"),"),
        "expected `Effect: lazuli.Reorder(...)` in command.gen.go:\n{}",
        command_file.contents
    );
}

#[test]
fn command_kind_skips_file_when_feature_declares_no_commands() {
    // Mirrors the resource / enum skip rule — a feature without
    // commands should not materialise `command.gen.go`.
    let module = minimal_module("marketplace", "customer");
    let files = generate_v1(&module, &GoEmitOptions::default());
    assert!(
        files.iter().all(|f| f.path != "customer/command.gen.go"),
        "expected no command.gen.go for an empty feature, got files: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}
