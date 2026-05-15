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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub max_attempts: u32,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-MAX-RETRIES-UNBOUNDED-001";

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
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            aggregates: vec![],
            previous_names: vec![],
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
