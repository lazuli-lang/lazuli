//! Tier 3 aggregator — emits the rows-33-39 family of bucket-cycle
//! diagnostics (jobs, webhooks, notifications, event_group cross-checks).
//!
//! Covers:
//!   * tier3_diagnostics — entry-point fan-out
//!   * tier3_job_diagnostics (JOB-TIMEOUT / JOB-FANOUT-001/002 / ...)
//!   * tier3_webhook_diagnostics (WEBHOOK-* family)
//!   * tier3_notification_diagnostics (NOTIF-CHANNEL / NOTIF-DIGEST / NOTIF-THROTTLE)
//!
//! Helpers (`build_event_payload_index`, `leading_assignment_lhs`)
//! move with the aggregator since they have no other consumers.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::doctor::parsers::{
    catalog_list, format_name_list, is_parseable_duration, is_parseable_size,
    is_valid_notification_duration, parse_notification_duration_seconds, payload_field_list,
};
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Closed catalog for notification channels. `NOTIF-CHANNEL-001` rejects
/// any value not in this list. SPECULATIVE channels (`push`, `sms`)
/// gate on adapter binding evidence; the catalog stays narrow today.
const NOTIFICATION_CHANNEL_CATALOG: &[&str] = &["email", "in_app", "slack", "discord", "webhook"];

pub(crate) fn tier3_diagnostics(
    facts: &[Tier3FeatureFacts],
    registry: Option<&lazuli_ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let webhook_events: BTreeMap<&str, &lazuli_ir::WebhookEvent> = registry
        .map(|r| {
            r.webhook_events
                .iter()
                .map(|e| (e.name.as_str(), e))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    // Notifications expanded bucket cycle — cross-feature event
    // payload index keyed on the qualified `<feature>.<event>` name a
    // `notification.trigger event ...` reference uses. The map carries
    // the union of (event-specific payload fields) and (event_group
    // payload fields inherited via pattern match) so
    // `NOTIF-DIGEST-001`'s `group_by` resolution mirrors what the
    // runtime actually sees on the wire.
    let event_payload_index = build_event_payload_index(facts);

    // Webhooks expanded cycle — track which `webhook_events.<name>`
    // entries are referenced anywhere across the package. Anything
    // unreferenced at the end fires `WEBHOOK-EVENT-001`.
    let mut referenced_envelopes: BTreeSet<&str> = BTreeSet::new();

    for feature in facts {
        // Webhooks expanded cycle — single feature event set used by
        // `WEBHOOK-DLQ-001` (event-name resolution). Pulls from every
        // construct in the feature that can declare or emit an event.
        let mut declared_events: BTreeSet<String> = BTreeSet::new();
        for job in &feature.jobs {
            for e in &job.emits {
                declared_events.insert(e.clone());
            }
        }
        for webhook in &feature.webhooks {
            for e in &webhook.emits {
                declared_events.insert(e.clone());
            }
        }
        for notification in &feature.notifications {
            for e in &notification.emits {
                declared_events.insert(e.clone());
            }
        }
        for group in &feature.event_groups {
            for e in &group.events {
                declared_events.insert(e.clone());
            }
        }

        for job in &feature.jobs {
            tier3_job_diagnostics(feature, job, &mut diagnostics);
        }
        for webhook in &feature.webhooks {
            tier3_webhook_diagnostics(
                feature,
                webhook,
                &webhook_events,
                &declared_events,
                &mut referenced_envelopes,
                &mut diagnostics,
            );
        }
        for notification in &feature.notifications {
            tier3_notification_diagnostics(
                feature,
                notification,
                &event_payload_index,
                &mut diagnostics,
            );
        }
    }

    // WEBHOOK-EVENT-001 — every declared `webhook_events.<X>` envelope
    // must be referenced by at least one `webhook ... payload from`.
    // Dead-letter envelope catalog entries are an authoring smell.
    if let Some(reg) = registry {
        for envelope in &reg.webhook_events {
            if !referenced_envelopes.contains(envelope.name.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    // Without a registry-source line map the diagnostic
                    // points at the package root. The LSP rule still
                    // gives the precise underline on the source line.
                    path: PathBuf::from("registry.lzi"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WEBHOOK-EVENT-001".to_owned(),
                    message: format!(
                        "`registry.webhook_events.{}` is declared but no `webhook ... payload from` references it.",
                        envelope.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn tier3_job_diagnostics(
    feature: &Tier3FeatureFacts,
    job: &lazuli_ir::Job,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .job_lines
        .get(&job.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // JOB-TIMEOUT-001: job declares external calls but no timeout. The
    // `INT-CALL-002` text-pattern check on `ExternalCallFact` covers
    // the same ground today; this rule fires from the IR lift so the
    // diagnostic survives `parse_command` arriving in Tier 4 and the
    // text-pattern fact disappearing.
    if !job.external_calls.is_empty() && job.timeout.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "JOB-TIMEOUT-001".to_owned(),
            message: format!(
                "job `{}` declares external `calls` but no `timeout \"...\"` — external operations require an explicit timeout.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // JOB-FANOUT-001: fanout.axis must match the feature's tenancy axis.
    if let (Some(fanout), Some(axis)) = (&job.fanout, &feature.tenancy_axis) {
        if &fanout.axis != axis {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "JOB-FANOUT-001".to_owned(),
                message: format!(
                    "job `{}` declares `fanout tenants {}` but feature `{}` uses tenancy axis `{}`.",
                    job.name, fanout.axis, feature.feature, axis
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // JOB-FANOUT-002: scheduled job declares fanout but no idempotency
    // key — without a key, fanout re-fire can double-execute on tenants.
    if matches!(job.trigger, lazuli_ir::JobTrigger::Schedule { .. })
        && job.fanout.is_some()
        && job.idempotency.is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "JOB-FANOUT-002".to_owned(),
            message: format!(
                "scheduled job `{}` declares `fanout` but no `idempotency by ...` — re-fires may double-execute per tenant.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

pub(crate) fn tier3_webhook_diagnostics<'a>(
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

pub(crate) fn tier3_notification_diagnostics(
    feature: &Tier3FeatureFacts,
    notification: &lazuli_ir::Notification,
    _event_payload_index: &BTreeMap<String, BTreeSet<String>>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .notification_lines
        .get(&notification.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // NOTIF-CHANNEL-001: every channel literal must be in the closed
    // catalog. Empty channel list is also rejected.
    if notification.channels.is_empty() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "NOTIF-CHANNEL-001".to_owned(),
            message: format!(
                "notification `{}` declares no `channel` — at least one of {} is required.",
                notification.name,
                NOTIFICATION_CHANNEL_CATALOG.join(", ")
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    } else {
        for channel in &notification.channels {
            if !NOTIFICATION_CHANNEL_CATALOG.contains(&channel.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "NOTIF-CHANNEL-001".to_owned(),
                    message: format!(
                        "notification `{}` declares channel `{}` outside the closed catalog ({}).",
                        notification.name,
                        channel,
                        NOTIFICATION_CHANNEL_CATALOG.join(", ")
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // ---------------------------------------------------------------
    // Notifications expanded bucket cycle — six new diagnostics.
    //
    // The shape contracts:
    //   - `digest` aggregates triggers per window per group_by key.
    //   - `throttle` rate-limits per recipient/channel with optional
    //     burst, distinct from scalar `rate_limit`.
    //
    // Each diagnostic anchors at the notification header line; the
    // LSP sub-block hover surfaces the precise child token at edit
    // time.
    // ---------------------------------------------------------------

    if let Some(digest) = notification.digest.as_ref() {
        // NOTIF-DIGEST-001 — `every` must parse as `<N> <unit>` where
        // unit is in the closed catalog (seconds, minutes, hours,
        // days). The catalog mirrors Go's `time.ParseDuration` plus
        // `days` so authors can write "1 day" without dropping into
        // `"24h"`. The doctor reads the literal verbatim; precise
        // numeric ranges are left to the adapter.
        if !is_valid_notification_duration(&digest.every) {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-DIGEST-001".to_owned(),
                message: format!(
                    "notification `{}` declares `digest every \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, digest.every
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // NOTIF-DIGEST-002 — `max_size` must be in (0, 10_000].
        // Above the ceiling, the adapter would buffer arbitrarily
        // many payloads per window; doctor caps it explicitly so the
        // contract states the bound rather than leaving it implicit.
        if let Some(max_size) = digest.max_size {
            if max_size == 0 || max_size > 10_000 {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "NOTIF-DIGEST-002".to_owned(),
                    message: format!(
                        "notification `{}` declares `digest max_size {}` outside the supported range 1..=10000.",
                        notification.name, max_size
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // NOTIF-DIGEST-003 — `template_strategy` is a closed catalog.
        if let Some(strategy) = digest.invalid_template_strategy.as_deref() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-DIGEST-003".to_owned(),
                message: format!(
                    "notification `{}` declares `digest template_strategy {}` outside the closed catalog (merge, append).",
                    notification.name, strategy
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if let Some(throttle) = notification.throttle.as_ref() {
        // NOTIF-THROTTLE-001 — at least one key axis is required.
        if !throttle.per_recipient && !throttle.per_channel {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-001".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle` without `per_recipient` or `per_channel`; at least one throttle axis is required.",
                    notification.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if let Some(max_per_seconds) = parse_notification_duration_seconds(&throttle.max_per) {
            // NOTIF-THROTTLE-002 — `burst` cannot exceed the max-per
            // window expressed in seconds. This keeps an immediate
            // burst from being larger than the configured bucket.
            if let Some(burst) = throttle.burst {
                if u64::from(burst) > max_per_seconds {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "NOTIF-THROTTLE-002".to_owned(),
                        message: format!(
                            "notification `{}` declares `throttle burst {}` greater than `max_per \"{}\"`.",
                            notification.name, burst, throttle.max_per
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        } else {
            // NOTIF-THROTTLE-003 — `max_per` must parse as `<N> <unit>`.
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-003".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle max_per \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, throttle.max_per
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
}

/// Notifications expanded bucket cycle — closed-catalog duration
/// matcher reused by `NOTIF-DIGEST-001` and `NOTIF-THROTTLE-003`.
/// Accepts `<N> <unit>` and `<N><unit>` (Go-style), with units in
/// `{s,sec,secs,second,seconds,m,min,mins,minute,minutes,h,hr,hour,hours,d,day,days}`.
/// The runtime resolves the final string via Go's `time.ParseDuration`;
/// doctor's job is to reject obviously wrong literals at design
/// time so the adapter never sees `"1 month"` or `"forever"`.

/// Notifications expanded bucket cycle — cross-feature event-payload
/// index keyed on `<feature>.<event-name>`. Each entry stores the
/// union of (a) event-specific typed payload fields, (b) `event_group`
/// `raw_payload` lines that apply to the event via the group's glob
/// pattern. Built once per doctor run so `NOTIF-DIGEST-001` is
/// constant-time per notification.
pub(crate) fn build_event_payload_index(
    facts: &[Tier3FeatureFacts],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for feature in facts {
        // (a) Typed events lifted on the feature (legacy flow may
        //     populate `Feature.events` in the future; today the
        //     canonical-indent slice leaves it empty, but the loop is
        //     here so the index stays correct when the legacy lifter
        //     catches up).
        for event in &feature.events {
            let key = format!("{}.{}", feature.feature, event.name);
            let fields: BTreeSet<String> = event.payload.iter().map(|f| f.name.clone()).collect();
            index.entry(key).or_default().extend(fields);
        }

        // (b) Concrete events authored under `event_group <prefix>*`
        //     blocks. The lift stores them as short names; the
        //     qualified event name a notification references is
        //     `<feature>.<prefix><short>`. The payload set is the
        //     union of the group's `payload` block (raw `<name> =
        //     <expr>` lines, plus payload-shaped lines like
        //     `customer_id`).
        for group in &feature.event_groups {
            let prefix = group.pattern.strip_suffix('*').unwrap_or(&group.pattern);
            let mut group_fields: BTreeSet<String> = BTreeSet::new();
            for raw in &group.raw_payload {
                if let Some(name) = leading_assignment_lhs(raw) {
                    group_fields.insert(name.to_owned());
                }
            }
            for short_name in &group.events {
                // Avoid double-prefixing when the author already wrote
                // the full prefixed name (`event customer_archived`
                // instead of `event archived` under `customer_*`).
                let qualified = if short_name.starts_with(prefix) {
                    format!("{}.{}", feature.feature, short_name)
                } else {
                    format!("{}.{}{}", feature.feature, prefix, short_name)
                };
                index
                    .entry(qualified)
                    .or_default()
                    .extend(group_fields.iter().cloned());
            }
        }
    }

    index
}

/// Notifications expanded bucket cycle — extract the LHS of an
/// `<name> = <expr>` assignment captured in `event_group.raw_payload`.
/// Returns the bare field name or `None` if the line is not an
/// assignment (e.g. a deeper `audit ...` or comment leftover).
pub(crate) fn leading_assignment_lhs(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let (lhs, _rest) = trimmed.split_once('=')?;
    let lhs = lhs.trim();
    if lhs.is_empty() || lhs.contains(char::is_whitespace) {
        return None;
    }
    Some(lhs)
}
