//! POLLER-DUAL-SCHEDULER-001 — same feature declares both a `poller` and
//! a `job trigger schedule` whose handler walks the same source resource.
//!
//! Heuristic: any scheduled job whose handler file path mentions the
//! poller's source resource (snake_case) flags as suspect. Authors can
//! suppress with `# lazuli-allow POLLER-DUAL-SCHEDULER-001` comment
//! (handled by the comment-suppression machinery; this check just emits
//! the finding).
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5, §10.2.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, JobBody, JobTrigger};

/// One POLLER-DUAL-SCHEDULER-001 finding — the same feature declares
/// both a `poller` and a `job trigger schedule` whose handler walks
/// the same source resource. Two schedulers over the same cursor
/// table race on the `attempts` counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the constructs were authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller in the suspected collision pair.
    pub poller: String,
    /// Source resource both the poller and the job touch.
    pub source: String,
    /// Scheduled job suspected of duplicating the poller's clock.
    pub colliding_job: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-DUAL-SCHEDULER-001";

    /// Render the "two clocks over one cursor" message naming the
    /// poller, source resource, and colliding job.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::dual_scheduler_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     source: "Message".into(),
    ///     colliding_job: "send_messages".into(),
    /// };
    /// assert!(f.message().contains("Message"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares both `poller {}` (over `{}`) AND `job {} trigger schedule ...` whose handler touches `{}` — pick one clock. Two schedulers over the same cursor table race on the `attempts` counter.",
            self.feature, self.poller, self.source, self.colliding_job, self.source,
        )
    }
}

/// Walk pollers and scheduled jobs in `feature` and emit a finding
/// for every pair whose handler file path mentions the poller's
/// source resource (snake_case). Heuristic check — authors can
/// suppress with the standard comment-suppression machinery.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::dual_scheduler_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller + scheduled-job over same resource");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for poller in &feature.pollers {
        let needle = pascal_to_snake(&poller.source);
        for job in &feature.jobs {
            if !matches!(job.trigger, JobTrigger::Schedule { .. }) {
                continue;
            }
            let handler_path = match &job.body {
                JobBody::Handler(h) => h.path.path.to_ascii_lowercase(),
                _ => continue,
            };
            if handler_path.contains(&needle) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    poller: poller.name.clone(),
                    source: poller.source.clone(),
                    colliding_job: job.name.clone(),
                });
            }
        }
    }
    findings
}

fn pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Job, JobBody, JobHandler, JobTrigger,
        Path as IrPath, PathRef, Policies, Poller, PollerBackoff, PollerCursor, PollerRetry,
        PollerState, PollerStateKind, PollerTick, QualifiedName,
    };

    fn mk_poller(source: &str) -> Poller {
        Poller {
            name: "v8_consult_resolver".into(),
            source: source.into(),
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

    fn mk_scheduled_job(name: &str, handler_path: &str) -> Job {
        Job {
            name: name.into(),
            trigger: JobTrigger::Schedule {
                cron: "*/30 * * * * *".into(),
            },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: vec![],
            body: JobBody::Handler(JobHandler {
                path: PathRef::authored(handler_path),
                returns: None,
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_event_job(name: &str) -> Job {
        let mut j = mk_scheduled_job(name, "./jobs/x.go");
        j.trigger = JobTrigger::Event {
            event: QualifiedName {
                feature: None,
                name: "e".into(),
            },
        };
        j
    }

    fn mk_feature(pollers: Vec<Poller>, jobs: Vec<Job>) -> Feature {
        Feature {
            name: "f".into(),
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
            jobs,
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers,
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
    fn fires_on_dual_scheduler() {
        let feat = mk_feature(
            vec![mk_poller("V8PendingConsult")],
            vec![mk_scheduled_job(
                "old_loop",
                "./jobs/poll_v8_pending_consult.go",
            )],
        );
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("V8PendingConsult"));
    }

    #[test]
    fn quiet_when_no_overlap() {
        let feat = mk_feature(
            vec![mk_poller("V8PendingConsult")],
            vec![mk_scheduled_job("rotate_keys", "./jobs/rotate_keys.go")],
        );
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_job_not_scheduled() {
        let feat = mk_feature(
            vec![mk_poller("V8PendingConsult")],
            vec![mk_event_job("ev")],
        );
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
