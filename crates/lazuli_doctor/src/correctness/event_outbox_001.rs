//! EVENT-OUTBOX-001 — payments-class event missing `outbox guaranteed`.
//!
//! EVENT-OUTBOX §3.3 — `outbox guaranteed` opts an event into the
//! transactional outbox: the producing command's pgx tx writes a
//! `lazuli_outbox` row in the same commit as the resource mutation, and
//! the runtime pump dispatches post-commit. Money-flow events
//! (`payments`, `billing`) must not lose deliveries on process crash
//! between commit and dispatch — they should carry the guarantee.
//!
//! This rule warns when a feature whose name contains `payment` or
//! `billing` declares an event that does not author
//! `outbox guaranteed`. Severity is `warning` so the gate stays
//! friendly during BC port; strict-profile may escalate later.
//!
//! Scope: feature-level events authored under `event_group`. Trace
//! events (`event.trace <name>`) are observability and do not enter
//! the reaction graph — they are excluded from this rule.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, OutboxMode};

// output

/// One EVENT-OUTBOX-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub event_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "EVENT-OUTBOX-001";

    /// Render the "payments-class event needs `outbox guaranteed`" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::event_outbox_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     event_name: "payment.captured".into(),
    /// };
    /// assert!(f.message().contains("outbox guaranteed"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Payments-class event `{}` should declare `outbox guaranteed` to ensure \
             transactional delivery.",
            self.event_name
        )
    }
}

// detection

/// `true` when the feature name suggests money-flow semantics. Closed
/// catalog of substrings — extend only with pilot evidence.
fn is_payments_class(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("payment") || lower.contains("billing")
}

/// Run EVENT-OUTBOX-001 against one feature.
///
/// `path` anchors findings. No I/O; the IR is the source of truth.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::event_outbox_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a payments-class feature");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if !is_payments_class(&feature.name) {
        return Vec::new();
    }

    let mut out: Vec<Finding> = Vec::new();

    // (a) feature-level events (legacy lifter populates `feature.events`
    //     directly).
    for event in &feature.events {
        if matches!(event.kind, lazuli_ir::EventKind::Trace) {
            continue;
        }
        if !event.outbox.is_guaranteed() {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                event_name: event.name.clone(),
            });
        }
    }

    // (b) events authored inside an `event_group` block. The IR carries
    //     a parallel `events_outbox` vec by index.
    for group in &feature.event_groups {
        for (i, name) in group.events.iter().enumerate() {
            let mode = group
                .events_outbox
                .get(i)
                .copied()
                .unwrap_or(OutboxMode::None);
            if mode.is_guaranteed() {
                continue;
            }
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                event_name: name.clone(),
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, EventGroup, OutboxMode, Policies};

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn group_with(events: Vec<(&str, OutboxMode)>) -> EventGroup {
        let mut names = Vec::new();
        let mut modes = Vec::new();
        for (name, mode) in events {
            names.push(name.to_owned());
            modes.push(mode);
        }
        EventGroup {
            pattern: "payment_*".to_owned(),
            on_resource: Some("Payment".to_owned()),
            raw_payload: Vec::new(),
            raw_audit: None,
            events: names,
            events_outbox: modes,
            variants: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn non_payments_feature_does_not_fire() {
        let mut feature = empty_feature("customer");
        feature
            .event_groups
            .push(group_with(vec![("payment_captured", OutboxMode::None)]));
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn payments_feature_event_without_guarantee_fires() {
        let mut feature = empty_feature("payments");
        feature
            .event_groups
            .push(group_with(vec![("payment_captured", OutboxMode::None)]));
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].event_name, "payment_captured");
    }

    #[test]
    fn payments_feature_event_with_guarantee_does_not_fire() {
        let mut feature = empty_feature("billing");
        feature.event_groups.push(group_with(vec![
            ("payment_captured", OutboxMode::Guaranteed),
            ("payment_refunded", OutboxMode::Guaranteed),
        ]));
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn mixed_modes_only_unguaranteed_events_fire() {
        let mut feature = empty_feature("payments");
        feature.event_groups.push(group_with(vec![
            ("payment_captured", OutboxMode::Guaranteed),
            ("payment_pending", OutboxMode::None),
        ]));
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].event_name, "payment_pending");
    }
}
