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
// The inline test modules below address a few IR types via `use super::*;`.
// Re-import the surface from `lazuli_ir` once so the tests pick it up.
#[cfg(test)]
use lazuli_ir::{CommandEffect, QualifiedName, TypedSlot};

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
        module_with_features, simple_resource,
    };
    use super::*;
    use lazuli_ir::{CommandInput, CreateEffect, TypeRef, UpdateEffect};

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
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

}
