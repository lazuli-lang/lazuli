//! POLLER-MAX-RETRIES-UNBOUNDED-001 — `poller retry` lacks `max_attempts`
//! or carries a value > 1000 (sanity cap).
//!
//! v0.1: the parser already requires `max_attempts <int>` (so absence is
//! a parse error). This rule fires on the secondary case — value above
//! the sanity cap, or value == 0 (parser allows it because `u32` parses
//! "0", but a 0 cap is a footgun).
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

const SANITY_CAP: u32 = 1000;

/// One POLLER-MAX-RETRIES-UNBOUNDED-001 finding — a poller's
/// `retry max_attempts` is either zero (never resolves) or above the
/// sanity cap (effectively unbounded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller carrying the suspect max-attempts value.
    pub poller: String,
    /// The actual `max_attempts` value (`0` or above the sanity cap).
    pub max_attempts: u32,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-MAX-RETRIES-UNBOUNDED-001";

    /// Render the "max_attempts out of band" message — branches
    /// between the zero footgun and the "above sanity cap" case so the
    /// remediation is tailored.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::max_retries_unbounded_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     max_attempts: 0,
    /// };
    /// assert!(f.message().contains("never resolves"));
    /// ```
    pub fn message(&self) -> String {
        if self.max_attempts == 0 {
            format!(
                "poller `{}` declares `max_attempts 0` — a poller that never resolves is a footgun; pick a real bound (typical: 5..30)",
                self.poller,
            )
        } else {
            format!(
                "poller `{}` declares `max_attempts {}` exceeding the sanity cap ({}). If you really need unbounded polling, redesign as a `job trigger event` reactor.",
                self.poller, self.max_attempts, SANITY_CAP,
            )
        }
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// `retry.max_attempts` is `0` or greater than the sanity cap.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::max_retries_unbounded_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller carrying max_attempts 0");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter(|p| p.retry.max_attempts == 0 || p.retry.max_attempts > SANITY_CAP)
        .map(|p| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            poller: p.name.clone(),
            max_attempts: p.retry.max_attempts,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Poller, PollerBackoff,
        PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick, Policies,
    };

    fn mk_poller(max: u32) -> Poller {
        Poller {
            name: "p".into(),
            source: "Src".into(),
            cursor: PollerCursor {
                next_at_field: "n".into(),
                resolved_at_field: "r".into(),
                attempts_field: "a".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: max,
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
                name: "h".into(),
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
                by: IrPath::from_segments(["row.id"]),
            },
            audit: None,
            emits: vec![],
            retry_quirks: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(p: Poller) -> Feature {
        Feature {
            name: "f".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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
            pollers: vec![p],
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
    fn fires_when_zero() {
        let feat = mk_feature(mk_poller(0));
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("0"));
    }

    #[test]
    fn fires_when_over_cap() {
        let feat = mk_feature(mk_poller(5000));
        assert_eq!(check(&feat, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn quiet_when_reasonable() {
        let feat = mk_feature(mk_poller(30));
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
