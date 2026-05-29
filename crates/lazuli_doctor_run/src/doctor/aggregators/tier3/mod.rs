//! Tier 3 aggregator — emits the rows-33-39 family of bucket-cycle
//! diagnostics (jobs, webhooks, notifications, event_group cross-checks).
//!
//! Rails-style split. The orchestrator (`tier3_diagnostics`) lives in
//! `mod.rs` because it owns the cross-feature accumulator
//! (`referenced_envelopes` for `WEBHOOK-EVENT-001`) and the once-per-
//! run event-payload index. The per-construct rule emitters live in
//! sibling modules:
//!
//! * [`jobs`]            — `JOB-TIMEOUT-001`, `JOB-FANOUT-001/002`
//! * [`webhooks`]        — `WEBHOOK-SCOPE-001`, `WEBHOOK-PAYLOAD-001/002`,
//!                         `WEBHOOK-REPLAY-001/002`, `WEBHOOK-DLQ-001/002/003`
//! * [`notifications`]   — `NOTIF-CHANNEL-001`, `NOTIF-DIGEST-001/002/003`,
//!                         `NOTIF-THROTTLE-001/002/003`
//! * [`event_payload`]   — `build_event_payload_index`,
//!                         `leading_assignment_lhs`
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4 and
//! re-split in R9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

mod event_payload;
mod jobs;
mod notifications;
mod webhooks;

pub(crate) use event_payload::{build_event_payload_index, leading_assignment_lhs};
use jobs::tier3_job_diagnostics;
use notifications::tier3_notification_diagnostics;
use webhooks::tier3_webhook_diagnostics;

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
