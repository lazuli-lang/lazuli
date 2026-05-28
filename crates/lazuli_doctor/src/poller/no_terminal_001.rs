//! POLLER-NO-TERMINAL-001 — `poller states` list has no `terminal` entry.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PollerStateKind};

/// One POLLER-NO-TERMINAL-001 finding — a poller's `states` block
/// has no entry marked `terminal`, so the runtime can never freeze
/// resolved rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller missing the terminal state.
    pub poller: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-NO-TERMINAL-001";

    /// Render the "poller has no terminal state" message, naming the
    /// poller.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::no_terminal_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    /// };
    /// assert!(f.message().contains("terminal"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "poller `{}` declares no `terminal` state — at least one state must be marked `terminal` so the runtime can freeze resolved rows",
            self.poller,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// `states` block has no `PollerStateKind::Terminal` entry.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::no_terminal_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller lacking a terminal state");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter(|p| {
            !p.states
                .iter()
                .any(|s| matches!(s.kind, PollerStateKind::Terminal))
        })
        .map(|p| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            poller: p.name.clone(),
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

    fn mk_poller(states: Vec<(&str, PollerStateKind)>) -> Poller {
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
                max_attempts: 1,
                backoff: PollerBackoff::Fixed { base: None },
                span_ref: None,
            },
            states: states
                .into_iter()
                .map(|(name, kind)| PollerState {
                    name: name.into(),
                    kind,
                    span_ref: None,
                })
                .collect(),
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
    fn fires_when_no_terminal() {
        let p = mk_poller(vec![
            ("pending", PollerStateKind::Initial),
            ("waiting", PollerStateKind::Intermediate),
        ]);
        let feat = mk_feature(p);
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("terminal"));
    }

    #[test]
    fn quiet_with_terminal() {
        let p = mk_poller(vec![
            ("pending", PollerStateKind::Initial),
            ("resolved", PollerStateKind::Terminal),
        ]);
        let feat = mk_feature(p);
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
