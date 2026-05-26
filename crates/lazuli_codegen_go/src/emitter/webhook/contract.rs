//! Per-webhook `webhooks.WebhookContract{...}` emitter.
//!
//! The orchestrator (`mod.rs`) walks `feature.webhooks` deterministically and
//! delegates each entry to `emit_webhook` here. Gate annotations (PG.C.2)
//! and `effective_policy` resolution live alongside because both are part of
//! the per-webhook shape.

use lazuli_ir::{Feature, Gate, PolicyRef, Webhook};

use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_WEBHOOK_RECEIVER, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::TypeCtx;
use super::emit_bindings::format_emit_bindings;
use super::format::{
    emit_runtime_gaps, escape_string, format_policy_string, format_string_slice, pascal_case,
    path_to_string, return_type_name, write_section_banner,
};
use super::specs::{
    format_dlq_spec, format_payload_from, format_replay_spec, format_retry_policy,
    format_verify_spec,
};

pub(super) fn emit_webhook(
    p: &mut GoPrinter,
    feature: &Feature,
    webhook: &Webhook,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.{}", feature.name, webhook.name);
    // PascalCase so cross-package handlers in `<feature>handlers` can
    // reference the contract via `<feature>gen.<Name>Webhook` (Go exports
    // start with an uppercase first letter).
    let var_name = format!("{}Webhook", pascal_case(&webhook.name));

    write_section_banner(
        p,
        &[
            format!("Webhook: {qualified_name}"),
            format!("  webhook {}", webhook.name),
        ],
    );

    emit_pattern_header(p, PATTERN_WEBHOOK_RECEIVER);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, webhook.span_ref);
    p.line(&format!("var {var_name} = webhooks.WebhookContract{{"));
    p.indent();

    let mut kv_rows: Vec<(String, String)> = vec![
        (
            "Feature:".to_owned(),
            format!("\"{}\",", escape_string(&feature.name)),
        ),
        (
            "Name:".to_owned(),
            format!("\"{}\",", escape_string(&webhook.name)),
        ),
        (
            "Route:".to_owned(),
            format!("\"{}\",", escape_string(&webhook.route)),
        ),
    ];

    if let Some(verify) = &webhook.structured_verify {
        kv_rows.push(("Verify:".to_owned(), format_verify_spec(verify)));
    }
    if let Some(tenant_from) = &webhook.tenant_from {
        kv_rows.push((
            "TenantFrom:".to_owned(),
            format!(
                "&webhooks.TenantFromSpec{{Path: \"{}\"}},",
                escape_string(&path_to_string(&tenant_from.path))
            ),
        ));
    }
    if let Some(idempotency) = &webhook.idempotency {
        kv_rows.push((
            "IdempotencyBy:".to_owned(),
            format!("\"{}\",", escape_string(&path_to_string(&idempotency.by))),
        ));
    }
    if let Some(policy) = effective_policy(feature, webhook).and_then(format_policy_string) {
        kv_rows.push((
            "Policy:".to_owned(),
            format!("\"{}\",", escape_string(&policy)),
        ));
    }
    kv_rows.push((
        "HandlerPath:".to_owned(),
        format!("\"{}\",", escape_string(&webhook.handler.path)),
    ));
    if let Some(return_type) = &webhook.returns {
        kv_rows.push((
            "ReturnsType:".to_owned(),
            format!(
                "\"{}\",",
                escape_string(&return_type_name(return_type, ctx))
            ),
        ));
    }
    if !webhook.emits.is_empty() {
        kv_rows.push(("Emits:".to_owned(), format_string_slice(&webhook.emits)));
    }
    // B5 framework gap 2 — per-branch dispatch. Emitted whenever the
    // DSL authored at least one `when <predicate>` clause; the flat
    // shape (all predicates absent) leaves `EmitBindings` empty so
    // legacy runtime behaviour is unchanged.
    if webhook.emit_predicates.iter().any(|p| p.as_ref().is_some()) {
        kv_rows.push((
            "EmitBindings:".to_owned(),
            format_emit_bindings(&webhook.emits, &webhook.emit_predicates),
        ));
    }
    if let Some(payload_from) = &webhook.payload_from {
        kv_rows.push(("PayloadFrom:".to_owned(), format_payload_from(payload_from)));
    }
    if let Some(replay) = &webhook.replay {
        kv_rows.push(("Replay:".to_owned(), format_replay_spec(replay)));
    }
    if let Some(dlq) = &webhook.dlq {
        kv_rows.push(("DLQ:".to_owned(), format_dlq_spec(dlq)));
    }
    if let Some(retry) = &webhook.retry {
        kv_rows.push(("Retry:".to_owned(), format_retry_policy(retry)));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "webhook", &webhook.name, webhook.span_ref);
    emit_gate_annotations(p, emit_ctx.gates_for("webhook", &webhook.name));

    emit_runtime_gaps(p, webhook);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

fn effective_policy<'a>(feature: &'a Feature, webhook: &'a Webhook) -> Option<&'a PolicyRef> {
    webhook.policy.as_ref().or(feature.defaults.policy.as_ref())
}

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `webhooks.WebhookContract` value. The receiver (`webhooks.Mount`
/// → `handleOne`) consults the slice via the package-level runner
/// the `billing` package registers at init. Empty slice → no field
/// emitted.
fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
    if gates.is_empty() {
        return;
    }
    p.line("Prelude: []billing.GateRef{");
    p.indent();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => {
                p.line(&format!(
                    "{{Kind: billing.GateBehind, Name: {:?}}},",
                    feature
                ));
            }
            Gate::Quota { limit } => {
                p.line(&format!("{{Kind: billing.GateQuota, Name: {:?}}},", limit));
            }
        }
    }
    p.dedent();
    p.line("},");
}
