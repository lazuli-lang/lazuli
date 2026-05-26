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

/// Stable diagnostic code emitted with every finding from this rule.
pub const CODE: &str = "POLLER-EXPONENTIAL-NO-CAP-001";

/// One POLLER-EXPONENTIAL-NO-CAP-001 finding — a poller's retry
/// backoff is `exponential` without a `cap`. Without a clamp the
/// schedule grows unbounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller carrying the un-capped exponential backoff.
    pub poller: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding (alias of the
    /// module-level [`CODE`]).
    pub const CODE: &'static str = CODE;

    /// Render the "exponential backoff needs a cap" message, naming
    /// the poller and the canonical `backoff ... cap <d>` syntax.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::exponential_no_cap_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    /// };
    /// assert!(f.message().contains("cap"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' uses exponential backoff without a cap; add 'backoff exponential base <d> cap <d>' to bound retry delay.",
            Self::CODE,
            self.poller,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// retry backoff is `Exponential` with `cap: None`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::exponential_no_cap_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller using uncapped exponential backoff");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
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
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
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
