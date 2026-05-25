//! Cell E3 — file-level walker for `command.gen.go`. This module hosts
//! the `emit_command_file` entry point that the parent emitter calls;
//! per-command emission, format helpers, lifecycle / policy / scope /
//! semantic helpers all live in their own sibling submodules under
//! `command/`.
//!
//! The Rails-style split (wave R8-2) lifted this orchestrator out of
//! `command/mod.rs` to keep that file a thin module-root. ABI is held
//! constant via the `pub use file_emit::emit_command_file` re-export in
//! `mod.rs`.

use lazuli_ir::{Command, CommandInput, Feature};
// The inline test modules below address several IR types via `use
// super::*;`. Re-import the surface from `lazuli_ir` once so the tests
// pick it up without each block listing its own subset.
#[cfg(test)]
use lazuli_ir::{
    CommandEffect, CommandKind, Expr, NamedArg, Path, QualifiedName, RouteSlot, TypedSlot,
};

use super::super::cross_feature::CrossFeatureIndex;
use super::super::error_envelope::emit_wrap_helper;
use super::super::error_resolver::command_has_error_keys;
use super::super::imports::ImportSet;
use super::super::module::EmitContext;
use super::super::printer::GoPrinter;
use super::super::types::TypeCtx;

use super::emit::emit_command;
use super::format::register_imports_for_type;
use super::lifecycle::emit_lifecycle_machines;
use super::semantic::semantic_validator_plugins;
use super::wrap::command_wrap_buckets;

pub fn emit_command_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.commands.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    // Sort commands by name so iteration order is independent of how
    // the IR `Vec` happened to be populated.
    let mut commands: Vec<&Command> = feature.commands.iter().collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    let wrap_buckets = command_wrap_buckets(&commands);

    // Pre-walk to populate imports. Every command pulls in the
    // top-level `lazuli.dev/runtime/lazuli` package for at minimum
    // `Command[I,O]`, `Policy`, `AuditDefault`, `Creates/Updates/Deletes`,
    // `Bindings`, and `EventEmit`. Input field types may surface extra
    // imports (e.g. `storage.FileRef` for `@cap.File` slots).
    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/observability");
    if feature.resources.iter().any(|r| r.lifecycle.is_some()) {
        imports.add("lazuli.dev/runtime/lazuli/lifecycle");
    }
    if !wrap_buckets.is_empty() {
        imports.add("context");
        imports.add("errors");
        imports.add("lazuli.dev/runtime/lazuli/auth");
    }
    // PG.C.1 — gated commands import `billing` (CheckFeature/CheckQuota/
    // IncrQuota) and `plan` (the package-wide Catalog). Detected via
    // the gate-map lookup on the EmitContext.
    let any_gated = commands
        .iter()
        .any(|cmd| !emit_ctx.gates_for("command", &cmd.name).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }
    // IR Error-Vocab — per-command `ErrorKeys` literals reference the
    // typed `i18n.MessageRef` shape (proposal §5.1). Import the i18n
    // package whenever any command in this feature has an override.
    if commands
        .iter()
        .any(|c| command_has_error_keys(c, Some(&feature.policies)))
    {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }
    // Handlers live in the same Go package as the feature (see
    // `emitter/handlers.rs` module docs) — no extra package import
    // needed for `lazuli.Returns(<HandlerName>)` calls.
    let mut any_semantic_validators = false;
    for command in &commands {
        if let CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                register_imports_for_type(&slot.type_ref, &type_ctx, &mut imports);
            }
        }
        // Route slots (`route id: ID`, `route customer_id: ID`) are
        // folded into the emitted `<Cmd>Input` struct, so their type
        // refs also need import registration.
        for slot in &command.route {
            register_imports_for_type(&slot.type_ref, &type_ctx, &mut imports);
        }
        if let lazuli_ir::CommandEffect::Returns(ret) = &command.effect {
            register_imports_for_type(&ret.return_type, &type_ctx, &mut imports);
        }
        // Resource references (Creates/Updates/Deletes) live in the
        // same Go package — no cross-feature import needed beyond what
        // the resource emitter already registers in `resource.gen.go`.

        // LAZ-SEMANTIC-AUTO-VALIDATE — every input field that resolves to
        // a SemanticPluginType carrying a non-empty validator name pulls
        // in the plugin's Go package so the pre-handler validation pass
        // can call `<alias>.<Validator>(...)`.
        for plugin in semantic_validator_plugins(command) {
            imports.add_aliased(&plugin.alias, &plugin.import_path);
            any_semantic_validators = true;
        }
    }
    if any_semantic_validators {
        imports.add("lazuli.dev/runtime/lazuli");
    }

    p.banner(
        source_label,
        &super::super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();
    if !wrap_buckets.is_empty() {
        emit_wrap_helper(&mut p, &wrap_buckets);
        p.blank();
    }
    if emit_lifecycle_machines(&mut p, feature) {
        p.blank();
    }

    let mut first_block = true;
    for command in &commands {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_command(&mut p, feature, command, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

#[cfg(test)]
mod feature_emit_tests {
    use super::super::test_support::{
        base_command, base_feature, local_qname, module_with_features, simple_resource, typed_slot,
    };
    use super::*;
    use lazuli_ir::{Assignment, BuiltinType, CreateEffect};
    use lazuli_ir::{CommandEffect, CommandInput, Expr, Path};

    #[test]
    fn representative_feature_emits_command_file_shape() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource("Customer"));

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
        feature.commands.push(cmd);

        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/command.gen.go");
        let out = emit_command_file(
            "features/customer/customer.lzi",
            &feature,
            "lazuli/test",
            &index,
            &emit_ctx,
        )
        .expect("representative command feature must emit command.gen.go");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("type CreateCustomerInput struct {"));
        assert!(
            out.contains("var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{")
        );
        assert!(out.contains(
            "func HandleCreate(ctx *lazuli.Ctx, input CreateCustomerInput) (Customer, error) {"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit, local_qname,
        module_with_features, simple_resource, typed_slot,
    };
    use super::*;
    use lazuli_ir::{
        Assignment, BackoffStrategy, BuiltinType, CreateEffect, DeleteEffect,
        DeprecationReplacement, EnumLiteral, EnvName, IdempotencyKey, InvalidatesSpec, LetBinding,
        PolicyExpr, PolicyRef, RateLimitByEnv, RateLimitSpec, Record, RetryPolicy, ReturnsEffect,
        Tenancy, TypeRef, UpdateEffect,
    };

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn empty_input_creates_command_skips_input_struct_and_uses_struct_unit() {
        // `delete` style command: no input slots, Creates effect on
        // Customer. The Command value still names a type pair, so we
        // surface `struct{}` for the I parameter.
        let mut feature = base_feature("customer");
        let mut cmd = base_command("archive");
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        // No Input struct on the empty-input branch.
        assert!(!out.contains("ArchiveCustomerInput"));
        assert!(out.contains("var archiveCustomer = lazuli.Command[struct{}, Customer]{"));
    }

    #[test]
    fn typed_input_creates_command_emits_input_struct_and_creates_bindings() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("create");
        cmd.input = CommandInput::Typed(vec![
            typed_slot("name", BuiltinType::Text, true),
            typed_slot("email", BuiltinType::SemanticEmail, true),
        ]);
        cmd.policy = PolicyRef::Atom("@role.admin".to_owned());
        cmd.rate_limit = Some(lazuli_ir::RateLimitSpec::from_default(
            "30 per hour per ip".to_owned(),
        ));
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
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
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // Input struct shape.
        assert!(out.contains("type CreateCustomerInput struct {"));
        // L0 #3 §10 (D.3) — required slots now stack a `validate:"required"`
        // alongside the json tag; both pieces ship in the same backtick block.
        assert!(out.contains("json:\"name\" validate:\"required\""));
        assert!(out.contains("json:\"email\" validate:\"required\""));
        assert!(out.contains("Email lazuli.Email"));
        // Command value shape.
        assert!(
            out.contains("var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{")
        );
        assert!(out.contains("Name:      \"customer.create\","));
        assert!(out.contains("Resource:  &customerResource,"));
        assert!(out.contains("lazuli.PolicyAtom{{Namespace: \"role\", Name: \"admin\"}}"));
        assert!(out.contains("RateLimit: lazuli.RateLimit{Default: \"30 per hour per ip\"},"));
        // Effect block — Creates with two FromInput bindings.
        assert!(out.contains("Effect: lazuli.Creates(&customerResource, lazuli.Bindings{"));
        assert!(out.contains("\"name\": lazuli.FromInput(\"name\"),"));
        assert!(out.contains("\"email\": lazuli.FromInput(\"email\"),"));
    }

    #[test]
    fn returns_command_emits_returns_effect_and_handler_signature_comment() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("summary");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Builtin(BuiltinType::Text),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // Handlers live in the same Go package — no `handlers` import
        // and no qualifier on the handler value.
        assert!(!out.contains("\"lazuli/test/customer/handlers\""));
        assert!(out.contains("var summaryResult = lazuli.Command[SummaryResultInput, string]{"));
        assert!(out.contains(
            "Effect: lazuli.ReturnsFromRegistry[SummaryResultInput, string](\"customer.summary\"),"
        ));
        assert!(!out.contains("Effect: lazuli.Returns(handlers.Summary),"));
        assert!(out.contains(
            "// Wire Summary as `func(ctx *lazuli.Ctx, input SummaryResultInput) (string, error)`"
        ));
        assert!(out.contains(
            "// then register with `lazuli.RegisterFn(\"customer.summary\", Summary)` at init()."
        ));
        assert!(!out.contains("TODO(returns):"));
    }

    #[test]
    fn no_effect_command_emits_nil_effect_without_legacy_todo() {
        let mut feature = base_feature("customer");
        feature.commands.push(base_command("summary"));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("Effect: nil,"));
        assert!(out.contains(
            "// No-effect commands are pure-read legacy APIs invoked via command.Invoke."
        ));
        assert!(!out.contains("TODO(effect):"));
    }

    #[test]
    fn cap_file_synthesised_no_effect_command_wires_returns_from_registry() {
        use lazuli_ir::{AutoPhotoCommandRole, BuiltinType, CommandInput, SynthesizedFromCapFile};
        let mut feature = base_feature("customer");
        let mut cmd = base_command("confirm_profile_photo_upload");
        cmd.input = CommandInput::Typed(vec![typed_slot("key", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::None;
        cmd.handler = None;
        cmd.synthesized_from_cap_file = Some(SynthesizedFromCapFile {
            resource: "Customer".to_owned(),
            field: "profile_photo".to_owned(),
            role: AutoPhotoCommandRole::Confirm,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // The runtime fn lives in auto_photo.gen.go under "customer.confirm_profile_photo_upload".
        // Effect must forward to that registry key with struct{} return — without
        // the wire-up the runtime 500s with "command has no effect".
        assert!(
            out.contains(
                "Effect: lazuli.ReturnsFromRegistry[ConfirmResultProfilePhotoUploadInput, struct{}](\"customer.confirm_profile_photo_upload\"),"
            ),
            "missing ReturnsFromRegistry wire-up for cap_file confirm:\n{out}"
        );
        assert!(!out.contains("Effect: nil,"));
        assert!(out.contains(
            "// cap_file synth: handler registered by auto_photo.gen.go under \"customer.confirm_profile_photo_upload\"."
        ));
    }

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
            out.contains(
                "\"tier\": lazuli.FromConst(\"new_tier\") /* let new_tier = input.tier */,"
            )
        );
    }

    #[test]
    fn updates_emits_updates_effect_with_id_where_clause() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("update_tier");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![typed_slot("tier", BuiltinType::Text, true)]);
        cmd.policy = PolicyRef::Local("update".to_owned());
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Customer"),
            assignments: vec![Assignment {
                field: "tier".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "tier"])),
            }],
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains(
            "var updateCustomerTier = lazuli.Command[UpdateCustomerTierInput, Customer]{"
        ));
        assert!(out.contains("Effect: lazuli.Updates(&customerResource,"));
        assert!(out.contains("lazuli.Bindings{\"id\": lazuli.FromInput(\"ID\")},"));
        assert!(out.contains("\"tier\": lazuli.FromInput(\"tier\"),"));
        // LAZ-route-id-codegen-go (Cell A1) — the route slot MUST land
        // on the emitted Input struct so the `FromInput("ID")` binding
        // above resolves at runtime. Route fields come first, body
        // fields after. `pascal_case("id")` hits the acronym path so
        // the Go field name is `ID`, not `Id`; the route type ref
        // `BuiltinType::Id` lowers to `lazuli.ID` via go_type_for.
        assert!(
            out.contains("type UpdateCustomerTierInput struct {"),
            "Input struct must be emitted for route + body commands:\n{out}"
        );
        assert!(
            out.contains("ID   lazuli.ID `json:\"id\" validate:\"required\"`"),
            "route id slot must surface as `ID lazuli.ID` aligned field:\n{out}"
        );
        assert!(
            out.contains("Tier string    `json:\"tier\" validate:\"required\"`"),
            "body Tier field must remain after the route Id field:\n{out}"
        );
        // Ordering invariant — route slots precede body slots.
        let id_pos = out
            .find("ID   lazuli.ID")
            .expect("ID field must be emitted");
        let tier_pos = out.find("Tier string").expect("Tier field must be emitted");
        assert!(
            id_pos < tier_pos,
            "route slots must precede body slots in the Input struct:\n{out}"
        );
        // Local policy renders as `@policy.<name>`. The exact padding
        // depends on which other kv rows landed; assertion targets the
        // payload so renaming the column doesn't break the test.
        assert!(out.contains("lazuli.Policy{Name: \"@policy.update\"},"));
    }


    #[test]
    fn deletes_emits_deletes_effect_with_id_binding() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("archive");
        cmd.kind = CommandKind::Delete;
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("Customer"),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("var archiveCustomer = lazuli.Command[ArchiveCustomerInput, Customer]{")
        );
        assert!(out.contains("Effect: lazuli.Deletes(&customerResource, lazuli.Bindings{"));
        assert!(out.contains("\"id\": lazuli.FromInput(\"ID\"),"));
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
    fn tier4_fields_emit_runtime_struct_fields() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("reassign");
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Customer"),
            assignments: Vec::new(),
        });
        cmd.approval = Some(lazuli_ir::ApprovalSpec {
            required_when: Some("target.tier = enterprise".to_owned()),
            by: "@role.admin".to_owned(),
            timeout: Some("24h".to_owned()),
            then: lazuli_ir::ApprovalThen::Deny,
        });
        cmd.external_calls = vec![lazuli_ir::ExternalCallRef {
            slot: "audit".to_owned(),
            op: "log".to_owned(),
            args: Vec::new(),
            span_ref: None,
        }];
        cmd.timeout = Some("30s".to_owned());
        cmd.retry = Some(RetryPolicy {
            count: 3,
            backoff: BackoffStrategy::Exponential,
        });
        cmd.idempotency = Some(IdempotencyKey {
            by: Path::from_segments(["input", "external_id"]),
        });
        cmd.deprecated = Some(lazuli_ir::Deprecation {
            since: Some("2026.04".to_owned()),
            replacement: Some(DeprecationReplacement::LocalCommand(
                "reassign_v2".to_owned(),
            )),
            sunset: Some("2026-12-31".to_owned()),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains(
            "Approval: &lazuli.ApprovalSpec{Then: \"deny\", By: \"@role.admin\", Reason: \"target.tier = enterprise\"},"
        ));
        assert!(out.contains("ExternalCalls: []lazuli.ExternalCallRef{"));
        assert!(out.contains("{Slot: \"audit\", Operation: \"log\"},"));
        assert!(out.contains("Timeout: \"30s\","));
        assert!(out.contains("Retry: &lazuli.RetryPolicy{Count: 3, Backoff: \"exponential\"},"));
        assert!(out.contains("Idempotency: &lazuli.IdempotencyKey{Path: \"input.external_id\"},"));
        assert!(out.contains(
            "Deprecation: &lazuli.Deprecation{Since: \"2026.04\", Replacement: \"customer.command.reassign_v2\", Sunset: \"2026-12-31\"},"
        ));
        assert!(!out.contains("TODO("));
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
    fn tier4_fields_omit_absent_slots() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("create");
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("Approval:"));
        assert!(!out.contains("ExternalCalls:"));
        assert!(!out.contains("Timeout:"));
        assert!(!out.contains("Retry:"));
        assert!(!out.contains("Idempotency:"));
        assert!(!out.contains("Deprecation:"));
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

    #[test]
    fn cross_feature_input_type_emits_lazuli_id() {
        // Command in `customer` takes a `User` (declared in `org`) as
        // an input slot. The input collapses to `lazuli.ID` — JSON
        // bodies carry FK ids, never the embedded resource row — so
        // the cross-feature import is dropped along with the struct
        // ref. Records would still emit `orggen.<Name>` + import.
        let mut customer = base_feature("customer");
        customer.resources.push(simple_resource("Customer"));
        let mut cmd = base_command("reassign");
        cmd.input = CommandInput::Typed(vec![TypedSlot {
            name: "owner".to_owned(),
            type_ref: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "User".to_owned(),
            }),
            required: true,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Customer"),
            assignments: Vec::new(),
        });
        customer.commands.push(cmd);

        let mut org = base_feature("org");
        org.resources.push(simple_resource("User"));

        let module = module_with_features(vec![customer.clone(), org]);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/command.gen.go");
        let out = emit_command_file(
            "examples/x.lzi",
            &customer,
            "lazuli/test",
            &index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("Owner lazuli.ID"),
            "expected `Owner lazuli.ID` for cross-feature resource input, got:\n{out}"
        );
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource("Customer"));
        let mut zebra = base_command("zebra");
        zebra.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        let mut alpha = base_command("alpha");
        alpha.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(zebra);
        feature.commands.push(alpha);

        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/command.gen.go");

        let a = emit_command_file("examples/x.lzi", &feature, "lazuli/test", &index, &emit_ctx)
            .expect("must emit");
        let b = emit_command_file("examples/x.lzi", &feature, "lazuli/test", &index, &emit_ctx)
            .expect("must emit");
        assert_eq!(a, b);

        // Alphabetical order: alpha banner before zebra banner.
        let alpha_pos = a.find("Command: customer.alpha").expect("alpha banner");
        let zebra_pos = a.find("Command: customer.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn gate_prelude_injects_feature_and_quota_checks_into_handler_wrapper() {
        // PG.C.1 — a `command create` carrying `gate behind ...` +
        // `gate quota ...` directives should surface in the generated
        // handler wrapper as billing.CheckFeature / billing.CheckQuota
        // short-circuits followed by a post-success billing.IncrQuota.
        let mut feature = base_feature("billing");
        feature.resources.push(simple_resource("Invoice"));
        let mut cmd = base_command("create");
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Invoice"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        // Synthesise a gate map keyed off `<feature>/command:<name>`.
        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/command:create".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "create_invoice".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "invoices_per_month".to_owned(),
                },
            ],
        );
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/command.gen.go")
                .with_gates(Some(&gates));

        let out = emit_command_file(
            "examples/billing.lzi",
            &module.features[0],
            "lazuli/test",
            &index,
            &emit_ctx,
        )
        .expect("must emit");

        // Imports must include billing + plan packages.
        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"lazuli/test/plan\""),
            "plan import missing:\n{out}"
        );
        // Behind-gate prelude line.
        assert!(
            out.contains("billing.CheckFeature(ctx, plan.Catalog, \"create_invoice\")"),
            "CheckFeature call missing:\n{out}"
        );
        // Quota-gate prelude line.
        assert!(
            out.contains("billing.CheckQuota(ctx, plan.Catalog, \"invoices_per_month\")"),
            "CheckQuota call missing:\n{out}"
        );
        // Post-success increment.
        assert!(
            out.contains("billing.IncrQuota(ctx, plan.Catalog, \"invoices_per_month\")"),
            "IncrQuota call missing:\n{out}"
        );
        // Ordering: feature-check fires before quota-check, both before
        // the .Handle() call site.
        let feat_pos = out
            .find("billing.CheckFeature(ctx, plan.Catalog, \"create_invoice\")")
            .expect("feature-check site");
        let quota_pos = out
            .find("billing.CheckQuota(ctx, plan.Catalog, \"invoices_per_month\")")
            .expect("quota-check site");
        let handle_pos = out
            .find("createInvoice.Handle(ctx, input)")
            .expect("handler site");
        assert!(
            feat_pos < quota_pos,
            "feature-check must run before quota-check"
        );
        assert!(
            quota_pos < handle_pos,
            "quota-check must run before .Handle()"
        );
    }

    #[test]
    fn no_gates_means_no_billing_imports_or_prelude_lines() {
        // PG.C.1 backward compat — commands without gates emit the
        // legacy wrapper byte-for-byte (no billing / plan imports,
        // no Check* lines, no Incr* lines).
        let mut feature = base_feature("customer");
        let mut cmd = base_command("create");
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("billing.CheckFeature"),
            "no CheckFeature when no gates"
        );
        assert!(
            !out.contains("billing.CheckQuota"),
            "no CheckQuota when no gates"
        );
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates"
        );
    }

    // The Record import is dragged in for typed-record output binding
    // ergonomics in later cells; keep a smoke-fn so the `Record` import
    // doesn't bit-rot when its emission branch lands.
    #[allow(dead_code)]
    fn _record_compiles(_: Record) {}
    #[allow(dead_code)]
    fn _tenancy_compiles(_: Tenancy) {}

    // ------------------------------------------------------------------
    // RB.S6.C — `policy_expr` rendering.
    // ------------------------------------------------------------------

    #[test]
    fn policy_expr_authenticated_renders_predicate_atom() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("create");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::Authenticated);
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("Name: \"authenticated\""),
            "expected `Name: \"authenticated\"` literal in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"),
            "expected predicate atom in:\n{out}"
        );
    }

    #[test]
    fn policy_expr_has_permission_renders_rbac_permission_atom() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("start");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::HasPermission("queries:start".to_owned()));
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"rbac.permission\", Name: \"queries:start\"}"),
            "expected rbac.permission atom in:\n{out}"
        );
        assert!(
            out.contains("Name: \"has_permission queries:start\""),
            "expected display name in:\n{out}"
        );
    }

    #[test]
    fn policy_expr_and_combinator_renders_paren_and_predicate_atoms() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("start");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::And(vec![
            PolicyExpr::Authenticated,
            PolicyExpr::HasRole("manager".to_owned()),
        ]));
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"),
            "missing authenticated atom in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"and\"}"),
            "missing and atom in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"rbac.role\", Name: \"manager\"}"),
            "missing rbac.role atom in:\n{out}"
        );
        assert!(
            out.contains("Name: \"authenticated and has_role manager\""),
            "missing combined display name in:\n{out}"
        );
    }


    // `ir-rate-limit-env-aware` Cell 2 — codegen emission tests.

    #[test]
    fn rate_limit_default_only_emits_compact_struct_literal() {
        // Backward-compat: legacy single-line `rate_limit "X"` source
        // lowers to `RateLimitSpec { default: "X", by_env: [] }` and
        // emits the compact one-liner struct literal so existing
        // fixtures are byte-stable.
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
        cmd.rate_limit = Some(RateLimitSpec::from_default(
            "5 per 10 minutes per ip".to_owned(),
        ));
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("RateLimit: lazuli.RateLimit{Default: \"5 per 10 minutes per ip\"},"),
            "compact single-line shape expected for default-only spec, got:\n{out}"
        );
        // No multi-line `ByEnv` block in the compact shape.
        assert!(
            !out.contains("ByEnv: []lazuli.RateLimitByEnv"),
            "unexpected ByEnv block in default-only emission:\n{out}"
        );
    }

    #[test]
    fn rate_limit_with_by_env_emits_multi_line_struct_literal() {
        // The 22+ playwright trigger case: production strict + dev /
        // staging / test loose. Emission must list every env-qualified
        // entry verbatim (RULE-VOCAB-04 read-through).
        let mut feature = base_feature("customer");
        let mut cmd = base_command("register");
        cmd.input = CommandInput::Typed(vec![typed_slot("email", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: vec![Assignment {
                field: "email".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "email"])),
            }],
        });
        cmd.rate_limit = Some(RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: vec![RateLimitByEnv {
                envs: vec![EnvName::Dev, EnvName::Staging, EnvName::Test],
                unknown_envs: Vec::new(),
                limit: "1000 per 10 minutes per ip".to_owned(),
                span_ref: None,
            }],
            span_ref: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // The opening of the struct literal sits on the kv-aligned line.
        assert!(
            out.contains("RateLimit: lazuli.RateLimit{"),
            "multi-line struct opening missing, got:\n{out}"
        );
        // Default + ByEnv block emitted with absolute indentation so
        // gofmt accepts the file without rewriting.
        assert!(
            out.contains("\t\tDefault: \"5 per 10 minutes per ip\","),
            "Default row not at expected indent, got:\n{out}"
        );
        assert!(
            out.contains("\t\tByEnv: []lazuli.RateLimitByEnv{"),
            "ByEnv slice declaration missing, got:\n{out}"
        );
        assert!(
            out.contains(
                "{Envs: []string{\"dev\", \"staging\", \"test\"}, Limit: \"1000 per 10 minutes per ip\"},"
            ),
            "by-env entry not emitted verbatim, got:\n{out}"
        );
    }

    #[test]
    fn rate_limit_with_unlimited_keyword_emits_empty_string_limit() {
        // The `"unlimited"` keyword lowers to an empty `limit` string
        // (proposal §4.4). Codegen pastes the empty string verbatim;
        // the runtime's `IsUnlimited()` reads the empty as "no throttle".
        let mut feature = base_feature("customer");
        let mut cmd = base_command("register");
        cmd.input = CommandInput::Typed(vec![typed_slot("email", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: vec![Assignment {
                field: "email".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "email"])),
            }],
        });
        cmd.rate_limit = Some(RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: vec![RateLimitByEnv {
                envs: vec![EnvName::Test],
                unknown_envs: Vec::new(),
                limit: String::new(), // "unlimited" lowered.
                span_ref: None,
            }],
            span_ref: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("{Envs: []string{\"test\"}, Limit: \"\"},"),
            "unlimited (empty string) by-env entry missing, got:\n{out}"
        );
    }
}
