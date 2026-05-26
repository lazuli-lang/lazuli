//! Webhook contract sub-spec formatters.
//!
//! Each function renders one Lazuli Go expression for a single optional slot
//! on `webhooks.WebhookContract` — `Verify`, `PayloadFrom`, `Replay`, `DLQ`,
//! `Retry`. Lifted from `webhook.rs` to keep the per-slot shapes together
//! and away from the orchestrator walk.

use lazuli_ir::{BackoffStrategy, DlqSpec, ReplayMode, RetryPolicy, VerifyScheme};

use super::format::escape_string;

pub(super) fn format_verify_spec(verify: &lazuli_ir::VerifySpec) -> String {
    let scheme = match verify.scheme {
        VerifyScheme::Hmac => "webhooks.VerifyHmac",
    };
    format!(
        "webhooks.VerifySpec{{Scheme: {scheme}, Algorithm: \"{}\", SecretEnv: \"{}\", Header: \"{}\"}},",
        escape_string(&verify.algorithm),
        escape_string(&verify.secret_env),
        escape_string(&verify.header),
    )
}

pub(super) fn format_payload_from(payload_from: &lazuli_ir::WebhookEventRef) -> String {
    format!(
        "&webhooks.WebhookEventRef{{Name: \"{}\"}},",
        escape_string(&payload_from.name)
    )
}

pub(super) fn format_replay_spec(replay: &lazuli_ir::ReplaySpec) -> String {
    let mut fields = vec![format!("Mode: {}", replay_mode_const(replay.mode))];
    if let Some(window) = &replay.within {
        fields.push(format!("Window: \"{}\"", escape_string(window)));
    }
    format!("&webhooks.ReplaySpec{{{}}},", fields.join(", "))
}

fn replay_mode_const(mode: ReplayMode) -> &'static str {
    match mode {
        ReplayMode::Allow => "webhooks.ReplayAllow",
        ReplayMode::Deny => "webhooks.ReplayDeny",
    }
}

pub(super) fn format_dlq_spec(dlq: &DlqSpec) -> String {
    match dlq {
        DlqSpec::Emit { event } => format!(
            "&webhooks.DlqSpec{{Kind: webhooks.DlqEmit, Topic: \"{}\"}},",
            escape_string(event)
        ),
        DlqSpec::Handler { path } => format!(
            "&webhooks.DlqSpec{{Kind: webhooks.DlqHandler, Handler: \"{}\"}},",
            escape_string(&path.path)
        ),
        DlqSpec::Drop { .. } => "&webhooks.DlqSpec{Kind: webhooks.DlqDrop},".to_owned(),
    }
}

pub(super) fn format_retry_policy(retry: &RetryPolicy) -> String {
    format!(
        "&jobs.RetryPolicy{{Count: {}, Backoff: {}}},",
        retry.count,
        backoff_const(retry.backoff)
    )
}

fn backoff_const(backoff: BackoffStrategy) -> &'static str {
    match backoff {
        BackoffStrategy::Fixed => "jobs.BackoffFixed",
        BackoffStrategy::Exponential => "jobs.BackoffExponential",
    }
}
