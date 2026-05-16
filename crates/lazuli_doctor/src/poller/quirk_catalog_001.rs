//! POLLER-QUIRK-CATALOG-MISMATCH-001 — `poller retry_quirk`
//! kind is outside the v0.1 closed catalog.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §3.13.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PollerRetryQuirk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub kind: String,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-QUIRK-CATALOG-MISMATCH-001";

    pub fn message(&self) -> String {
        format!(
            "POLLER-QUIRK-CATALOG-MISMATCH-001: poller '{}' retry_quirk '{}' not in closed catalog. v0.1 supports: gender_flip_once.",
            self.poller, self.kind,
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for poller in &feature.pollers {
        for quirk in &poller.retry_quirks {
            if let Some(kind) = unsupported_kind(quirk) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    poller: poller.name.clone(),
                    kind,
                });
            }
        }
    }

    findings
}

#[allow(unreachable_patterns)]
fn unsupported_kind(quirk: &PollerRetryQuirk) -> Option<String> {
    match quirk {
        PollerRetryQuirk::GenderFlipOnce { .. } => None,
        other => Some(debug_variant_to_snake_case(other)),
    }
}

fn debug_variant_to_snake_case(quirk: &PollerRetryQuirk) -> String {
    let debug = format!("{quirk:?}");
    let variant = debug
        .split(|c: char| c == ' ' || c == '(' || c == '{')
        .next()
        .unwrap_or("unknown");
    let mut out = String::new();

    for (idx, ch) in variant.chars().enumerate() {
        if ch.is_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Policies, Poller,
        PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick,
    };

    fn mk_poller(retry_quirks: Vec<PollerRetryQuirk>) -> Poller {
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
            retry_quirks,
            span_ref: None,
        }
    }

    fn mk_feature(poller: Poller) -> Feature {
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn quiet_for_gender_flip_once() {
        let quirk = PollerRetryQuirk::GenderFlipOnce {
            when: "row.status == \"gender_ambiguous\"".into(),
            counter_field: "gender_retry_count".into(),
            gender_field: "gender".into(),
        };
        let feat = mk_feature(mk_poller(vec![quirk]));

        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_no_quirk() {
        let feat = mk_feature(mk_poller(vec![]));

        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
