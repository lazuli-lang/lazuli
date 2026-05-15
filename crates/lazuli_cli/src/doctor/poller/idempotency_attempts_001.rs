//! POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001 — `poller idempotency by`
//! omits `row.attempts`.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §3.10, §10 risk #1.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub keys: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' idempotency keys must include 'row.attempts' for conditional UPDATE crash-recovery; current keys: [{}].",
            Self::CODE,
            self.poller,
            self.keys.join(", "),
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter_map(|poller| {
            let keys = poller.idempotency.by.segments.clone();
            if keys.is_empty() || keys.iter().any(is_attempts_key) {
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

fn is_attempts_key(key: &String) -> bool {
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
            pollers: vec![poller],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
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
