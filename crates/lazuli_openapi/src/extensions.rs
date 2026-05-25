//! `x-lazuli-*` OpenAPI extension blocks — approval, verify, retry,
//! replay, dlq, plus the small literal-table helpers that keep enum
//! serialisation consistent.
//!
//! All of these emit into the shared `YamlEmitter` so callers in
//! `paths.rs` can interleave them with the per-operation flow.

use lazuli_ir as ir;

use crate::paths::render_path;
use crate::yaml::YamlEmitter;

pub(crate) fn emit_approval(out: &mut YamlEmitter, approval: &ir::ApprovalSpec) {
    out.line("x-lazuli-approval:");
    out.indent();
    out.kv("then", approval_then_literal(approval.then));
    out.kv_quoted("by", &approval.by);
    out.kv_quoted("reason", approval.required_when.as_deref().unwrap_or(""));
    out.dedent();
}

pub(crate) fn emit_verify(out: &mut YamlEmitter, verify: &ir::VerifySpec) {
    out.line("x-lazuli-verify:");
    out.indent();
    out.kv("scheme", verify_scheme_literal(verify.scheme));
    out.kv_quoted("algorithm", &verify.algorithm);
    out.kv_quoted("secret_env", &verify.secret_env);
    out.kv_quoted("header", &verify.header);
    out.dedent();
}

pub(crate) fn emit_retry(out: &mut YamlEmitter, retry: &ir::RetryPolicy) {
    out.line("x-lazuli-retry:");
    out.indent();
    out.kv("count", &retry.count.to_string());
    out.kv("backoff", backoff_literal(retry.backoff));
    out.dedent();
}

pub(crate) fn emit_replay(out: &mut YamlEmitter, replay: &ir::ReplaySpec) {
    out.line("x-lazuli-replay:");
    out.indent();
    out.kv("mode", replay_mode_literal(replay.mode));
    if let Some(within) = &replay.within {
        out.kv_quoted("within", within);
    }
    if let Some(dedupe_by) = &replay.dedupe_by {
        out.kv_quoted("dedupe_by", &render_path(dedupe_by));
    }
    out.dedent();
}

pub(crate) fn emit_dlq(out: &mut YamlEmitter, dlq: &ir::DlqSpec) {
    out.line("x-lazuli-dlq:");
    out.indent();
    match dlq {
        ir::DlqSpec::Emit { event } => {
            out.kv("kind", "emit");
            out.kv_quoted("event", event);
        }
        ir::DlqSpec::Handler { path } => {
            out.kv("kind", "handler");
            out.kv_quoted("path", &path.path);
        }
        ir::DlqSpec::Drop { reason } => {
            out.kv("kind", "drop");
            out.kv_quoted("reason", reason);
        }
    }
    out.dedent();
}

fn approval_then_literal(then: ir::ApprovalThen) -> &'static str {
    match then {
        ir::ApprovalThen::Deny => "deny",
        ir::ApprovalThen::Allow => "allow",
        ir::ApprovalThen::Escalate => "escalate",
    }
}

fn verify_scheme_literal(scheme: ir::VerifyScheme) -> &'static str {
    match scheme {
        ir::VerifyScheme::Hmac => "hmac",
    }
}

fn replay_mode_literal(mode: ir::ReplayMode) -> &'static str {
    match mode {
        ir::ReplayMode::Allow => "allow",
        ir::ReplayMode::Deny => "deny",
    }
}

fn backoff_literal(backoff: ir::BackoffStrategy) -> &'static str {
    match backoff {
        ir::BackoffStrategy::Fixed => "fixed",
        ir::BackoffStrategy::Exponential => "exponential",
    }
}
