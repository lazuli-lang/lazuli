//! Tier-3 webhook aggregator — WEBHOOK-SCOPE-001, WEBHOOK-PAYLOAD-001/002,
//! WEBHOOK-REPLAY-001/002, WEBHOOK-DLQ-001/002/003.
//!
//! Lifted from the parent `tier3` god-file in the rails-style split.
//! The cross-feature WEBHOOK-EVENT-001 sweep stays in the parent
//! `mod.rs` because it owns the `referenced_envelopes` accumulator.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(super) fn tier3_webhook_diagnostics<'a>(
    feature: &Tier3FeatureFacts,
    webhook: &'a lazuli_ir::Webhook,
    webhook_events: &BTreeMap<&'a str, &'a lazuli_ir::WebhookEvent>,
    declared_events: &BTreeSet<String>,
    referenced_envelopes: &mut BTreeSet<&'a str>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .webhook_lines
        .get(&webhook.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // WEBHOOK-SCOPE-001: webhook missing `tenant_from` on a multi-tenant
    // app. Pilot-gated until the tenancy axis lifter lands (Tier 4);
    // we fire the diagnostic conservatively when the webhook simply
    // lacks `tenant_from` so the LSP rule keeps its coverage.
    //
    // An explicit `scope global` + `reason "..."` declaration silences
    // the lint — matches the LSP rule at `lazuli_lsp/src/lib.rs:10720`
    // (closes the doctor-side gap surfaced by multi-tenant pilot port).
    if webhook.tenant_from.is_none() && webhook.scope_global.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-SCOPE-001".to_owned(),
            message: format!(
                "webhook `{}` does not declare `tenant_from payload.<axis>_id` or explicit `scope global` with a reason — verify it should be globally scoped.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // Webhooks expanded cycle — `payload from webhook_events.<X>` must
    // resolve to a declared envelope.
    let envelope: Option<&lazuli_ir::WebhookEvent> = match &webhook.payload_from {
        Some(reference) => {
            let resolved = webhook_events.get(reference.name.as_str()).copied();
            if resolved.is_some() {
                referenced_envelopes.insert(reference.name.as_str());
            } else {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WEBHOOK-PAYLOAD-001".to_owned(),
                    message: format!(
                        "webhook `{}` references `webhook_events.{}` but no such envelope is declared in `registry.webhook_events`.",
                        webhook.name, reference.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            resolved
        }
        None => None,
    };

    // WEBHOOK-PAYLOAD-002 — when the envelope is declared, every
    // `tenant_from payload.<axis>_id` must point at a field that
    // actually exists in the envelope. We only check the first
    // segment after `payload.` so structured/nested probes are
    // tolerated until pilot evidence justifies deeper traversal.
    if let (Some(envelope), Some(tenant_from)) = (envelope, webhook.tenant_from.as_ref())
        && tenant_from.path.segments.first().map(String::as_str) == Some("payload")
        && let Some(axis) = tenant_from.path.segments.get(1)
        && !envelope.payload.iter().any(|f| &f.name == axis)
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-PAYLOAD-002".to_owned(),
            message: format!(
                "webhook `{}` uses `tenant_from payload.{}` but envelope `webhook_events.{}` declares no `{}` field — the runtime will fail at decode time.",
                webhook.name, axis, envelope.name, axis
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-REPLAY-001 — `replay allow` must carry `within "..."`.
    if let Some(replay) = webhook.replay.as_ref()
        && matches!(replay.mode, lazuli_ir::ReplayMode::Allow)
        && replay.within.is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-REPLAY-001".to_owned(),
            message: format!(
                "webhook `{}` declares `replay allow` but no `within \"<duration>\"` window — the adapter has no SLA to enforce.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-REPLAY-002 — replay without `idempotency by ...` has no
    // dedupe key.
    if webhook.replay.is_some()
        && webhook.idempotency.is_none()
        && webhook
            .replay
            .as_ref()
            .and_then(|r| r.dedupe_by.as_ref())
            .is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-REPLAY-002".to_owned(),
            message: format!(
                "webhook `{}` declares `replay` but no `idempotency by ...` nor `dedupe by ...` — replay dedupe has no key.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-001 — `dlq emit <X>` must resolve to a declared
    // event in the same feature.
    if let Some(lazuli_ir::DlqSpec::Emit { event }) = webhook.dlq.as_ref()
        && !declared_events.contains(event)
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-DLQ-001".to_owned(),
            message: format!(
                "webhook `{}` `dlq emit {}` references event `{}` that is not declared in feature `{}` (no `emits`, `event_group`, or `event.trace` matches).",
                webhook.name, event, event, feature.feature
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-002 — `dlq drop` requires `reason "..."`. The parser
    // already rejects the empty form; doctor keeps a defensive check
    // in case a future analyzer path lowers the field without a
    // reason.
    if let Some(lazuli_ir::DlqSpec::Drop { reason }) = webhook.dlq.as_ref()
        && reason.trim().is_empty()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-DLQ-002".to_owned(),
            message: format!(
                "webhook `{}` declares `dlq drop` without `reason \"...\"` — silent drops on dead-letter must be explicit waivers.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-003 — `retry` without `dlq` falls through to the
    // adapter default (silent drop on River etc.).
    if webhook.retry.is_some() && webhook.dlq.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-DLQ-003".to_owned(),
            message: format!(
                "webhook `{}` declares `retry` but no `dlq` — after exhaustion the runtime falls back to the adapter default (silent drop on River).",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}
