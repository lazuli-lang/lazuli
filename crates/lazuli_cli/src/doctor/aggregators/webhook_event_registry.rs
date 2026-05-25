//! Webhook-event registry aggregator.
//!
//! Cross-checks every `webhook_event <name>` declaration in
//! `registry.lzi` against three contract invariants. None of the rules
//! need feature scope or IR lift — the registry alone is enough.
//!
//! - `webhook-event-version-decreasing` (error) — `previous_version`
//!   strictly greater than `version` reverses time.
//! - `webhook-event-payload-empty` (error) — outbound schemas must be
//!   explicit; an empty payload is a contract gap.
//! - `webhook-event-deprecated-no-replacement` (warning) — deprecated
//!   events should document the successor (`previous_version <n>` or
//!   inline note).

use crate::doctor::{DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity};

pub(crate) fn diagnostics(registry: Option<&DoctorAppRegistry>) -> Vec<DoctorDiagnostic> {
    let Some(registry) = registry else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();

    for event in &registry.manifest.webhook_events {
        if let Some(previous_version) = event.previous_version {
            if previous_version > event.version {
                diagnostics.push(DoctorDiagnostic {
                    path: registry.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "webhook-event-version-decreasing".to_owned(),
                    message: format!(
                        "`webhook_event {}` declares `previous_version {}` greater than current `version {}`.",
                        event.name, previous_version, event.version
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        if event.payload.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: registry.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "webhook-event-payload-empty".to_owned(),
                message: format!(
                    "`webhook_event {}` declares no payload fields; outbound event schemas must be explicit.",
                    event.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if event.deprecated && event.previous_version.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: registry.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "webhook-event-deprecated-no-replacement".to_owned(),
                message: format!(
                    "`webhook_event {}` is deprecated but declares no replacement trail; add `previous_version <n>` or document the successor inline.",
                    event.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}
