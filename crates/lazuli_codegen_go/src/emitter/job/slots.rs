//! Per-slot field emitters for `jobs.JobContract`.
//!
//! Each helper writes one optional slot into the JobContract literal:
//! `TenantFrom`, `Fanout`, `Idempotency`, `Retry`, `ExternalCalls`,
//! `Emits`, and the PG.C.2 `Prelude: []billing.GateRef{...}` slice
//! that wires per-callable plan gates into the dispatcher.
//!
//! Sort order is set-stable: the external-call walker sorts by
//! (slot, op, sorted args) and the emits walker sorts the emitted
//! event-name list, so two runs over the same IR produce the same
//! Go bytes.

use lazuli_ir::{ExternalCallRef, FanoutSpec, Gate, IdempotencyKey, RetryPolicy, TenantFromSpec};

use crate::emitter::printer::GoPrinter;

use super::format::{
    backoff_const, escape_string, fanout_scope, format_args_key, format_path, sorted_arg_strings,
};

pub(super) fn emit_tenant_from(p: &mut GoPrinter, tenant_from: &TenantFromSpec) {
    p.line("TenantFrom: &jobs.TenantFromSpec{");
    p.indent();
    p.line(&format!(
        "Path: \"{}\",",
        escape_string(&format_path(&tenant_from.path))
    ));
    p.dedent();
    p.line("},");
}

pub(super) fn emit_fanout(p: &mut GoPrinter, fanout: &FanoutSpec) {
    p.line("Fanout: &jobs.FanoutSpec{");
    p.indent();
    p.line(&format!("Scope: \"{}\",", fanout_scope(fanout.scope)));
    p.line(&format!("Axis:  \"{}\",", escape_string(&fanout.axis)));
    p.dedent();
    p.line("},");
}

pub(super) fn emit_idempotency(p: &mut GoPrinter, idempotency: &IdempotencyKey) {
    p.line("Idempotency: &jobs.IdempotencyKeySpec{");
    p.indent();
    p.line(&format!(
        "Path: \"{}\",",
        escape_string(&format_path(&idempotency.by))
    ));
    p.dedent();
    p.line("},");
}

pub(super) fn emit_retry(p: &mut GoPrinter, retry: &RetryPolicy) {
    p.line("Retry: &jobs.RetryPolicy{");
    p.indent();
    p.line(&format!("Count:   {},", retry.count));
    p.line(&format!("Backoff: {},", backoff_const(retry.backoff)));
    p.dedent();
    p.line("},");
}

pub(super) fn emit_external_calls(p: &mut GoPrinter, calls: &[ExternalCallRef]) {
    let mut sorted: Vec<&ExternalCallRef> = calls.iter().collect();
    sorted.sort_by(|a, b| {
        a.slot
            .cmp(&b.slot)
            .then_with(|| a.op.cmp(&b.op))
            .then_with(|| format_args_key(&a.args).cmp(&format_args_key(&b.args)))
    });

    p.line("ExternalCalls: []jobs.ExternalCallRef{");
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

pub(super) fn emit_emits(p: &mut GoPrinter, emits: &[String]) {
    let mut sorted: Vec<&String> = emits.iter().collect();
    sorted.sort();
    let entries = sorted
        .iter()
        .map(|name| format!("\"{}\"", escape_string(name)))
        .collect::<Vec<_>>()
        .join(", ");
    p.line(&format!("Emits: []string{{{entries}}},"));
}

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `jobs.JobContract` value. `DispatchJob` (and the River worker
/// shim) consults the slice via the package-level runner the
/// `billing` package registers at init. Empty slice → no field
/// emitted, preserving byte-equivalent output for ungated jobs.
pub(super) fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
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
