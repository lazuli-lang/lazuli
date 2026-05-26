//! POLLER-TICK-TOO-FAST-001 — `poller tick every <duration>` below
//! 5 seconds can hammer the database.
//!
//! Severity: warning / warning.
//! Reference: docs/proposals/poller-vocab.md §10 risk #3.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

const RECOMMENDED_FLOOR_SECONDS: u64 = 5;

/// One POLLER-TICK-TOO-FAST-001 finding — a poller's tick interval is
/// below the recommended floor (5s), risking database hammering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller carrying the too-fast tick.
    pub poller: String,
    /// Verbatim `every` literal authored in `tick every <duration>`.
    pub every: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-TICK-TOO-FAST-001";

    /// Render the "tick interval below floor" message, naming the
    /// poller and the actual interval.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::tick_too_fast_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     every: "100ms".into(),
    /// };
    /// assert!(f.message().contains("100ms"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' tick interval {} < 5s may hammer the database; recommended floor 5s.",
            Self::CODE,
            self.poller,
            self.every,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// `tick.every` parses to fewer than `RECOMMENDED_FLOOR_SECONDS`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::tick_too_fast_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller ticking every 100ms");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .pollers
        .iter()
        .filter_map(|p| {
            let seconds = duration_seconds(&p.tick.every)?;
            (seconds < RECOMMENDED_FLOOR_SECONDS).then(|| Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                poller: p.name.clone(),
                every: p.tick.every.clone(),
            })
        })
        .collect()
}

fn duration_seconds(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let unit_start = raw.find(|c: char| !c.is_ascii_digit())?;
    let (digits, unit) = raw.split_at(unit_start);
    if digits.is_empty() {
        return None;
    }
    let n = digits.parse::<u64>().ok()?;
    match unit {
        "s" => Some(n),
        "m" => n.checked_mul(60),
        "h" => n.checked_mul(60 * 60),
        "d" => n.checked_mul(24 * 60 * 60),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Policies, Poller,
        PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick,
    };

    fn mk_poller(every: &str) -> Poller {
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
                every: every.into(),
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
    fn quiet_when_tick_is_15s() {
        let feat = mk_feature(mk_poller("15s"));
        assert_eq!(check(&feat, Path::new("f.lzi")).len(), 0);
    }

    #[test]
    fn quiet_when_tick_is_5s() {
        let feat = mk_feature(mk_poller("5s"));
        assert_eq!(check(&feat, Path::new("f.lzi")).len(), 0);
    }

    #[test]
    fn warns_when_tick_is_1s() {
        let feat = mk_feature(mk_poller("1s"));
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].message(),
            "POLLER-TICK-TOO-FAST-001: poller 'p' tick interval 1s < 5s may hammer the database; recommended floor 5s."
        );
    }

    #[test]
    fn warns_when_tick_is_2s() {
        let feat = mk_feature(mk_poller("2s"));
        assert_eq!(check(&feat, Path::new("f.lzi")).len(), 1);
    }
}
