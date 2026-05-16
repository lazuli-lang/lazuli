//! POLLER-EXPONENTIAL-NO-CAP-001 — exponential backoff needs an upper cap.
//!
//! Without `cap`, an exponential schedule grows forever; the runtime
//! must clamp somewhere. Doctor warns when `backoff exponential base <d>`
//! is declared without a paired `cap <d>`.
//!
//! Severity: warning / warning.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PollerBackoff};

pub const CODE: &str = "POLLER-EXPONENTIAL-NO-CAP-001";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
}

impl Finding {
    pub const CODE: &'static str = CODE;

    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' uses exponential backoff without a cap; add 'backoff exponential base <d> cap <d>' to bound retry delay.",
            Self::CODE,
            self.poller,
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter(|poller| {
            matches!(
                &poller.retry.backoff,
                PollerBackoff::Exponential { cap: None, .. }
            )
        })
        .map(|poller| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            poller: poller.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Policies, Poller,
        PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick,
    };

    fn mk_poller(backoff: PollerBackoff) -> Poller {
        Poller {
            name: "p".into(),
            source: "Src".into(),
            cursor: PollerCursor {
                next_at_field: "next_check_at".into(),
                resolved_at_field: "resolved_at".into(),
                attempts_field: "attempts".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 30,
                backoff,
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

    fn mk_feature(backoff: PollerBackoff) -> Feature {
        Feature {
            name: "f".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
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
            pollers: vec![mk_poller(backoff)],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn quiet_when_exponential_has_cap() {
        let feat = mk_feature(PollerBackoff::Exponential {
            base: "30s".into(),
            cap: Some("10m".into()),
        });
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_fixed_backoff() {
        let feat = mk_feature(PollerBackoff::Fixed { base: None });
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_linear_backoff() {
        let feat = mk_feature(PollerBackoff::Linear {
            base: "30s".into(),
            cap: None,
        });
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn fires_when_exponential_lacks_cap() {
        let feat = mk_feature(PollerBackoff::Exponential {
            base: "30s".into(),
            cap: None,
        });
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].poller, "p");
        assert_eq!(
            findings[0].message(),
            "POLLER-EXPONENTIAL-NO-CAP-001: poller 'p' uses exponential backoff without a cap; add 'backoff exponential base <d> cap <d>' to bound retry delay."
        );
    }
}
