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
    ApprovalSpec, ApprovalThen, Assignment, BackoffStrategy, Command, CommandEffect, CommandInput,
    CreateEffect, DeleteEffect, Deprecation, DeprecationReplacement, Expr, ExternalCallRef,
    Feature, IdempotencyKey, InvalidatesSpec, NamedArg, Path, PolicyRef, QualifiedName,
    RetryPolicy, ReturnsEffect, TypedSlot, UpdateEffect,
};
use std::collections::BTreeMap;

use super::cross_feature::CrossFeatureIndex;
use super::error_envelope::{bucket_names_for_external_calls, emit_wrap_helper, sentinel_buckets};
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

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
    imports.add("lazuli.dev/runtime/lazuli");
    if !wrap_buckets.is_empty() {
        imports.add("context");
        imports.add("errors");
        imports.add("lazuli.dev/runtime/lazuli/auth");
    }
    for command in &commands {
        if let CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                register_imports_for_type(&slot.type_ref, &type_ctx, &mut imports);
            }
        }
        if let CommandEffect::Returns(ret) = &command.effect {
            register_imports_for_type(&ret.return_type, &type_ctx, &mut imports);
        }
        // Resource references (Creates/Updates/Deletes) live in the
        // same Go package — no cross-feature import needed beyond what
        // the resource emitter already registers in `resource.gen.go`.
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();
    if !wrap_buckets.is_empty() {
        emit_wrap_helper(&mut p, &wrap_buckets);
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

    // Input struct emission. `CommandInput::Empty` skips the type
    // declaration entirely; the Command value still names a Go struct
    // shape so we surface a `struct{}` synthetic for those.
    let input_type = match &command.input {
        CommandInput::Typed(slots) => {
            let input_struct = command_input_struct_name(&command.name, &resource_pascal);
            emit_input_struct(p, &input_struct, slots, ctx);
            p.blank();
            input_struct
        }
        CommandInput::Short(_) => {
            // Short form is sugar for typed inputs whose types live on
            // the targeted resource fields. The analyzer doesn't yet
            // expand them; until then we emit a synthetic empty input
            // and a TODO comment so the gap surfaces at review time.
            let input_struct = command_input_struct_name(&command.name, &resource_pascal);
            p.line(&format!(
                "// TODO(short-input): command {} declares a short input list;",
                command.name
            ));
            p.line("// expand against the targeted resource fields (proposal §3.2).");
            p.line(&format!("type {input_struct} struct {{}}"));
            p.blank();
            input_struct
        }
        CommandInput::Empty => "struct{}".to_owned(),
    };

    // Output type resolves from the effect. `None` falls back to
    // `struct{}` so the Command[I,O] still parses.
    let output_type = command_output_type(&command.effect, ctx);

    let var_name = command_var_name(&command.name, &resource_pascal);

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
    kv_rows.push(("Policy:".to_owned(), format_policy(&command.policy)));
    if let Some(rate) = &command.rate_limit {
        kv_rows.push((
            "RateLimit:".to_owned(),
            format!("lazuli.RateLimit(\"{}\"),", escape_string(rate)),
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
    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }

    // Effect block — multi-line. Emitted unaligned (independent
    // formatting from the kv block above).
    let let_bindings: BTreeMap<&str, &Expr> = command
        .lets
        .iter()
        .map(|binding| (binding.name.as_str(), &binding.value))
        .collect();
    emit_effect(p, &command.effect, ctx, &let_bindings);

    // Emits block.
    if !command.emits.is_empty() {
        emit_emits(p, &command.emits);
    }

    // Invalidates block.
    if !command.invalidates.is_empty() {
        emit_invalidates(p, &command.invalidates);
    }

    // Tier 4 operational/lifecycle fields. Approval is emitted in the
    // aligned key block above so it stays next to Audit in runtime
    // field order.
    emit_tier4_fields(p, feature, command);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

/// Emit the `type <Name>Input struct` block for a typed input list.
fn emit_input_struct(p: &mut GoPrinter, name: &str, slots: &[TypedSlot], ctx: &TypeCtx<'_>) {
    p.line(&format!("type {name} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> = Vec::with_capacity(slots.len());
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
        let tag = format!("`json:\"{}\"`", json_suffix);
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

/// Emit the Effect literal — picks one of Lazuli Go lib's `Creates`,
/// `Updates`, `Deletes` builders or falls through with a TODO comment
/// for `Returns`/`None`.
fn emit_effect(
    p: &mut GoPrinter,
    effect: &CommandEffect,
    ctx: &TypeCtx<'_>,
    let_bindings: &BTreeMap<&str, &Expr>,
) {
    match effect {
        CommandEffect::Creates(create) => emit_creates_effect(p, create, let_bindings),
        CommandEffect::Updates(update) => emit_updates_effect(p, update, let_bindings),
        CommandEffect::Deletes(delete) => emit_deletes_effect(p, delete),
        CommandEffect::Returns(ret) => {
            // `Returns` doesn't fit the side-effect axis; the runtime
            // would dispatch to a user-authored handler. Lazuli Go lib
            // doesn't yet model this — emit a TODO so the gap is
            // visible without breaking the build.
            let (ty, _import) = types::go_type_for(&ret.return_type, ctx);
            p.line(&format!(
                "// TODO(returns): Command.effect = Returns({ty}) — Lazuli Go lib has no"
            ));
            p.line("// pure-handler Command shape yet (proposal §4.1 / §10.5).");
        }
        CommandEffect::None => {
            p.line("// TODO(effect): command declares no effect; legacy lowering path.");
        }
    }
}

fn emit_creates_effect(
    p: &mut GoPrinter,
    create: &CreateEffect,
    let_bindings: &BTreeMap<&str, &Expr>,
) {
    let resource_var = resource_var_for_qname(&create.resource);
    if create.assignments.is_empty() && create.from_input {
        // `creates Customer from input` — bind every input field by
        // name. The analyzer has not yet materialised the input axis
        // into assignments; surface a TODO so the gap is visible.
        p.line(&format!(
            "Effect: lazuli.Creates(&{resource_var}, lazuli.Bindings{{}}),"
        ));
        p.line(
            "// TODO(from-input): `creates X from input` — expand bindings from typed input slots.",
        );
        return;
    }
    p.line(&format!(
        "Effect: lazuli.Creates(&{resource_var}, lazuli.Bindings{{"
    ));
    p.indent();
    for assignment in &create.assignments {
        p.line(&format_binding_row(assignment, let_bindings));
    }
    p.dedent();
    p.line("}),");
}

fn emit_updates_effect(
    p: &mut GoPrinter,
    update: &UpdateEffect,
    let_bindings: &BTreeMap<&str, &Expr>,
) {
    let resource_var = resource_var_for_qname(&update.resource);
    // Lazuli Go lib `Updates` takes (resource, where, bind). For E3 we
    // assume the implicit `id` route slot drives WHERE; if the IR ever
    // grows a structured `target_by` axis we lift it here.
    p.line(&format!("Effect: lazuli.Updates(&{resource_var},"));
    p.indent();
    p.line("lazuli.Bindings{\"id\": lazuli.FromInput(\"ID\")},");
    p.line("lazuli.Bindings{");
    p.indent();
    for assignment in &update.assignments {
        p.line(&format_binding_row(assignment, let_bindings));
    }
    p.dedent();
    p.line("},");
    p.dedent();
    p.line("),");
}

fn emit_deletes_effect(p: &mut GoPrinter, _delete: &DeleteEffect) {
    let resource_var = resource_var_for_qname(&_delete.resource);
    p.line(&format!(
        "Effect: lazuli.Deletes(&{resource_var}, lazuli.Bindings{{"
    ));
    p.indent();
    p.line("\"id\": lazuli.FromInput(\"ID\"),");
    p.dedent();
    p.line("}),");
}

/// Render one `Bindings` entry from an `Assignment`. Walks the Expr
/// shape to pick the right Lazuli Go lib `From*` constructor.
fn format_binding_row(assignment: &Assignment, let_bindings: &BTreeMap<&str, &Expr>) -> String {
    let column = assignment.field.to_ascii_lowercase();
    let value_repr = format_binding_source(&assignment.value, let_bindings);
    format!("\"{column}\": {value_repr},")
}

fn format_binding_source(expr: &Expr, let_bindings: &BTreeMap<&str, &Expr>) -> String {
    match expr {
        Expr::Path(path) => format_path_source(&path.segments, let_bindings),
        Expr::String(s) => format!("lazuli.FromConst(\"{}\")", escape_string(s)),
        Expr::Integer(n) => format!("lazuli.FromConst({n})"),
        Expr::Boolean(b) => format!("lazuli.FromConst({b})"),
        Expr::Enum(literal) => {
            let qualifier = literal
                .type_name
                .as_ref()
                .map(|q| pascal_case(&q.name))
                .unwrap_or_default();
            if qualifier.is_empty() {
                format!("lazuli.FromConst(\"{}\")", literal.variant)
            } else {
                format!(
                    "lazuli.FromConst({}{})",
                    qualifier,
                    pascal_case(&literal.variant)
                )
            }
        }
        Expr::Nil => "lazuli.FromConst(nil)".to_owned(),
    }
}

/// Classify a `Path` (e.g. `input.name`, `ctx.user`, `route.id`) into
/// the matching Lazuli Go lib source constructor.
fn format_path_source(segments: &[String], let_bindings: &BTreeMap<&str, &Expr>) -> String {
    if let [name] = segments {
        if let Some(target_expr) = let_bindings.get(name.as_str()) {
            return format!(
                "lazuli.FromConst(\"{}\") /* let {} = {} */",
                escape_string(name),
                name,
                format_expr(target_expr)
            );
        }
    }

    let head = segments.first().map(|s| s.as_str()).unwrap_or("");
    let tail = if segments.len() > 1 {
        segments[1..].join(".")
    } else {
        String::new()
    };
    match head {
        "input" => format!("lazuli.FromInput(\"{tail}\")"),
        "ctx" => format!("lazuli.FromCtx(\"{tail}\")"),
        "target" => format!("lazuli.FromTarget(\"{tail}\")"),
        "route" => format!("lazuli.FromInput(\"{tail}\")"),
        _ => {
            // Fallback: surface as a constant string so the output
            // remains Go-valid. Cell I4 will upgrade this to a hard
            // diagnostic for unresolved binding sources.
            format!("lazuli.FromConst(\"{}\")", segments.join("."))
        }
    }
}

/// Emit `Emits: []lazuli.EventEmit{...}` block. The IR `emits: Vec<String>`
/// only carries event names today; the spike's `from creates` axis is
/// implicit when the surrounding effect is `Creates`. We default the
/// `From` to the matching effect-derived constant.
fn emit_emits(p: &mut GoPrinter, emits: &[String]) {
    p.line("Emits: []lazuli.EventEmit{");
    p.indent();
    for emit in emits {
        // Without typed `from <axis>` slots on the IR, we surface the
        // emit with `FromExplicit` (the runtime then requires an
        // explicit Bind block; the user's `let` declarations are
        // expected to land there in a follow-up cell).
        p.line(&format!(
            "{{Name: \"{}\", From: lazuli.FromExplicit}},",
            emit
        ));
    }
    p.dedent();
    p.line("},");
}

/// Emit `Invalidates: []string{...}` block. Source is the IR
/// `Vec<InvalidatesSpec>`; we render each as the fully-qualified
/// `<feature>.query.<name>` form Lazuli Go lib expects.
///
/// `lower_qualified_name` in the analyzer splits the authored
/// `query.list` form into `feature=Some("query"), name="list"` because
/// the parser doesn't yet differentiate the `query.` keyword prefix
/// from a real feature name. We coalesce the two shapes here so the
/// rendered Go literal carries the canonical
/// `<feature>.query.<name>` short form Lazuli Go lib's
/// `Invalidates []string` slot accepts.
fn emit_invalidates(p: &mut GoPrinter, specs: &[InvalidatesSpec]) {
    let mut entries: Vec<String> = Vec::with_capacity(specs.len());
    for spec in specs {
        let qname = &spec.query;
        let qualified = match qname.feature.as_deref() {
            // Pseudo-feature `query.<name>` — same-feature short form
            // surfaced by `lower_qualified_name` (analyzer doesn't
            // peel off the `query.` keyword prefix today).
            Some("query") => format!("query.{}", qname.name),
            Some(feat) => format!("{}.query.{}", feat, qname.name),
            None => format!("query.{}", qname.name),
        };
        entries.push(format!("\"{}\"", qualified));
    }
    p.line(&format!("Invalidates: []string{{{}}},", entries.join(", ")));
}

fn format_approval(approval: &ApprovalSpec) -> String {
    format!(
        "&lazuli.ApprovalSpec{{Then: \"{}\", By: \"{}\", Reason: \"{}\"}},",
        approval_then_literal(approval.then),
        escape_string(&approval.by),
        escape_string(approval.required_when.as_deref().unwrap_or(""))
    )
}

fn emit_tier4_fields(p: &mut GoPrinter, feature: &Feature, command: &Command) {
    if !command.external_calls.is_empty() {
        emit_external_calls(p, &command.external_calls);
    }
    if let Some(timeout) = &command.timeout {
        p.line(&format!("Timeout: \"{}\",", escape_string(timeout)));
    }
    if let Some(retry) = &command.retry {
        emit_retry(p, retry);
    }
    if let Some(idempotency) = &command.idempotency {
        emit_idempotency(p, idempotency);
    }
    if let Some(deprecation) = &command.deprecated {
        emit_deprecation(p, &feature.name, deprecation);
    }
}

fn emit_external_calls(p: &mut GoPrinter, calls: &[ExternalCallRef]) {
    let mut sorted: Vec<&ExternalCallRef> = calls.iter().collect();
    sorted.sort_by(|a, b| {
        a.slot
            .cmp(&b.slot)
            .then_with(|| a.op.cmp(&b.op))
            .then_with(|| format_args_key(&a.args).cmp(&format_args_key(&b.args)))
    });

    p.line("ExternalCalls: []lazuli.ExternalCallRef{");
    p.indent();
    for call in sorted {
        if call.args.is_empty() {
            p.line(&format!(
                "{{Slot: \"{}\", Operation: \"{}\"}},",
                escape_string(&call.slot),
                escape_string(&call.op)
            ));
            continue;
        }

        let args = sorted_arg_strings(&call.args)
            .into_iter()
            .map(|arg| format!("\"{}\"", escape_string(&arg)))
            .collect::<Vec<_>>()
            .join(", ");
        p.line(&format!(
            "{{Slot: \"{}\", Operation: \"{}\", Args: []string{{{}}}}},",
            escape_string(&call.slot),
            escape_string(&call.op),
            args
        ));
    }
    p.dedent();
    p.line("},");
}

fn emit_retry(p: &mut GoPrinter, retry: &RetryPolicy) {
    p.line(&format!(
        "Retry: &lazuli.RetryPolicy{{Count: {}, Backoff: {}}},",
        retry.count,
        backoff_literal(retry.backoff)
    ));
}

fn emit_idempotency(p: &mut GoPrinter, idempotency: &IdempotencyKey) {
    p.line(&format!(
        "Idempotency: &lazuli.IdempotencyKey{{Path: \"{}\"}},",
        escape_string(&format_path(&idempotency.by))
    ));
}

fn emit_deprecation(p: &mut GoPrinter, feature: &str, deprecation: &Deprecation) {
    p.line(&format!(
        "Deprecation: &lazuli.Deprecation{{Since: \"{}\", Replacement: \"{}\", Sunset: \"{}\"}},",
        escape_string(deprecation.since.as_deref().unwrap_or("")),
        escape_string(&format_deprecation_replacement(
            feature,
            deprecation.replacement.as_ref()
        )),
        escape_string(deprecation.sunset.as_deref().unwrap_or(""))
    ));
}

fn approval_then_literal(then: ApprovalThen) -> &'static str {
    match then {
        ApprovalThen::Deny => "deny",
        ApprovalThen::Allow => "allow",
        ApprovalThen::Escalate => "escalate",
    }
}

fn backoff_literal(backoff: BackoffStrategy) -> &'static str {
    match backoff {
        BackoffStrategy::Fixed => "\"fixed\"",
        BackoffStrategy::Exponential => "\"exponential\"",
    }
}

fn format_deprecation_replacement(
    feature: &str,
    replacement: Option<&DeprecationReplacement>,
) -> String {
    match replacement {
        Some(DeprecationReplacement::LocalCommand(name)) => {
            format!("{feature}.command.{name}")
        }
        Some(DeprecationReplacement::Qualified(qname)) => format!(
            "{}.command.{}",
            qname.feature.as_deref().unwrap_or(feature),
            qname.name
        ),
        Some(DeprecationReplacement::Url(url)) => url.clone(),
        None => String::new(),
    }
}

fn format_path(path: &Path) -> String {
    path.segments.join(".")
}

fn format_args_key(args: &[NamedArg]) -> String {
    sorted_arg_strings(args).join("\u{1f}")
}

fn sorted_arg_strings(args: &[NamedArg]) -> Vec<String> {
    let mut out: Vec<String> = args
        .iter()
        .map(|arg| format!("{}={}", arg.name, format_expr(&arg.value)))
        .collect();
    out.sort();
    out
}

fn format_expr(expr: &Expr) -> String {
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
fn resource_var_for_qname(qname: &QualifiedName) -> String {
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
fn command_output_type(effect: &CommandEffect, ctx: &TypeCtx<'_>) -> String {
    match effect {
        CommandEffect::Creates(c) => pascal_case(&c.resource.name),
        CommandEffect::Updates(u) => pascal_case(&u.resource.name),
        CommandEffect::Deletes(d) => pascal_case(&d.resource.name),
        CommandEffect::Returns(r) => {
            let (ty, _import) = types::go_type_for(&r.return_type, ctx);
            ty
        }
        CommandEffect::None => "struct{}".to_owned(),
    }
}

/// Returns the resource pascal name pinned by the command's effect.
/// Used for the input struct naming axis.
fn effect_resource_pascal(effect: &CommandEffect) -> String {
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

/// Render the `Policy:` line. `PolicyRef::Atom` directly carries a
/// single `@role.*` / `@scope.*` / `@actor.*` atom; `Local` and
/// `External` reference a feature's `policies` block (atom-list
/// resolution happens at codegen time once `populate_commands_from_ir`
/// lands the bridge — until then we surface the named reference and
/// let the runtime resolve through the registry, mirroring how the
/// spike's `dist/go/customer/customer.gen.go:56` renders it). `None`
/// emits an empty `Policy{}` value to keep the field site present in
/// the generated file.
fn format_policy(policy: &PolicyRef) -> String {
    match policy {
        PolicyRef::Local(name) => format!(
            "lazuli.Policy{{Name: \"@policy.{}\"}},",
            escape_string(name)
        ),
        PolicyRef::Atom(atom) => {
            // Atom forms split on `.`: `@role.admin` → namespace=role,
            // name=admin. The analyzer strips the leading `@` before
            // landing the IR (`policy.create`, `role.admin`,
            // `scope.same_org`, `actor.system`), but tests construct
            // the IR by hand and may leave the `@` in place — strip
            // defensively.
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            // `policy.<name>` is the feature-local reference
            // `@policy.<name>` — it must resolve through the feature's
            // `policies` block to its atom list before codegen can
            // emit the typed atoms. That resolution doesn't land in
            // the IR yet, so we render the reference as
            // `lazuli.Policy{Name: "@policy.<name>"}` with an empty
            // atom list and let the Lazuli Go lib's registry walk
            // resolve at boot — matching what the spike does for
            // unresolved feature-local policies.
            if let Some(local) = stripped.strip_prefix("policy.") {
                return format!(
                    "lazuli.Policy{{Name: \"@policy.{}\"}},",
                    escape_string(local)
                );
            }
            let mut parts = stripped.splitn(2, '.');
            let ns = parts.next().unwrap_or("");
            let nm = parts.next().unwrap_or("");
            format!(
                "lazuli.Policy{{Name: \"@{stripped}\", Atoms: []lazuli.PolicyAtom{{{{Namespace: \"{ns}\", Name: \"{nm}\"}}}}}},"
            )
        }
        PolicyRef::External { feature, name } => {
            format!("lazuli.Policy{{Name: \"{feature}.policy.{name}\"}},")
        }
        PolicyRef::Unresolved(raw) => format!("lazuli.Policy{{Name: \"{}\"}},", escape_string(raw)),
        PolicyRef::None => "lazuli.Policy{},".to_owned(),
    }
}

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
fn command_input_struct_name(short_name: &str, resource_pascal: &str) -> String {
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
fn command_var_name(short_name: &str, resource_pascal: &str) -> String {
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

fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| c == '_' || c == '-') {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

fn lower_camel(s: &str) -> String {
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
fn escape_string(raw: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BackoffStrategy, BuiltinType, CommandKind, CreateEffect, Defaults,
        DeleteEffect, DeprecationReplacement, EnumLiteral, Feature, IdempotencyKey, LetBinding,
        Module, NamedArg, Path, Policies, QualifiedName, Record, Resource, RetryPolicy, RouteSlot,
        Tenancy, TypeRef, UpdateEffect,
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            locale: None,
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
            features,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
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
        }
    }

    fn typed_slot(name: &str, builtin: BuiltinType, required: bool) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            required,
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
            kind: CommandKind::Create,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            deprecated: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
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
        assert!(out.contains("package customer"));
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
        cmd.rate_limit = Some("30 per hour per ip".to_owned());
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
        assert!(out.contains("`json:\"name\"`"));
        assert!(out.contains("`json:\"email\"`"));
        assert!(out.contains("Email lazuli.Email"));
        // Command value shape.
        assert!(out.contains("var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{"));
        assert!(out.contains("Name:      \"customer.create\","));
        assert!(out.contains("Resource:  &customerResource,"));
        assert!(out.contains("lazuli.PolicyAtom{{Namespace: \"role\", Name: \"admin\"}}"));
        assert!(out.contains("RateLimit: lazuli.RateLimit(\"30 per hour per ip\"),"));
        // Effect block — Creates with two FromInput bindings.
        assert!(out.contains("Effect: lazuli.Creates(&customerResource, lazuli.Bindings{"));
        assert!(out.contains("\"name\": lazuli.FromInput(\"name\"),"));
        assert!(out.contains("\"email\": lazuli.FromInput(\"email\"),"));
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
        assert!(out
            .contains("\"tier\": lazuli.FromConst(\"new_tier\") /* let new_tier = input.tier */,"));
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
        assert!(out.contains("Invalidates: []string{\"query.list\", \"billing.query.ledger\"},"));
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
    fn cross_feature_input_type_emits_qualified_ref_and_import() {
        // Command in `customer` takes `User.ID` from the `org` feature
        // — surfaces an `*org.User` qualified ref + the cross-feature
        // import. Test exercises the same cross-feature pathway the
        // resource emitter already covers.
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

        assert!(out.contains("Owner org.User"));
        assert!(out.contains("\"lazuli/test/org\""));
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

    // The Record import is dragged in for typed-record output binding
    // ergonomics in later cells; keep a smoke-fn so the `Record` import
    // doesn't bit-rot when its emission branch lands.
    #[allow(dead_code)]
    fn _record_compiles(_: Record) {}
    #[allow(dead_code)]
    fn _tenancy_compiles(_: Tenancy) {}
}
