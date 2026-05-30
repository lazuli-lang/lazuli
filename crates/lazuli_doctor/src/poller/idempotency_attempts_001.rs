//! POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001 — `poller idempotency by`
//! omits `row.attempts`.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §3.10, §10 risk #1.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001 finding — a poller's
/// `idempotency by` segments don't include `row.attempts`. Conditional
/// UPDATE crash-recovery relies on the attempts counter being part of
/// the idempotency key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller missing `row.attempts` from its idempotency key.
    pub poller: String,
    /// Current segments of the `idempotency by` block.
    pub keys: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001";

    /// Render the "idempotency missing row.attempts" message, naming
    /// the poller and listing the current keys.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::idempotency_attempts_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     keys: vec!["row.id".into()],
    /// };
    /// assert!(f.message().contains("row.attempts"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' idempotency keys must include 'row.attempts' for conditional UPDATE crash-recovery; current keys: [{}].",
            Self::CODE,
            self.poller,
            self.keys.join(", "),
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// `idempotency by` segments are non-empty but lack a `row.attempts`
/// (or `attempts`) entry.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::idempotency_attempts_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller idempotency missing row.attempts");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter_map(|poller| {
            let keys = poller.idempotency.by.segments.clone();
            if keys.is_empty() || keys.iter().any(|k| is_attempts_key(k)) {
                return None;
            }

            Some(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                poller: poller.name.clone(),
                keys,
            })
        })
        .collect()
}

fn is_attempts_key(key: &str) -> bool {
    let key = key.trim();
    key == "row.attempts" || key.ends_with(".attempts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Policies, Poller,
        PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick,
    };

    fn mk_poller(keys: Vec<&str>) -> Poller {
        Poller {
            name: "v8_consult_resolver".into(),
            source: "Src".into(),
            cursor: PollerCursor {
                next_at_field: "next_check_at".into(),
                resolved_at_field: "resolved_at".into(),
                attempts_field: "attempts".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 30,
                backoff: PollerBackoff::Fixed { base: None },
                span_ref: None,
            },
            states: vec![PollerState {
                name: "resolved".into(),
                kind: PollerStateKind::Terminal,
                span_ref: None,
            }],
            resolve_handler: HandlerRef {
                namespace: "fn".into(),
                name: "poll_v8".into(),
                span_ref: None,
            },
            terminal_status_field: None,
            terminal_result_field: None,
            tick: PollerTick {
                every: "30s".into(),
                batch: 100,
            },
            tenant_from: None,
            idempotency: IdempotencyKey {
                by: IrPath::from_segments(keys),
            },
            audit: None,
            emits: vec![],
            retry_quirks: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(poller: Poller) -> Feature {
        Feature {
            name: "consults".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![poller],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn quiet_when_attempts_key_present() {
        let feat = mk_feature(mk_poller(vec!["row.id", "row.attempts"]));
        assert!(check(&feat, Path::new("consults.lzi")).is_empty());
    }

    #[test]
    fn fires_when_attempts_key_missing() {
        let feat = mk_feature(mk_poller(vec!["row.id"]));
        let findings = check(&feat, Path::new("consults.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].keys, vec!["row.id"]);
        assert!(findings[0].message().contains("current keys: [row.id]"));
    }

    #[test]
    fn quiet_when_idempotency_keys_empty() {
        let feat = mk_feature(mk_poller(vec![]));
        assert!(check(&feat, Path::new("consults.lzi")).is_empty());
    }
}
