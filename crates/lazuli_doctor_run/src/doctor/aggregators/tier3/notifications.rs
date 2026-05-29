//! Tier-3 notification aggregator — NOTIF-CHANNEL-001,
//! NOTIF-DIGEST-001/002/003, NOTIF-THROTTLE-001/002/003.
//!
//! Lifted from the parent `tier3` god-file in the rails-style split.

use std::collections::{BTreeMap, BTreeSet};

use super::NOTIFICATION_CHANNEL_CATALOG;
use crate::doctor::parsers::{is_valid_notification_duration, parse_notification_duration_seconds};
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(super) fn tier3_notification_diagnostics(
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
