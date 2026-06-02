//! Cell E3 — Tier 4 command slot emission (§4.1).
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns every helper that lowers Tier 4 slots on the
//! `lazuli.Command[I, O]` envelope plus the related runtime-axis
//! helpers (`Emits`, `Invalidates`, `Approval`, `RateLimit`):
//!
//! - `emit_emits` / `build_outbox_index` — `Emits: []lazuli.EventEmit{...}`
//!   with EVENT-OUTBOX §3.3 outbox-mode tagging.
//! - `emit_invalidates` — `Invalidates: []string{...}` keyed by the
//!   fully-qualified `<feature>.<query>` registry name.
//! - `emit_tier4_fields` — fans out into `emit_external_calls`,
//!   `emit_retry`, `emit_idempotency`, `emit_deprecation`, plus the
//!   raw `Timeout: "..."` line.
//! - `format_approval` / `approval_then_literal` — `Approval: &lazuli.ApprovalSpec{...}`.
//! - `backoff_literal` — `RetryPolicy.Backoff` enum literal.
//! - `format_deprecation_replacement` — fully-qualified replacement
//!   target for the Deprecation envelope (consumed by api.rs too).
//! - `format_rate_limit_struct` — `RateLimit: lazuli.RateLimit{...}`
//!   with multi-env resolution (RULE-VOCAB-04 read-through render).
//!
//! Proposal: §4.1 of `lazuli-go-runtime.md` (Tier 4 slots) +
//! EVENT-OUTBOX §3.3 (outbox tagging).

use lazuli_ir::{
    ApprovalSpec, ApprovalThen, BackoffStrategy, Command, Deprecation, DeprecationReplacement,
    ExternalCallRef, Feature, IdempotencyKey, InvalidatesSpec, RetryPolicy,
};

use super::super::printer::GoPrinter;
use super::{escape_string, format_args_key, format_path, sorted_arg_strings};

/// Emit `Emits: []lazuli.EventEmit{...}` block. The IR `emits: Vec<String>`
/// only carries event names today; the spike's `from creates` axis is
/// implicit when the surrounding effect is `Creates`. We default the
/// `From` to the matching effect-derived constant.
///
/// EVENT-OUTBOX §3.3 — when `outbox_index` reports `Guaranteed` for an
/// event name, the literal carries `Outbox: lazuli.OutboxGuaranteed` so
/// the runtime command path writes the outbox row in the resource tx.
pub(super) fn emit_emits(
    p: &mut GoPrinter,
    emits: &[String],
    outbox_index: &std::collections::BTreeMap<String, lazuli_ir::OutboxMode>,
) {
    p.line("Emits: []lazuli.EventEmit{");
    p.indent();
    for emit in emits {
        let mode = outbox_index
            .get(emit)
            .copied()
            .unwrap_or(lazuli_ir::OutboxMode::None);
        let suffix = if mode.is_guaranteed() {
            ", Outbox: lazuli.OutboxGuaranteed"
        } else {
            ""
        };
        // Without typed `from <axis>` slots on the IR, we surface the
        // emit with `FromExplicit` (the runtime then requires an
        // explicit Bind block; the user's `let` declarations are
        // expected to land there in a follow-up cell).
        p.line(&format!(
            "{{Name: \"{}\", From: lazuli.FromExplicit{}}},",
            emit, suffix
        ));
    }
    p.dedent();
    p.line("},");
}

/// EVENT-OUTBOX §3.3 — build a lookup of `<event-name> -> OutboxMode`
/// from one feature. Walks `feature.event_groups` (parallel
/// `events`/`events_outbox`) and `feature.events`. Group events are
/// indexed by both the short name and the prefix-qualified full name
/// so command `emits` lines match regardless of authoring shape.
pub(super) fn build_outbox_index(
    feature: &Feature,
) -> std::collections::BTreeMap<String, lazuli_ir::OutboxMode> {
    let mut index: std::collections::BTreeMap<String, lazuli_ir::OutboxMode> =
        std::collections::BTreeMap::new();
    for group in &feature.event_groups {
        let prefix = group.pattern.strip_suffix('*').unwrap_or(&group.pattern);
        for (i, short_name) in group.events.iter().enumerate() {
            let mode = group
                .events_outbox
                .get(i)
                .copied()
                .unwrap_or(lazuli_ir::OutboxMode::None);
            if mode.is_none() {
                continue;
            }
            // Index both the short authored name and the prefix-qualified
            // full name so command `emits` lines match either shape.
            index.insert(short_name.clone(), mode);
            let qualified = if short_name.starts_with(prefix) {
                short_name.clone()
            } else {
                format!("{}{}", prefix, short_name)
            };
            index.insert(qualified, mode);
        }
    }
    for event in &feature.events {
        if event.outbox.is_guaranteed() {
            index.insert(event.name.clone(), event.outbox);
        }
    }
    index
}

/// Emit `Invalidates: []string{...}` block. Source is the IR
/// `Vec<InvalidatesSpec>`; we render each as the fully-qualified
/// `<feature>.<name>` wire-registry key Lazuli Go lib expects.
///
/// Cell B1 (codegen-correctness-cycle-2026-05-21) dropped the historical
/// `.query.` infix because the `/q/` HTTP prefix already disambiguates
/// query vs command at the route layer. The registry key the runtime
/// cache matches against is now `<feature>.<query_name>` (see
/// `query.rs` and `runtime.rs` emitters), so invalidates entries must
/// agree byte-for-byte.
///
/// Same-feature shorthand handling: `lower_qualified_name` in the
/// analyzer splits the authored `query.list` form into
/// `feature=Some("query"), name="list"` (legacy pseudo-feature) — and
/// in newer paths the `feature` slot may be `None`. Both cases are
/// resolved here by substituting the host feature name, so the rendered
/// wire key is always `<host_feature>.<name>`.
pub(super) fn emit_invalidates(p: &mut GoPrinter, specs: &[InvalidatesSpec], host_feature: &str) {
    let mut entries: Vec<String> = Vec::with_capacity(specs.len());
    for spec in specs {
        let qname = &spec.query;
        let feature = match qname.feature.as_deref() {
            // Pseudo-feature `query.<name>` — same-feature short form
            // surfaced by `lower_qualified_name` (analyzer doesn't
            // peel off the `query.` keyword prefix today).
            Some("query") | None => host_feature,
            Some(feat) => feat,
        };
        let qualified = format!("{}.{}", feature, qname.name);
        entries.push(format!("\"{}\"", qualified));
    }
    p.line(&format!("Invalidates: []string{{{}}},", entries.join(", ")));
}

pub(super) fn format_approval(approval: &ApprovalSpec) -> String {
    // W4 GAP-06 — emit the ordered approver chain + `sequential` flag. The
    // single-approver form lifts to a 1-element chain (`by == chain[0]`).
    let chain = approval.approvers();
    let chain_lit = chain
        .iter()
        .map(|a| format!("\"{}\"", escape_string(a)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "&lazuli.ApprovalSpec{{Then: \"{}\", By: \"{}\", Reason: \"{}\", Chain: []string{{{}}}, Sequential: {}}},",
        approval_then_literal(approval.then),
        escape_string(&approval.by),
        escape_string(approval.required_when.as_deref().unwrap_or("")),
        chain_lit,
        approval.sequential,
    )
}

pub(super) fn emit_tier4_fields(p: &mut GoPrinter, feature: &Feature, command: &Command) {
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

pub(in crate::emitter) fn format_deprecation_replacement(
    feature: &str,
    replacement: Option<&DeprecationReplacement>,
) -> String {
    match replacement {
        Some(DeprecationReplacement::LocalCommand(name)) => {
            format!("{feature}.command.{name}")
        }
        Some(DeprecationReplacement::LocalApi(name)) => {
            format!("{feature}.api.{name}")
        }
        Some(DeprecationReplacement::Qualified(qname)) => format!(
            "{}.command.{}",
            qname.feature.as_deref().unwrap_or(feature),
            qname.name
        ),
        Some(DeprecationReplacement::QualifiedApi(qname)) => format!(
            "{}.api.{}",
            qname.feature.as_deref().unwrap_or(feature),
            qname.name
        ),
        Some(DeprecationReplacement::Url(url)) => url.clone(),
        None => String::new(),
    }
}

/// RULE-VOCAB-04 — render a `lazuli.RateLimit{...}` struct literal.
/// When `by_env` is empty, emit the compact form (`{Default: "X"}` or
/// `{}`). Otherwise emit a multi-line struct so authors reading
/// `*.gen.go` see the full env-resolution table.
///
/// The runtime's `Resolve()` resolves the active limit at request time
/// against `LAZULI_ENV`; empty Default + empty ByEnv == no throttle.
pub(in crate::emitter) fn format_rate_limit_struct(
    rate_limit: &lazuli_ir::RateLimitSpec,
    continuation_indent: &str,
) -> String {
    if rate_limit.by_env.is_empty() {
        // Compact one-liner — matches the legacy single-`rate_limit "X"`
        // path so backward-compat fixtures emit a stable single line.
        if rate_limit.default.is_empty() {
            return "lazuli.RateLimit{}".to_owned();
        }
        return format!(
            "lazuli.RateLimit{{Default: \"{}\"}}",
            escape_string(&rate_limit.default)
        );
    }
    // Multi-line struct literal: the IR carries env-qualified entries.
    // Per RULE-VOCAB-04 the emission is read-through: each by_env entry
    // appears verbatim so authors reading `*.gen.go` see the full
    // resolution table.
    //
    // Layout (with continuation_indent = "\t"):
    //   RateLimit: lazuli.RateLimit{
    //   \t\tDefault: "X",
    //   \t\tByEnv: []lazuli.RateLimitByEnv{
    //   \t\t\t{Envs: []string{"dev"}, Limit: "..."},
    //   \t\t},
    //   \t},
    let mut out = String::new();
    out.push_str("lazuli.RateLimit{\n");
    out.push_str(continuation_indent);
    out.push_str("\tDefault: \"");
    out.push_str(&escape_string(&rate_limit.default));
    out.push_str("\",\n");
    out.push_str(continuation_indent);
    out.push_str("\tByEnv: []lazuli.RateLimitByEnv{\n");
    for entry in &rate_limit.by_env {
        out.push_str(continuation_indent);
        out.push_str("\t\t{Envs: []string{");
        let envs: Vec<String> = entry
            .envs
            .iter()
            .map(|e| format!("\"{}\"", e.as_str()))
            .collect();
        out.push_str(&envs.join(", "));
        out.push_str("}, Limit: \"");
        out.push_str(&escape_string(&entry.limit));
        out.push_str("\"},\n");
    }
    out.push_str(continuation_indent);
    out.push_str("\t},\n");
    out.push_str(continuation_indent);
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    //! Tier-4 field-emission tests — exercise the runtime struct-field
    //! emission for `approval`, `external_calls`, `timeout`, `retry`,
    //! `idempotency`, `deprecated`. Lifted out of `file_emit.rs` (wave
    //! R8-2b) so the tier-4 concern owns its own behavioural tests.
    //! The `rate_limit` literal sub-cluster moved to the sibling
    //! `rate_limit_emit_tests.rs` (wave R8-2c) to keep this file under
    //! the ≤500-LOC gold standard.
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit, local_qname,
    };
    use lazuli_ir::{
        BackoffStrategy, CommandEffect, CreateEffect, DeprecationReplacement, IdempotencyKey, Path,
        RetryPolicy, UpdateEffect,
    };

    #[test]
    fn tier4_fields_emit_runtime_struct_fields() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("reassign");
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Customer"),
            assignments: Vec::new(),
            where_clause: Vec::new(),
        });
        cmd.approval = Some(lazuli_ir::ApprovalSpec {
            required_when: Some("target.tier = enterprise".to_owned()),
            by: "@role.admin".to_owned(),
            chain: vec!["@role.admin".to_owned()],
            sequential: false,
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
            "Approval: &lazuli.ApprovalSpec{Then: \"deny\", By: \"@role.admin\", Reason: \"target.tier = enterprise\", Chain: []string{\"@role.admin\"}, Sequential: false},"
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
}
