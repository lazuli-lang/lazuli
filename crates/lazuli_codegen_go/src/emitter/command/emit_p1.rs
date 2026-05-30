/// Walk a single `Command` — optional Input struct, then the
/// `lazuli.Command[I, O]` value.
pub(super) fn emit_command(
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
    if let Some(audit) = &command.audit {
        // Lazuli Go lib has `AuditDefault` + bespoke `AuditSpec`. The
        // IR carries subject lists + optional `emit_to`, both of which
        // map onto `AuditSpec.Fields`. Until the lib grows the
        // `emit_to` slot we emit the default marker — the captured
        // subjects round-trip through the audit-default behaviour.
        if let Some(materialize) = &audit.materialize {
            // GAP-AUDIT-01 — the audit record is written to a declared
            // append_only OperationLog in addition to the event. We emit
            // a populated `lazuli.AuditSpec` carrying the target table so
            // the runtime's existing audit path (`writeAuditRow` →
            // `writeAuditMaterializeRow`) does a second INSERT of the
            // SAME assembled record, same tx. Wire-thin: no new audit
            // logic, one record two sinks. Table name = snake-cased
            // target resource (matches the migration DDL convention).
            let table = super::scope::command_pascal_to_snake(&materialize.resource);
            kv_rows.push((
                "Audit:".to_owned(),
                format!(
                    "&lazuli.AuditSpec{{MaterializeTable: \"{}\"}},",
                    escape_string(&table)
                ),
            ));
        } else {
            kv_rows.push(("Audit:".to_owned(), "lazuli.AuditDefault,".to_owned()));
        }
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
    // policy-check gate. Matches the the canonical pilot pattern surfaced
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
    if let Some(binding) = owner_scope_binding(command.owner_scope_sql.as_ref())
        && !scope_bindings.iter().any(|b| b.column == binding.column) {
            scope_bindings.push(binding);
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
fn partition_gates(gates: &[Gate]) -> (Vec<&str>, Vec<&str>) {
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
