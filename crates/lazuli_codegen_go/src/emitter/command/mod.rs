//! Cell E3 — `Command` kind emission. Walks every `Command` declared
//! on a feature and emits the typed `<Verb><Resource>Input` struct (when
//! `CommandInput::Typed`) plus the `lazuli.Command[I, O]` value into
//! `<feature>/command.gen.go`.
//!
//! Proposal references:
//! - §3.2 — `lazuli.Command[I,O]` value shape (proven by the runtime
//!   spike at `dist/go/customer/customer.gen.go:48-122`).
//! - §4.1 — Tier 4 slots (`Command.approval`, `Command.external_calls`,
//!   `Command.timeout`, `Command.retry`, `Command.idempotency`,
//!   `Command.deprecated`) lower to the Lazuli Go lib backfill on
//!   `Command[I, O]`.
//! - §11 — boundary discipline: every `lazuli.*` reference flows
//!   through `types::go_type_for` so `imports::ImportSet` records
//!   `lazuli.dev/runtime/lazuli` once for the whole file.
//!
//! ## Effect / Output type axis
//!
//! `CommandEffect` decides the `Command[I, O]` `O` parameter:
//! - `Creates(resource, ...)` → resource pascal name (e.g. `Customer`).
//! - `Updates(resource, ...)` → resource pascal name (Lazuli Go lib's
//!   `UpdatesEffect` returns the loaded row, mirroring `Creates`).
//! - `Deletes(resource)` → resource pascal name (same — the runtime
//!   returns the row the soft-delete touched).
//! - `Returns(typeref)` → emitter resolves the typeref via
//!   `types::go_type_for` (e.g. `Customer`, `[]Tag`, `lazuli.Money`).
//!   This is Returns from §3.2 — pure request/response commands.
//! - `None` → `struct{}` (no effect declared; legacy lowering path).
//!
//! ## Bindings axis
//!
//! `Command.lets` carries `let <name> = <expr>` lines; the spike used
//! these to populate the `Bindings` body. For E3 we lower the simpler
//! and exact source: `CommandEffect::Creates.assignments` /
//! `UpdateEffect.assignments` already carry the structural form
//! `<col> = <expr>`. Each Assignment's `value: Expr::Path(...)`
//! becomes a `lazuli.FromInput("...")` / `lazuli.FromCtx("...")`
//! call; literals fall back to `lazuli.FromConst(<lit>)`.
//!
//! ## Determinism
//!
//! Commands are sorted by `Command.name` before emission. The IR `Vec`
//! ordering mirrors the source `.lzi` lexical order which is already
//! stable per-feature; sorting keeps cross-feature byte-equivalence
//! intact even when IR reordering happens elsewhere.

use lazuli_ir::{
    Command, CommandEffect, CommandInput, Expr, Feature, FieldConstraints, Gate, NamedArg, Path,
    QualifiedName, ReturnsEffect, RouteSlot, TypedSlot,
};
use std::collections::BTreeMap;

use super::cross_feature::CrossFeatureIndex;
use super::error_envelope::{bucket_names_for_external_calls, emit_wrap_helper, sentinel_buckets};
use super::error_resolver::{
    command_error_keys_var, command_has_error_keys, emit_command_error_keys,
};
use super::imports::ImportSet;
use super::module::EmitContext;
use super::patterns::{
    PATTERN_COMMAND_PGX_INSERT, PATTERN_COMMAND_PGX_UPDATE, emit_pattern_header,
};
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

mod lifecycle;
use lifecycle::{
    command_trigger_names, emit_lifecycle_machines, emit_transition_advances,
    lifecycle_transition_for, transition_advances_for_triggers,
};

mod policy;
use policy::format_policy_with_expr;
pub(super) use policy::format_policy_with_expr_public;

mod semantic;
use semantic::{emit_semantic_validate_prelude, semantic_validator_plugins};

mod scope;
use scope::{owner_scope_binding, resolve_scope_bindings};

mod tier4;
use tier4::{build_outbox_index, emit_emits, emit_invalidates, emit_tier4_fields, format_approval};
pub(super) use tier4::{format_deprecation_replacement, format_rate_limit_struct};

mod effects;
use effects::emit_effect;

/// Emit `<feature>/command.gen.go` for a feature, or `None` when the
/// feature declares no commands (mirrors `resource.rs` skip rule —
/// avoid emitting a stray `package <feature>` file).
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
        if let CommandEffect::Returns(ret) = &command.effect {
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
        &super::casing::gen_package_name(&feature.name),
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

fn command_wrap_buckets(commands: &[&Command]) -> std::collections::BTreeSet<&'static str> {
    let referenced: std::collections::BTreeSet<&str> = commands
        .iter()
        .flat_map(|command| bucket_names_for_external_calls(&command.external_calls))
        .collect();
    sentinel_buckets(&referenced)
}

/// Walk a single `Command` — optional Input struct, then the
/// `lazuli.Command[I, O]` value.
fn emit_command(
    p: &mut GoPrinter,
    feature: &Feature,
    command: &Command,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let resource_pascal = effect_resource_pascal(&command.effect);
    let qualified_name = format!("{}.{}", feature.name, command.name);

    write_section_banner(
        p,
        &[
            format!("Command: {qualified_name}"),
            format!("  command {}", command.name),
        ],
    );

    // Input struct emission. `CommandInput::Empty` skips the typed
    // declaration; the Command value still names a Go struct shape so
    // we surface a `struct{}` synthetic when neither typed inputs nor
    // route slots are declared.
    //
    // `route id: ID` slots are folded into the same `<Cmd>Input` struct
    // ahead of the body slots so the Effect's `Bindings{"id":
    // FromInput("ID")}` resolves against a real field. Order matches
    // the typical REST convention (URL path params, then body fields).
    // Empty / Short input forms that declare route slots still emit a
    // struct carrying just the route fields so the runtime can bind
    // them — without this, mutating commands with `route id: ID` and
    // no body would 400-error on every dispatch.
    let route_slots = command.route.as_slice();
    let input_type = match &command.input {
        CommandInput::Typed(slots) => {
            let input_struct = command_input_struct_name(&command.name, &resource_pascal);
            emit_input_struct(p, &input_struct, route_slots, slots, ctx);
            p.blank();
            input_struct
        }
        CommandInput::Short(_) => {
            // Short form is sugar for typed inputs whose types live on
            // the targeted resource fields. The analyzer doesn't yet
            // expand them; until then we emit a synthetic struct
            // populated with only the route slots (if any) and a TODO
            // comment so the gap surfaces at review time.
            let input_struct = command_input_struct_name(&command.name, &resource_pascal);
            p.line(&format!(
                "// TODO(short-input): command {} declares a short input list;",
                command.name
            ));
            p.line("// expand against the targeted resource fields (proposal §3.2).");
            emit_input_struct(p, &input_struct, route_slots, &[], ctx);
            p.blank();
            input_struct
        }
        CommandInput::Empty => {
            if route_slots.is_empty() {
                "struct{}".to_owned()
            } else {
                let input_struct = command_input_struct_name(&command.name, &resource_pascal);
                emit_input_struct(p, &input_struct, route_slots, &[], ctx);
                p.blank();
                input_struct
            }
        }
    };

    // Output type resolves from the effect. `None` falls back to
    // `struct{}` so the Command[I,O] still parses.
    let output_type = command_output_type(&command.effect, ctx);
    let lifecycle_transition = lifecycle_transition_for(feature, command);

    let var_name = command_var_name(&command.name, &resource_pascal);

    // Cell CODEGEN-1 (IR Error-Vocab) — when the command declares
    // `policy_when_denied @translation.<key>`, emit the per-command
    // `var <cmd>ErrorKeys = lazuli.ErrorKeys{ ... }` literal first so
    // the `lazuli.Command[I, O]{ ... }` value below can reference it
    // via the `ErrorKeys` kv row. The runtime resolver consults this
    // struct as step 1 of the resolution chain (proposal §2.E).
    if command_has_error_keys(command, Some(&feature.policies)) {
        emit_command_error_keys(p, command, &feature.name, Some(&feature.policies));
        p.blank();
    }

    let pattern = match command.effect {
        CommandEffect::Updates(_) => PATTERN_COMMAND_PGX_UPDATE,
        _ => PATTERN_COMMAND_PGX_INSERT,
    };
    emit_pattern_header(p, pattern);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, command.span_ref);
    p.line(&format!(
        "var {var_name} = lazuli.Command[{input_type}, {output_type}]{{"
    ));
    p.indent();

    // Aligned key block — mirrors the resource emitter's value block.
    // Keys come in a fixed surface-level order so the result is
    // byte-equivalent regardless of which IR slots are populated.
    let mut kv_rows: Vec<(String, String)> = Vec::new();
    kv_rows.push(("Name:".to_owned(), format!("\"{qualified_name}\",")));
    if let Some(resource_var) = effect_resource_var(&command.effect) {
        kv_rows.push(("Resource:".to_owned(), format!("&{resource_var},")));
    }
    kv_rows.push((
        "Policy:".to_owned(),
        format_policy_with_expr(
            &command.policy,
            command.policy_expr.as_ref(),
            Some(&feature.policies),
        ),
    ));
    if let Some(rate) = &command.rate_limit {
        // `ir-rate-limit-env-aware` Cell 2 — emit the env-qualified
        // `lazuli.RateLimit` struct shape. The runtime's `Resolve()`
        // picks the active limit per request against `LAZULI_ENV`.
        // Printer is at indent_level=1 inside the Command literal, so
        // continuation lines get one absolute tab prefix.
        kv_rows.push((
            "RateLimit:".to_owned(),
            format!("{},", format_rate_limit_struct(rate, "\t")),
        ));
    }
    if command.audit.is_some() {
        // Lazuli Go lib has `AuditDefault` + bespoke `AuditSpec`. The
        // IR carries subject lists + optional `emit_to`, both of which
        // map onto `AuditSpec.Fields`. Until the lib grows the
        // `emit_to` slot we emit the default marker — the captured
        // subjects round-trip through the audit-default behaviour.
        kv_rows.push(("Audit:".to_owned(), "lazuli.AuditDefault,".to_owned()));
    }
    if let Some(approval) = &command.approval {
        kv_rows.push(("Approval:".to_owned(), format_approval(approval)));
    }
    // Cell CODEGEN-1 — when the command declares
    // `policy_when_denied`, point the runtime at the per-command
    // `<cmd>ErrorKeys` literal emitted above. The Lazuli Go runtime
    // (`lazuli.Command[I, O].ErrorKeys` field, Cell RUNTIME-1) reads
    // this pointer at handler-construction time to short-circuit the
    // resolver chain on `policy_denied`.
    if command_has_error_keys(command, Some(&feature.policies)) {
        let keys_var = command_error_keys_var(command);
        kv_rows.push(("ErrorKeys:".to_owned(), format!("&{keys_var},")));
    }
    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "command", &command.name, command.span_ref);

    // Effect block — multi-line. Emitted unaligned (independent
    // formatting from the kv block above).
    let let_bindings: BTreeMap<&str, &Expr> = command
        .lets
        .iter()
        .map(|binding| (binding.name.as_str(), &binding.value))
        .collect();
    // Resolve scope atoms (`@scope.owner`, `@scope.same_org`) from the
    // command's policy. When present, codegen auto-injects ownership /
    // tenant-scoping WHERE bindings on Update / Delete effects so the
    // emitted SQL constrains the row at the database, not just at the
    // policy-check gate. Matches the hostpoint pilot pattern surfaced
    // 2026-05-17 (closes the SHIP-NOW row-ownership gap).
    let mut scope_bindings = resolve_scope_bindings(command, feature);
    // `ir-resource-conventions-owner-scope.md` §7.3 + §8.1-8.2 —
    // project the analyzer-composed `owner_scope_sql` slot into the
    // same WHERE-binding pipeline. The carrier is populated by the
    // crud / me synth passes on resources carrying a
    // `@owner_axis(through: <col>)` field; absence is the tenant-only
    // default. We emit the binding through the existing
    // `FromCtxOwnedVia` shape (column-traversal subquery) so the
    // runtime composes `<fk_col> IN (SELECT id FROM <fk_table>
    // WHERE <through> = $N)` after the existing tenant predicates,
    // matching spec §8.1 verbatim. Author-override commands carry
    // `owner_scope_sql: None` (handled by the analyzer); they may
    // still emit `@scope.owner` via the legacy path above. We dedupe
    // so a single resource doesn't get the same FK column bound twice.
    if let Some(binding) = owner_scope_binding(command.owner_scope_sql.as_ref()) {
        if !scope_bindings.iter().any(|b| b.column == binding.column) {
            scope_bindings.push(binding);
        }
    }
    let scope_bindings = scope_bindings;

    emit_effect(
        p,
        &feature.name,
        command,
        &command.name,
        &command.effect,
        command.handler.as_ref(),
        &input_type,
        ctx,
        &let_bindings,
        lifecycle_transition.as_ref(),
        &scope_bindings,
    );
    let transition_advances =
        transition_advances_for_triggers(feature, &command.effect, command_trigger_names(command));
    emit_transition_advances(p, &transition_advances);

    // Emits block.
    if !command.emits.is_empty() {
        let outbox_index = build_outbox_index(feature);
        emit_emits(p, &command.emits, &outbox_index);
    }

    // Invalidates block.
    if !command.invalidates.is_empty() {
        emit_invalidates(p, &command.invalidates, &feature.name);
    }

    // Tier 4 operational/lifecycle fields. Approval is emitted in the
    // aligned key block above so it stays next to Audit in runtime
    // field order.
    emit_tier4_fields(p, feature, command);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
    p.blank();
    let gates = emit_ctx.gates_for("command", &command.name);
    emit_command_handler_wrapper(
        p,
        feature,
        command,
        &var_name,
        &input_type,
        &output_type,
        pattern,
        gates,
    );
}

fn emit_command_handler_wrapper(
    p: &mut GoPrinter,
    feature: &Feature,
    command: &Command,
    var_name: &str,
    input_type: &str,
    output_type: &str,
    pattern: (&str, &str),
    gates: &[Gate],
) {
    emit_pattern_header(p, pattern);
    p.line(&format!(
        "func {}(ctx *lazuli.Ctx, input {input_type}) ({output_type}, error) {{",
        command_handler_func_name(&command.name)
    ));
    p.indent();
    p.line("if ctx.Context == nil {");
    p.indent();
    p.line("ctx.Context = context.Background()");
    p.dedent();
    p.line("}");
    p.line("ctx.Context = lazuli.WithSource(ctx.Context, lazuli.SourceTag{");
    p.indent();
    p.line(&format!("Feature: \"{}\",", escape_string(&feature.name)));
    p.line("Kind:    \"command\",");
    p.line(&format!("Op:      \"{}\",", escape_string(&command.name)));
    p.dedent();
    p.line("})");
    p.line("var endOp func()");
    p.line("ctx.Context, endOp = observability.StartOp(ctx.Context)");
    p.line("defer endOp()");
    // LAZ-SEMANTIC-AUTO-VALIDATE (ir-semantic-auto-validate-2026-05-22).
    // Pre-handler validation pass for fields whose type is a
    // @semantic.X scalar with a plugin-declared validator. Returns
    // validation_failed with {data:{fields:{<field>:<code>}}} — the
    // same shape useLazuliFormRHF + setServerFieldErrors expects.
    emit_semantic_validate_prelude(p, command, output_type);
    // PG.C.1 — plan-gate prelude (pre-dispatch). Behind-gates run
    // first (boolean feature check → 402 plan.feature_forbidden on
    // failure). Quota gates next (counter check → 402
    // plan.quota_exceeded on failure). Order matches
    // docs/proposals/plan-and-gate-vocab.md §"Ordering and
    // combinability".
    let (behind_gates, quota_gates) = partition_gates(gates);
    emit_command_gate_prelude(p, output_type, &behind_gates, &quota_gates);
    if quota_gates.is_empty() {
        p.line(&format!("return {var_name}.Handle(ctx, input)"));
    } else {
        // Post-success quota increment path: capture the wrapped
        // result, then conditionally bump every quota counter before
        // returning. Increment errors are swallowed (logged by the
        // runtime); the user-visible response is the handler's result.
        p.line(&format!("out, err := {var_name}.Handle(ctx, input)"));
        p.line("if err == nil {");
        p.indent();
        for limit in &quota_gates {
            p.line(&format!(
                "_ = billing.IncrQuota(ctx, plan.Catalog, {:?})",
                limit
            ));
        }
        p.dedent();
        p.line("}");
        p.line("return out, err");
    }
    p.dedent();
    p.line("}");
}

/// PG.C.1 — split the authored gate list into the two evaluation
/// buckets. `gate behind plan.feature` checks fire first; `gate quota
/// plan.limit` checks (and their post-success increments) fire after.
fn partition_gates<'a>(gates: &'a [Gate]) -> (Vec<&'a str>, Vec<&'a str>) {
    let mut behinds = Vec::new();
    let mut quotas = Vec::new();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => behinds.push(feature.as_str()),
            Gate::Quota { limit } => quotas.push(limit.as_str()),
        }
    }
    (behinds, quotas)
}

/// PG.C.1 — emit the pre-dispatch gate prelude. Each `gate behind`
/// becomes a `billing.CheckFeature(...)` short-circuit; each `gate
/// quota` becomes a `billing.CheckQuota(...)` short-circuit. The
/// post-success `billing.IncrQuota` calls are emitted by the
/// surrounding wrapper because they need to read the handler's
/// `(out, err)` return.
fn emit_command_gate_prelude(
    p: &mut GoPrinter,
    output_type: &str,
    behind_gates: &[&str],
    quota_gates: &[&str],
) {
    if behind_gates.is_empty() && quota_gates.is_empty() {
        return;
    }
    let zero = zero_value_for_go_type(output_type);
    for feature in behind_gates {
        p.line(&format!("// gate: behind plan.feature {feature}"));
        p.line(&format!(
            "if err := billing.CheckFeature(ctx, plan.Catalog, {:?}); err != nil {{",
            feature
        ));
        p.indent();
        p.line(&format!("return {zero}, err"));
        p.dedent();
        p.line("}");
    }
    for limit in quota_gates {
        p.line(&format!("// gate: quota plan.limit {limit}"));
        p.line(&format!(
            "if err := billing.CheckQuota(ctx, plan.Catalog, {:?}); err != nil {{",
            limit
        ));
        p.indent();
        p.line(&format!("return {zero}, err"));
        p.dedent();
        p.line("}");
    }
}

/// PG.C.1 helper — best-effort zero literal for a Go return type.
/// Used by the gate prelude when it has to short-circuit before the
/// wrapped handler runs. Falls back to `*new(T)` when the type is too
/// shaped to write a literal for (named structs, generics).
pub(super) fn zero_value_for_go_type(ty: &str) -> String {
    let trimmed = ty.trim();
    match trimmed {
        "string" => "\"\"".to_owned(),
        "bool" => "false".to_owned(),
        "int" | "int8" | "int16" | "int32" | "int64" => "0".to_owned(),
        "uint" | "uint8" | "uint16" | "uint32" | "uint64" => "0".to_owned(),
        "float32" | "float64" => "0".to_owned(),
        "any" => "nil".to_owned(),
        "error" => "nil".to_owned(),
        "struct{}" => "struct{}{}".to_owned(),
        _ if trimmed.starts_with('*')
            || trimmed.starts_with('[')
            || trimmed.starts_with("map[")
            || trimmed.starts_with("chan ") =>
        {
            "nil".to_owned()
        }
        _ => format!("*new({trimmed})"),
    }
}

/// Emit the `type <Name>Input struct` block for a command.
///
/// Field order: route slots first (URL path params — `route id: ID`),
/// then typed body slots. Route params always emit `validate:"required"`
/// because the path is the addressing key; without them the Effect's
/// `Bindings{...: FromInput("ID")}` resolves against nothing.
fn emit_input_struct(
    p: &mut GoPrinter,
    name: &str,
    route_slots: &[RouteSlot],
    slots: &[TypedSlot],
    ctx: &TypeCtx<'_>,
) {
    p.line(&format!("type {name} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> =
        Vec::with_capacity(route_slots.len() + slots.len());
    // Route slots come first — `route id: ID` becomes `Id ID
    // \`json:"id" validate:"required"\``. Route slots have no inline
    // constraints in the IR (`RouteSlot` carries no `FieldConstraints`),
    // and are always required by definition (the URL path can't be
    // optional).
    let empty_constraints = FieldConstraints::default();
    for slot in route_slots {
        let (go_type, _import) = types::go_type_for(&slot.type_ref, ctx);
        let validate_body = super::validator_tag_body(&empty_constraints, true);
        let tag = if validate_body.is_empty() {
            format!("`json:\"{}\"`", slot.name)
        } else {
            format!("`json:\"{}\" validate:\"{}\"`", slot.name, validate_body)
        };
        rows.push((pascal_case(&slot.name), go_type, tag));
    }
    for slot in slots {
        let (go_type, _import) = types::go_type_for(&slot.type_ref, ctx);
        let optional = !slot.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", slot.name)
        } else {
            slot.name.clone()
        };
        // L0 #3 §10 — pick up inline constraints (Cells D.1+D.3). The
        // tag chain stays deterministic: `json:"…"` then optional
        // `validate:"…"` (only when the slot is required OR carries
        // at least one constraint).
        let validate_body = super::validator_tag_body(&slot.constraints, slot.required);
        let tag = if validate_body.is_empty() {
            format!("`json:\"{}\"`", json_suffix)
        } else {
            format!("`json:\"{}\" validate:\"{}\"`", json_suffix, validate_body)
        };
        rows.push((pascal_case(&slot.name), final_type, tag));
    }
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(n, t, g)| (n.as_str(), t.as_str(), g.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

pub(super) fn format_path(path: &Path) -> String {
    path.segments.join(".")
}

pub(super) fn format_args_key(args: &[NamedArg]) -> String {
    sorted_arg_strings(args).join("\u{1f}")
}

pub(super) fn sorted_arg_strings(args: &[NamedArg]) -> Vec<String> {
    let mut out: Vec<String> = args
        .iter()
        .map(|arg| format!("{}={}", arg.name, format_expr(&arg.value)))
        .collect();
    out.sort();
    out
}

pub(super) fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => format_path(path),
        Expr::String(value) => format!("\"{}\"", escape_string(value)),
        Expr::Integer(value) => value.to_string(),
        Expr::Boolean(value) => value.to_string(),
        Expr::Enum(literal) => match &literal.type_name {
            Some(qname) => format!("{}.{}", format_qname(None, qname), literal.variant),
            None => literal.variant.clone(),
        },
        Expr::Nil => "nil".to_owned(),
        // Diagnostic-only render for FnCall; binding sites use the
        // typed `format_binding_source` path instead.
        Expr::FnCall(call) => {
            let args: Vec<String> = call.args.iter().map(format_expr).collect();
            format!("@fn.{}({})", call.name.name, args.join(", "))
        }
    }
}

fn format_qname(default_feature: Option<&str>, qname: &QualifiedName) -> String {
    if qname.name.contains('.') && qname.feature.is_none() {
        return qname.name.clone();
    }
    match qname.feature.as_deref().or(default_feature) {
        Some(feature) => format!("{feature}.{}", qname.name),
        None => qname.name.clone(),
    }
}

/// Resolve the Go-side `<resource>Resource` variable name from a
/// qualified IR resource name. The resource emitter declared this var
/// in the same package using the lowerCamel form of `Resource.name`;
/// we mirror the convention here.
pub(super) fn resource_var_for_qname(qname: &QualifiedName) -> String {
    // Cross-feature resource references would need a cross-package
    // dotted form, but Command.Effect carries `feature: None` today
    // (commands write to same-feature resources by language rule).
    // When the IR ever lifts cross-feature writes the typed slot lands
    // on this branch; until then we emit a bare lower-camel ref.
    format!("{}Resource", lower_camel(&qname.name))
}

/// Resolve the Output type for the `Command[I, O]` generic. Effects
/// pin the type to the resource pascal name; `Returns` consumes the
/// declared `TypeRef`; `None` falls back to an empty struct so the
/// generic still parses.
///
/// For `Returns`, we use `go_return_type_for` (not `go_type_for`) so
/// resource refs render as the full struct (`User`) rather than the
/// FK collapse (`lazuli.ID`). The FK collapse is correct for field
/// positions (BIGINT column) and wrong for return positions (handler
/// returns the typed row, not the id).
fn command_output_type(effect: &CommandEffect, ctx: &TypeCtx<'_>) -> String {
    match effect {
        CommandEffect::Creates(c) => pascal_case(&c.resource.name),
        CommandEffect::Updates(u) => pascal_case(&u.resource.name),
        CommandEffect::Deletes(d) => pascal_case(&d.resource.name),
        CommandEffect::Returns(r) => {
            let (ty, _import) = types::go_return_type_for(&r.return_type, ctx);
            ty
        }
        CommandEffect::None => "struct{}".to_owned(),
    }
}

/// Returns the resource pascal name pinned by the command's effect.
/// Used for the input struct naming axis.
pub(super) fn effect_resource_pascal(effect: &CommandEffect) -> String {
    match effect {
        CommandEffect::Creates(c) => pascal_case(&c.resource.name),
        CommandEffect::Updates(u) => pascal_case(&u.resource.name),
        CommandEffect::Deletes(d) => pascal_case(&d.resource.name),
        CommandEffect::Returns(r) => match &r.return_type {
            lazuli_ir::TypeRef::UserDefined(q) | lazuli_ir::TypeRef::EnumRef(q) => {
                pascal_case(&q.name)
            }
            _ => "Result".to_owned(),
        },
        CommandEffect::None => "Result".to_owned(),
    }
}

/// `Some(resource_var)` when the command has a resource-bound effect,
/// otherwise `None` so we skip emitting the `Resource:` field
/// entirely.
fn effect_resource_var(effect: &CommandEffect) -> Option<String> {
    match effect {
        CommandEffect::Creates(c) => Some(resource_var_for_qname(&c.resource)),
        CommandEffect::Updates(u) => Some(resource_var_for_qname(&u.resource)),
        CommandEffect::Deletes(d) => Some(resource_var_for_qname(&d.resource)),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// Sibling emitters (`api.rs`, `webhook.rs`, `query.rs`, etc.) re-export
/// the structured form so they can lower `policy_expr` without
/// duplicating the walker logic. The `policies` argument carries the
/// feature-local `Policies` block so `PolicyRef::Local("X")` can be
/// resolved to its atom decomposition at codegen time
/// (WAR-RUNTIME-POLICY-01).

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

/// Walk a `TypeRef` and register every surfaced import on the
/// file-level `ImportSet`. Mirrors `resource.rs::register_imports_for_type`.
fn register_imports_for_type(
    type_ref: &lazuli_ir::TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let lazuli_ir::TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}

/// `customer.create` -> `CreateCustomerInput`. Multi-word commands
/// like `update_email` slot the resource between the verb and the
/// modifier: `UpdateCustomerEmailInput`. Mirrors the spike's
/// `command_input_struct_name` so generated names stay stable.
pub(super) fn command_input_struct_name(short_name: &str, resource_pascal: &str) -> String {
    let mut parts = short_name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();

    let mut out = pascal_case(verb);
    out.push_str(resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out.push_str("Input");
    out
}

/// Command var name: lowerCamel mirror of the input struct without the
/// `Input` suffix. `create` -> `createCustomer`; `update_email` ->
/// `updateCustomerEmail`. Mirrors the spike for byte-equivalence.
pub(super) fn command_var_name(short_name: &str, resource_pascal: &str) -> String {
    let mut parts = short_name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();

    let mut out = verb.to_ascii_lowercase();
    out.push_str(resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out
}

fn command_handler_func_name(short_name: &str) -> String {
    format!("Handle{}", pascal_case(short_name))
}

pub(super) fn pascal_case(s: &str) -> String {
    super::casing::pascal_case(s)
}

pub(super) fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

/// Escape backslashes and double-quotes so a Go string literal stays
/// well-formed. Backticks are not used here because every literal
/// we emit is double-quoted (single-line strings).
pub(super) fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

// Unused (today) but kept so the Returns/None branches can graduate
// to a typed `ReturnsEffect` codepath without re-importing the symbol.
#[allow(dead_code)]
fn _returns_effect_compiles(_: ReturnsEffect) {}

/// `ir-rate-limit-env-aware` Cell 2 — emit the env-qualified
/// `lazuli.RateLimit` struct literal for Command / Api / Agent /
/// Report consumers. The string fragment slots in after `RateLimit:`
/// on an aligned kv row (no trailing comma so the caller appends one);
/// when `by_env` is non-empty, the fragment carries embedded newlines
/// where each continuation line is prefixed by `continuation_indent`
/// so it lines up under the container's struct fields.
///
/// `continuation_indent` is the absolute tab prefix for lines AFTER
/// the first (the printer adds its own `indent_level` to the first
/// line via `p.line(...)`). For the Command emitter at
/// `indent_level == 1`, callers pass `"\t"` so child lines like
/// `\t\tDefault: "..."` line up one tab deeper than `RateLimit:`.
///
/// Shapes (proposal §7.2 + cell 2 spec):
///  * default only, non-empty             → `lazuli.RateLimit{Default: "X"}`
///  * default + by_env entries            → multi-line struct literal
///  * default empty AND by_env empty      → `lazuli.RateLimit{}`
///
/// The runtime's `Resolve()` resolves the active limit at request time
/// against `LAZULI_ENV`; empty Default + empty ByEnv == no throttle.

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, Assignment, BuiltinType, CommandKind, CreateEffect, Defaults, Feature, Module,
        Policies, PolicyRef, QualifiedName, Resource, TypeRef,
    };

    fn base_feature(name: &str) -> Feature {
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
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
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

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
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
        }
    }

    fn typed_slot(name: &str, builtin: BuiltinType, required: bool) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }
    }

    fn local_qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
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
    use super::*;
    use lazuli_ir::{
        AppManifest, Assignment, BackoffStrategy, BuiltinType, CommandKind, CreateEffect, Defaults,
        DeleteEffect, DeprecationReplacement, EnumLiteral, EnvName, Feature, HandlerRef,
        IdempotencyKey, InvalidatesSpec, LetBinding, Lifecycle, LifecycleState, LifecycleStateKind,
        LifecycleTransition, Module, NamedArg, Path, Policies, PolicyExpr, PolicyRef,
        QualifiedName, RateLimitByEnv, RateLimitSpec, Record, Resource, RetryPolicy, ReturnsEffect,
        RouteSlot, Tenancy, TypeRef, UpdateEffect,
    };

    fn base_feature(name: &str) -> Feature {
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
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
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

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
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
        }
    }

    fn lifecycle_resource(name: &str, field: &str, enum_name: &str) -> Resource {
        let mut resource = simple_resource(name);
        resource.lifecycle = Some(Lifecycle {
            discriminator_field: field.to_owned(),
            generated_enum: enum_name.to_owned(),
            states: vec![
                LifecycleState {
                    name: "scheduled".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "publishing".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
                LifecycleState {
                    name: "published".to_owned(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![LifecycleTransition {
                name: "begin_publishing".to_owned(),
                from: vec!["scheduled".to_owned()],
                to: "publishing".to_owned(),
                policy: None,
                audit: None,
                timestamps: Some("publishing_at".to_owned()),
                emits: Vec::new(),
                requires: None,
                tests: None,
                previous_names: Vec::new(),
                span_ref: None,
            }],
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        });
        resource
    }

    fn typed_slot(name: &str, builtin: BuiltinType, required: bool) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }
    }

    fn local_qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    /// Helper: emit `command.gen.go` for the given feature.
    fn emit(feature: &Feature) -> Option<String> {
        let mut features = vec![feature.clone()];
        // Ensure the resource targeted by command effects exists somewhere
        // in the module so the cross-feature index can resolve it.
        if !feature
            .commands
            .iter()
            .all(|c| matches!(c.effect, CommandEffect::None))
        {
            features[0].resources.push(simple_resource("Customer"));
        }
        let module = module_with_features(features);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/command.gen.go");
        emit_command_file(
            "examples/x.lzi",
            &module.features[0],
            "lazuli/test",
            &index,
            &emit_ctx,
        )
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
    fn lifecycle_command_emits_apply_call() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        let mut cmd = base_command("begin_publishing");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/lifecycle\""));
        assert!(out.contains("var publicationLifecycle = lifecycle.New[PublicationStatus]("));
        assert!(out.contains(
            "// lifecycle: newState, err := publicationLifecycle.Apply(ctx, current.Status, \"begin_publishing\")"
        ));
    }

    #[test]
    fn command_triggers_emit_transition_advances_in_order() {
        let mut feature = base_feature("publication");
        let mut resource = simple_resource("Publication");
        resource.lifecycle = Some(Lifecycle {
            discriminator_field: "lifecycle_state".to_owned(),
            generated_enum: "PublicationState".to_owned(),
            states: vec![
                LifecycleState {
                    name: "basic_details_pending".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "address_pending".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
                LifecycleState {
                    name: "review_pending".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
            ],
            transitions: vec![
                LifecycleTransition {
                    name: "T1".to_owned(),
                    from: vec!["basic_details_pending".to_owned()],
                    to: "address_pending".to_owned(),
                    policy: None,
                    audit: None,
                    timestamps: None,
                    emits: Vec::new(),
                    requires: None,
                    tests: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                },
                LifecycleTransition {
                    name: "T2".to_owned(),
                    from: vec!["address_pending".to_owned()],
                    to: "review_pending".to_owned(),
                    policy: None,
                    audit: None,
                    timestamps: None,
                    emits: Vec::new(),
                    requires: None,
                    tests: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                },
            ],
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        });
        feature.resources.push(resource);

        let mut cmd = base_command("advance_publication");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        let triggers = vec!["T1".to_owned(), "T2".to_owned()];

        let transitions = transition_advances_for_triggers(&feature, &cmd.effect, &triggers);
        let mut p = GoPrinter::new();
        emit_transition_advances(&mut p, &transitions);
        let out = p.finish();

        assert!(out.contains("Transitions: []lazuli.TransitionAdvance{"));
        let first = "{From: \"basic_details_pending\", To: \"address_pending\"},";
        let second = "{From: \"address_pending\", To: \"review_pending\"},";
        assert!(out.contains(first), "first trigger pair missing:\n{out}");
        assert!(out.contains(second), "second trigger pair missing:\n{out}");
        assert!(
            out.find(first) < out.find(second),
            "trigger order should be preserved:\n{out}"
        );
    }

    #[test]
    fn non_lifecycle_command_does_not_emit_apply_call() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        let mut cmd = base_command("rename");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(!out.contains(".Apply(ctx,"));
    }

    #[test]
    fn multi_lifecycle_resources_emit_per_resource_vars() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        feature
            .resources
            .push(lifecycle_resource("Issue", "state", "IssueState"));
        feature.commands.push(base_command("noop"));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("var issueLifecycle = lifecycle.New[IssueState]("));
        assert!(out.contains("var publicationLifecycle = lifecycle.New[PublicationStatus]("));
        assert_eq!(out.matches("lifecycle.New[").count(), 2);
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

    // -------------------------------------------------------------------------
    // @scope.owner / @scope.same_org policy-to-SQL lowering (hostpoint pilot
    // 2026-05-17). Closes the SHIP-NOW row-ownership gap surfaced by the
    // capability matrix audit.
    // -------------------------------------------------------------------------

    fn scope_field(name: &str) -> lazuli_ir::Field {
        lazuli_ir::Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn feature_with_owner_scope_policy() -> Feature {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "delete".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        feature
    }

    #[test]
    fn deletes_with_scope_owner_injects_user_id_where_binding() {
        let mut feature = feature_with_owner_scope_policy();
        let mut cmd = base_command("revoke_session");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("delete".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "still binds id from input:\n{out}"
        );
        assert!(
            out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should inject user_id WHERE binding:\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.owner resolved → user_id = ctx.user.id"),
            "scope comment should surface for reviewers:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_owner_injects_user_where_binding_when_user_id_absent() {
        // Resource has `user` (not `user_id`) — closed-catalog falls
        // through to the second candidate per priority.
        let mut feature = base_feature("messaging");
        let mut resource = simple_resource("NotificationDelivery");
        resource.fields.push(scope_field("user"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("mark_notification_read");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("NotificationDelivery"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should resolve to `user` field when user_id absent:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_same_org_injects_org_id_where_binding() {
        let mut feature = base_feature("billing");
        let mut resource = simple_resource("Charge");
        resource.fields.push(scope_field("org_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.same_org".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("flag_review");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Charge"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"org_id\": lazuli.FromCtx(\"user.org_id\"),"),
            "@scope.same_org should inject org_id WHERE binding:\n{out}"
        );
    }

    #[test]
    fn no_scope_atom_emits_baseline_where_binding() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        // No @scope.* atom in the policy.
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "admin".to_owned(),
                atoms: vec!["@role.admin".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("purge");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("admin".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // Baseline binds id from input. No scope injection.
        assert!(out.contains("\"id\": lazuli.FromInput(\"ID\"),"));
        assert!(
            !out.contains("FromCtx(\"user.id\")"),
            "no @scope.* atom → no auto-injected scope binding:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_owner_traverses_relation_when_no_direct_column() {
        // Property has no direct owner column but `host: Host required`
        // references the Host resource which has `user_id`. Codegen
        // should emit FromCtxOwnedVia("Host", "user_id", "user.id").
        let mut feature = base_feature("catalog");

        let mut host = simple_resource("Host");
        host.fields.push(scope_field("user_id"));
        feature.resources.push(host);

        let mut property = simple_resource("Property");
        // `host` field referencing the Host resource.
        property.fields.push(lazuli_ir::Field {
            name: "host".to_owned(),
            type_ref: TypeRef::Unresolved("Host".to_owned()),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        });
        feature.resources.push(property);

        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("publish_property");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Property"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"Host\", \"user_id\", \"user.id\"),"),
            "@scope.owner should traverse host → Host.user_id when Property has no direct column:\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.owner resolved via host → Host.user_id = ctx.user.id"),
            "scope comment should document the traversal:\n{out}"
        );
    }

    #[test]
    fn scope_owner_without_matching_column_skips_silently() {
        // Resource has no owner-like column. Codegen must not invent a
        // binding; doctor surfaces the warning separately.
        let mut feature = base_feature("trust");
        let mut resource = simple_resource("Review");
        resource.fields.push(scope_field("status")); // unrelated field
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("flag");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Review"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            !out.contains("FromCtx(\"user.id\")"),
            "no matching column → no scope binding emitted:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Alt-key WHERE binding (Wave 8). When a delete/update command has no
    // `route` and a single typed input slot whose name is NOT `id`, the
    // codegen now uses that slot as the WHERE key (column + Go input
    // field). Closes the hostpoint Phase 4 codegen gap surfaced 2026-05-17.
    // -------------------------------------------------------------------------

    #[test]
    fn deletes_with_single_input_slot_uses_alt_key_when_not_id() {
        let mut feature = base_feature("messaging");
        let mut resource = simple_resource("WebPushSubscription");
        resource.fields.push(scope_field("endpoint"));
        resource.fields.push(scope_field("user"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "delete".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("unregister_web_push");
        cmd.input = CommandInput::Typed(vec![typed_slot("endpoint", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("WebPushSubscription"),
        });
        cmd.policy = PolicyRef::Local("delete".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"endpoint\": lazuli.FromInput(\"Endpoint\"),"),
            "single-slot input `endpoint` should drive WHERE:\n{out}"
        );
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "no `id` binding should leak when input slot is `endpoint`:\n{out}"
        );
        assert!(
            out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should still inject the ownership column:\n{out}"
        );
    }

    #[test]
    fn updates_with_route_slot_uses_route_as_where_key() {
        let mut feature = base_feature("trust");
        let mut resource = simple_resource("Review");
        resource.fields.push(scope_field("status"));
        feature.resources.push(resource);
        let mut cmd = base_command("flag");
        cmd.route = vec![lazuli_ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![typed_slot("reason", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Review"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // Route drives the WHERE key. `reason` is the body slot, not a
        // WHERE key candidate.
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\")"),
            "route id should drive WHERE:\n{out}"
        );
        assert!(
            !out.contains("\"reason\": lazuli.FromInput(\"Reason\"),"),
            "non-route, non-key input should not leak into WHERE bindings:\n{out}"
        );
        // LAZ-route-id-codegen-go (Cell A1) — the route id slot must
        // ALSO be present on the Input struct so the FromInput("ID")
        // binding above resolves at dispatch.
        assert!(
            out.contains("ID     lazuli.ID `json:\"id\" validate:\"required\"`"),
            "route id slot must land on the Input struct as `ID lazuli.ID`:\n{out}"
        );
        assert!(
            out.contains("Reason string    `json:\"reason\" validate:\"required\"`"),
            "body Reason field must still be present:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // @scope.self — ctx-as-key WHERE binding (Wave 9 / hostpoint codegen gap G).
    // Closes `account.choose_role` UPDATE WHERE id = ctx.user.id.
    // -------------------------------------------------------------------------

    #[test]
    fn updates_with_scope_self_uses_ctx_user_id_as_where_key() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("User");
        resource.fields.push(scope_field("role"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "choose_role".to_owned(),
                atoms: vec!["@scope.self".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("choose_role");
        cmd.input = CommandInput::Typed(vec![typed_slot("role", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("User"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("choose_role".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // @scope.self drives WHERE via ctx; the `role` input slot is
        // a body field, not a key.
        assert!(
            out.contains("\"id\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.self should bind id from ctx.user.id:\n{out}"
        );
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\""),
            "@scope.self must suppress the route/input id binding (no double-id):\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.self resolved → id = ctx.user.id"),
            "scope comment should document the ctx-key pattern:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Bulk delete — @scope.owner with no route AND no typed input
    // (Wave 9 / hostpoint codegen gap H). Closes `account.logout` etc.
    // -------------------------------------------------------------------------

    #[test]
    fn deletes_in_bulk_mode_drops_legacy_id_binding() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "logout".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("logout");
        cmd.input = CommandInput::Empty;
        // No route slots either.
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("logout".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "bulk delete must NOT emit legacy id-from-input binding:\n{out}"
        );
        assert!(
            out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
            "scope.owner should still inject the ownership binding:\n{out}"
        );
        assert!(
            out.contains("// bulk: no id/route key"),
            "bulk-mode comment should be visible for reviewers:\n{out}"
        );
    }

    #[test]
    fn deletes_with_multi_route_emits_composite_where() {
        let mut feature = base_feature("customer_tags");
        let mut resource = simple_resource("CustomerTagAssignment");
        feature.resources.push(resource.clone());
        let mut cmd = base_command("remove_tag");
        cmd.route = vec![
            lazuli_ir::RouteSlot {
                name: "customer_id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                from: None,
                kind: lazuli_ir::RouteSlotKind::Plain,
            },
            lazuli_ir::RouteSlot {
                name: "tag_id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                from: None,
                kind: lazuli_ir::RouteSlotKind::Plain,
            },
        ];
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("CustomerTagAssignment"),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"customer_id\": lazuli.FromInput(\"CustomerID\"),"),
            "first route slot should bind (note `id` acronym uppercases per is_acronym):\n{out}"
        );
        assert!(
            out.contains("\"tag_id\": lazuli.FromInput(\"TagID\"),"),
            "second route slot should bind:\n{out}"
        );
        // LAZ-route-id-codegen-go (Cell A1) — Empty-input + route slots
        // must STILL emit a synthetic Input struct carrying the route
        // fields. Without it, FromInput("CustomerID") / FromInput("TagID")
        // would resolve against `struct{}` and return 400 bad_request.
        assert!(
            out.contains("type RemoveCustomerTagAssignmentTagInput struct {"),
            "Empty input + route slots must still emit an Input struct:\n{out}"
        );
        assert!(
            out.contains("CustomerID lazuli.ID `json:\"customer_id\" validate:\"required\"`"),
            "first composite-route slot must surface on the Input struct:\n{out}"
        );
        assert!(
            out.contains("TagID      lazuli.ID `json:\"tag_id\" validate:\"required\"`"),
            "second composite-route slot must surface on the Input struct:\n{out}"
        );
    }

    /// `command me returns User` — the IR lowers to
    /// `CommandEffect::Returns(ReturnsEffect { return_type: UserDefined("User") })`.
    /// The emitted Output generic must be the full resource struct
    /// (`Customer` same-feature, `<owner>gen.Customer` cross-feature),
    /// NOT the `lazuli.ID` FK collapse used for resource-field positions.
    /// Closes the `account.me` 500-internal at dispatch — the runtime's
    /// `ReturnsFromRegistry[I, O]` type-asserts the registered fn as
    /// `func(*Ctx, I) (O, error)`; with `O = lazuli.ID` and the
    /// registered handler returning `(User, error)`, the assertion
    /// fails and the runtime emits a 500 internal.
    #[test]
    fn returns_user_defined_resource_emits_full_struct_not_id() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("me");
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::UserDefined(local_qname("Customer")),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // Output generic in the Command[I, O] declaration is the full
        // struct (`Customer`) — NOT `lazuli.ID`. `command_var_name`
        // composes `meCustomer` from `verb=me, resource=Customer`.
        assert!(
            out.contains("var meCustomer = lazuli.Command[struct{}, Customer]{"),
            "Command[I, O] should pin O to the resource struct, got:\n{out}"
        );
        // Effect's ReturnsFromRegistry generic pins the same struct.
        assert!(
            out.contains("Effect: lazuli.ReturnsFromRegistry[struct{}, Customer]("),
            "ReturnsFromRegistry should pin O to Customer (not lazuli.ID), got:\n{out}"
        );
        assert!(
            !out.contains("ReturnsFromRegistry[struct{}, lazuli.ID]"),
            "regression: ReturnsFromRegistry must NOT collapse Customer to lazuli.ID:\n{out}"
        );
        // Handler comment matches the registered fn shape — the
        // emitted Wire comment names `Customer` as the return type.
        assert!(
            out.contains("(Customer, error)"),
            "handler signature comment should return Customer, got:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Owner-scope projection — cell `codegen-os-projection`. The analyzer
    // composes `Command.owner_scope_sql` per spec
    // `ir-resource-conventions-owner-scope.md` §7.3; this codegen cell
    // pastes the carrier through `FromCtxOwnedVia` (DELETE/UPDATE) and
    // `CreatesWithOwnerCheck` (CREATE) so the emitted SQL matches §8.1 /
    // §8.5.A verbatim after the existing tenant predicates.
    // -------------------------------------------------------------------------

    fn owner_scope_sql_property() -> lazuli_ir::OwnerScopeSql {
        // Mirrors the analyzer's cell-O2 output for Hostpoint's
        // `Property.host: Host required @owner_axis(through: user)`.
        lazuli_ir::OwnerScopeSql {
            field_name: "host".to_owned(),
            fk_target: "Host".to_owned(),
            through_column: "user".to_owned(),
            where_predicate: "host IN (SELECT id FROM \"host\" WHERE \"user\" = ctx.User.ID)"
                .to_owned(),
            cte_owner_check: None,
        }
    }

    #[test]
    fn delete_with_owner_scope_sql_emits_owned_via_binding() {
        // Spec §8.1: synth `delete_property` lowers to
        // `DELETE FROM "property" WHERE id = $1 AND org_id = $2 AND
        //   host IN (SELECT id FROM "host" WHERE "user" = $3)`.
        // Codegen projection: existing `id` binding from route +
        // tenant via baseScopeConditions + FromCtxOwnedVia for the
        // ownership chain. We assert the emitted Go contains the
        // owned-via binding row in the Deletes effect's Where map.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        feature.resources.push(resource);

        let mut cmd = base_command("delete_property");
        cmd.kind = CommandKind::Delete;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("Property"),
        });
        cmd.owner_scope_sql = Some(owner_scope_sql_property());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
            "DELETE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
        );
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "existing route-key id binding must remain:\n{out}"
        );
        assert!(
            out.contains("// scope: @owner_axis resolved via host"),
            "scope-binding comment must document the owner-axis traversal:\n{out}"
        );
    }

    #[test]
    fn delete_without_owner_scope_sql_emits_unchanged_tenant_only_shape() {
        // Resources without `@owner_axis` carry `owner_scope_sql: None`.
        // The emitted Go must be identical to today's tenant-only DELETE
        // shape — no FromCtxOwnedVia binding leaks into the Where map.
        let mut feature = base_feature("billing");
        feature.resources.push(simple_resource("Charge"));

        let mut cmd = base_command("delete_charge");
        cmd.kind = CommandKind::Delete;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("Charge"),
        });
        cmd.owner_scope_sql = None;
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("FromCtxOwnedVia"),
            "DELETE without owner_scope_sql must NOT emit owned-via:\n{out}"
        );
        assert!(
            !out.contains("@owner_axis"),
            "no owner-axis annotation should appear in emitted code when carrier is None:\n{out}"
        );
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "baseline route-key binding must be present:\n{out}"
        );
    }

    #[test]
    fn update_with_owner_scope_sql_emits_owned_via_binding() {
        // Spec §8.2: synth `update_property` lowers to
        // `UPDATE "property" SET ... WHERE id = $1 AND org_id = $4 AND
        //   host IN (SELECT id FROM "host" WHERE "user" = $5)`.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        resource.fields.push(scope_field("name"));
        feature.resources.push(resource);

        let mut cmd = base_command("update_property");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, false)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Property"),
            assignments: vec![Assignment {
                field: "name".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "name"])),
            }],
        });
        cmd.owner_scope_sql = Some(owner_scope_sql_property());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
            "UPDATE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
        );
        // SET-side binding: `name` is an optional input slot (above) so
        // the emitter now picks `FromInputOptional` so the runtime
        // skips the column when the wire payload omits it (partial-
        // update semantics). Required slots keep emitting plain
        // `FromInput`.
        assert!(
            out.contains("\"name\": lazuli.FromInputOptional(\"name\"),"),
            "SET-side optional input must emit FromInputOptional:\n{out}"
        );
    }

    /// Partial-write axis: an UPDATE command whose typed input mixes
    /// required + optional slots must emit `FromInput` for the
    /// required ones and `FromInputOptional` for the optional ones, so
    /// the runtime keeps the existing column value when the wire
    /// payload omits an optional field. Regression for the hostpoint
    /// 2026-05-22 settings-save outage.
    #[test]
    fn update_emits_from_input_optional_for_optional_input_slots() {
        let mut feature = base_feature("widget");
        let mut resource = simple_resource("Widget");
        resource.fields.push(scope_field("name"));
        resource.fields.push(scope_field("color"));
        feature.resources.push(resource);

        let mut cmd = base_command("update_widget");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![
            typed_slot("name", BuiltinType::Text, true),   // required
            typed_slot("color", BuiltinType::Text, false), // optional
        ]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Widget"),
            assignments: vec![
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
                Assignment {
                    field: "color".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "color"])),
                },
            ],
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"name\": lazuli.FromInput(\"name\"),"),
            "required input slot must emit plain FromInput:\n{out}"
        );
        assert!(
            out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
            "optional input slot must emit FromInputOptional:\n{out}"
        );
    }

    /// Mirror of the above for CREATE — required slots stay
    /// `FromInput`, optional slots become `FromInputOptional` so the
    /// INSERT skips columns whose wire field was nil and lets the
    /// column default take effect.
    #[test]
    fn create_emits_from_input_optional_for_optional_input_slots() {
        let mut feature = base_feature("widget");
        let mut resource = simple_resource("Widget");
        resource.fields.push(scope_field("name"));
        resource.fields.push(scope_field("color"));
        feature.resources.push(resource);

        let mut cmd = base_command("create_widget");
        cmd.kind = CommandKind::Create;
        cmd.input = CommandInput::Typed(vec![
            typed_slot("name", BuiltinType::Text, true),
            typed_slot("color", BuiltinType::Text, false),
        ]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Widget"),
            from_input: false,
            assignments: vec![
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
                Assignment {
                    field: "color".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "color"])),
                },
            ],
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"name\": lazuli.FromInput(\"name\"),"),
            "required input slot must emit plain FromInput:\n{out}"
        );
        assert!(
            out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
            "optional input slot must emit FromInputOptional:\n{out}"
        );
    }

    #[test]
    fn create_with_cte_owner_check_emits_creates_with_owner_check() {
        // Spec §8.5.A: synth `create_property` lowers to
        //   WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $<fk>
        //     AND "user" = ctx.User.ID)
        //   INSERT INTO "property" (...) SELECT ... FROM owner_check
        //   RETURNING ...
        // Codegen projection: switch from `lazuli.Creates(...)` to
        // `lazuli.CreatesWithOwnerCheck(..., OwnerCheckSpec{...})`. The
        // runtime composes the CTE prefix from the spec fields; codegen
        // only emits the carrier.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        resource.fields.push(scope_field("name"));
        feature.resources.push(resource);

        let mut cmd = base_command("create_property");
        cmd.kind = CommandKind::Create;
        cmd.input = CommandInput::Typed(vec![
            typed_slot("host", BuiltinType::Id, true),
            typed_slot("name", BuiltinType::Text, true),
        ]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Property"),
            from_input: false,
            assignments: vec![
                Assignment {
                    field: "host".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "host"])),
                },
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
            ],
        });
        let mut scope = owner_scope_sql_property();
        scope.cte_owner_check = Some(
            "WITH owner_check AS (SELECT 1 FROM \"host\" WHERE id = $host AND \"user\" = ctx.User.ID)"
                .to_owned(),
        );
        cmd.owner_scope_sql = Some(scope);
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Effect: lazuli.CreatesWithOwnerCheck(&propertyResource, lazuli.Bindings{"
            ),
            "CREATE with cte_owner_check should emit CreatesWithOwnerCheck:\n{out}"
        );
        assert!(
            out.contains("lazuli.OwnerCheckSpec{"),
            "OwnerCheckSpec literal must be emitted:\n{out}"
        );
        assert!(
            out.contains("FKColumn:      \"host\","),
            "OwnerCheckSpec.FKColumn must point at the FK field:\n{out}"
        );
        assert!(
            out.contains("RelatedTable:  \"host\","),
            "OwnerCheckSpec.RelatedTable must be the snake-cased FK target:\n{out}"
        );
        assert!(
            out.contains("ThroughColumn: \"user\","),
            "OwnerCheckSpec.ThroughColumn must match the @owner_axis through: value:\n{out}"
        );
        assert!(
            !out.contains("Effect: lazuli.Creates(&propertyResource"),
            "tenant-only Creates form should NOT appear when CTE is active:\n{out}"
        );
    }

    #[test]
    fn create_without_cte_owner_check_emits_regular_creates() {
        // When `owner_scope_sql.cte_owner_check` is None (or the slot
        // itself is None), the CREATE emit falls back to the tenant-only
        // `lazuli.Creates(...)` shape — no CTE wrapper.
        let mut feature = base_feature("billing");
        feature.resources.push(simple_resource("Charge"));

        let mut cmd = base_command("create_charge");
        cmd.input = CommandInput::Typed(vec![typed_slot("amount", BuiltinType::Integer, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Charge"),
            from_input: false,
            assignments: vec![Assignment {
                field: "amount".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "amount"])),
            }],
        });
        cmd.owner_scope_sql = None;
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("Effect: lazuli.Creates(&chargeResource, lazuli.Bindings{"),
            "CREATE without cte_owner_check must use the regular Creates form:\n{out}"
        );
        assert!(
            !out.contains("CreatesWithOwnerCheck"),
            "tenant-only CREATE must NOT use CreatesWithOwnerCheck:\n{out}"
        );
        assert!(
            !out.contains("OwnerCheckSpec"),
            "tenant-only CREATE must NOT emit OwnerCheckSpec:\n{out}"
        );
    }

    #[test]
    fn owner_scope_sql_snake_cases_pascal_fk_target() {
        // The analyzer's `OwnerScopeSql.fk_target` carries PascalCase
        // (`"Host"`, `"BookingProposal"`), matching the IR's resource
        // name shape. Codegen lowers to snake_case when projecting to
        // `FromCtxOwnedVia` so the runtime's `quoteIdent` round-trips
        // with the migrated SQL table name (`booking_proposal`).
        let mut feature = base_feature("operations");
        let mut resource = simple_resource("Transaction");
        resource.fields.push(scope_field("proposal"));
        feature.resources.push(resource);

        let mut cmd = base_command("cancel_transaction");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Transaction"),
            assignments: Vec::new(),
        });
        cmd.owner_scope_sql = Some(lazuli_ir::OwnerScopeSql {
            field_name: "proposal".to_owned(),
            fk_target: "BookingProposal".to_owned(),
            through_column: "user".to_owned(),
            where_predicate:
                "proposal IN (SELECT id FROM \"booking_proposal\" WHERE \"user\" = ctx.User.ID)"
                    .to_owned(),
            cte_owner_check: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "\"proposal\": lazuli.FromCtxOwnedVia(\"booking_proposal\", \"user\", \"user.id\"),"
            ),
            "PascalCase fk_target must be snake-cased in the emitted FromCtxOwnedVia:\n{out}"
        );
    }

    /// `command logout` (no `returns`, `handler @fn.logout`) — the IR
    /// lowers to `CommandEffect::None` with a handler ref. The Go
    /// handler stub is generated as `(struct{}, error)`. The emitted
    /// `ReturnsFromRegistry` Output generic MUST be `struct{}` so the
    /// runtime's type-assert (`fn.(func(*Ctx, I) (O, error))`) matches.
    /// Previously emitted `any`, which failed the assert and 500'd.
    #[test]
    fn none_effect_with_fn_handler_emits_struct_output_generic() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("logout");
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::None;
        cmd.handler = Some(HandlerRef {
            namespace: "fn".to_owned(),
            name: "logout".to_owned(),
            span_ref: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Effect: lazuli.ReturnsFromRegistry[struct{}, struct{}](\"customer.logout\"),"
            ),
            "no-returns + @fn handler should emit O=struct{{}} (matches Go handler stub):\n{out}"
        );
        assert!(
            !out.contains("ReturnsFromRegistry[struct{}, any]"),
            "regression: O=any breaks the runtime type-assert against the registered (struct{{}}, error) handler:\n{out}"
        );
        assert!(
            out.contains(
                "// Wire Logout as `func(ctx *lazuli.Ctx, input struct{}) (struct{}, error)`"
            ),
            "handler signature comment should match the (struct{{}}, error) shape, got:\n{out}"
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
