//! `JOB-DECLARATIVE-BODY-UNSUPPORTED-001` — flag a `job` whose
//! declarative body the runtime cannot execute (it lowers to a no-op).
//!
//! A `job` may carry either a handler body (`handler "./path"`) or a
//! declarative body (typed `target` + `effect`, mirroring a command).
//! The handler form lowers to a registered, executable Go function. The
//! declarative form, however, has **no runtime contract slot today**:
//! `jobs.JobContract` carries no `body` / `target` / `effect` field, so
//! the Go emitter surfaces a `// TODO(runtime): declarative job bodies
//! are not represented in jobs.JobContract yet` comment and the job
//! registers with nothing executable behind it
//! (`crates/lazuli_codegen_go/src/emitter/job/runtime_gaps.rs`).
//!
//! ## Why this is an error, not a TODO
//!
//! A declarative job body compiles green and registers a job that does
//! **nothing** at runtime — the work the author declared is silently
//! dropped. That is the exact failure mode `CLAUDE.md` inviolable rule 7
//! forbids ("Magic discovery requires visibility … No silent runtime
//! behavior"): a capability the runtime cannot honor must surface in
//! tooling, not compile green. This rule promotes the codegen-level
//! `TODO(runtime)` into a doctor finding so the gap is visible at
//! `lazuli doctor` time, with a clear remedy (rewrite the body as a
//! `handler "./path"` until the runtime grows a declarative job slot).
//!
//! Default severity: `Warning`. Under the `[doctor.error_handling]`
//! `tdd-iron-hand` preset: `Error` (mirrors
//! [`crate::error_handling::handler_no_panic_001`] and the rest of the
//! error-handling family — a silently-dropped job is the job-level twin
//! of a swallowed handler error).
//!
//! ## What fires / what stays silent
//!
//! - Fires once per [`lazuli_ir::Job`] whose [`lazuli_ir::JobBody`] is
//!   [`lazuli_ir::JobBody::Declarative`].
//! - Silent for handler-backed jobs
//!   ([`lazuli_ir::JobBody::Handler`]) — those lower to a registered,
//!   executable Go function.
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::error_handling::job_declarative_body_unsupported_001::Finding;
//!
//! assert_eq!(Finding::CODE, "JOB-DECLARATIVE-BODY-UNSUPPORTED-001");
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::preset`] — `tdd-iron-hand` escalates this
//!   rule to `Error`.
//! - `crates/lazuli_codegen_go/src/emitter/job/runtime_gaps.rs` — the
//!   emitter site that surfaces the `TODO(runtime)` this rule promotes.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, JobBody};

/// One `JOB-DECLARATIVE-BODY-UNSUPPORTED-001` finding: a job declared
/// with a declarative body the runtime has no contract slot to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the job was authored in.
    pub path: PathBuf,
    /// Job name as declared (`job <name> { … }`).
    pub job_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "JOB-DECLARATIVE-BODY-UNSUPPORTED-001";

    /// Render the "declarative job body is not executable" message,
    /// naming the job and pointing at the only remedy available until
    /// the runtime grows a declarative job slot.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::error_handling::job_declarative_body_unsupported_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/billing/billing.lzi"),
    ///     job_name: "reconcile".into(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("reconcile"));
    /// assert!(msg.contains("handler"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "job `{}` has a declarative body (typed `target` + `effect`), but the \
             Lazuli runtime `jobs.JobContract` has no slot to execute it — codegen \
             lowers the body to a no-op, so the job registers but does nothing. \
             Rewrite the body as `handler \"./path\"` (which lowers to an executable \
             Go function) until the runtime grows a declarative job slot. A \
             silently-dropped job must surface here, not compile green.",
            self.job_name,
        )
    }
}

/// Run `JOB-DECLARATIVE-BODY-UNSUPPORTED-001` for one feature.
///
/// Emits one finding per job whose body is
/// [`JobBody::Declarative`]; handler-backed jobs are silent because they
/// lower to a registered, executable Go function.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::error_handling::job_declarative_body_unsupported_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a declarative job");
/// let _ = check(&feature, Path::new("features/billing/billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .jobs
        .iter()
        .filter(|job| matches!(job.body, JobBody::Declarative(_)))
        .map(|job| Finding {
            path: path.to_path_buf(),
            job_name: job.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Finding, check};
    use std::path::Path;

    use lazuli_ir::{
        CommandEffect, CreateEffect, Defaults, Feature, Job, JobBody, JobDeclarative, JobHandler,
        JobTrigger, PathRef, PathSource, Policies, QualifiedName,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn declarative_job(name: &str) -> Job {
        Job {
            name: name.to_owned(),
            trigger: JobTrigger::Schedule {
                cron: "0 2 * * *".into(),
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
            body: JobBody::Declarative(JobDeclarative {
                target: None,
                lets: vec![],
                effect: CommandEffect::Creates(CreateEffect {
                    resource: qn("Invoice"),
                    from_input: false,
                    assignments: vec![],
                }),
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn handler_job(name: &str) -> Job {
        Job {
            body: JobBody::Handler(JobHandler {
                path: PathRef {
                    path: "./jobs/reconcile.go".into(),
                    source: PathSource::Authored,
                },
                returns: None,
            }),
            ..declarative_job(name)
        }
    }

    fn mk_feature(jobs: Vec<Job>) -> Feature {
        Feature {
            name: "billing".into(),
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
            jobs,
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
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
    fn declarative_body_job_fires() {
        let feature = mk_feature(vec![declarative_job("reconcile")]);
        let findings = check(&feature, Path::new("features/billing/billing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].job_name, "reconcile");
        assert_eq!(Finding::CODE, "JOB-DECLARATIVE-BODY-UNSUPPORTED-001");
        assert!(findings[0].message().contains("reconcile"));
        assert!(findings[0].message().contains("no-op"));
    }

    #[test]
    fn handler_body_job_does_not_fire() {
        let feature = mk_feature(vec![handler_job("reconcile")]);

        assert!(
            check(&feature, Path::new("features/billing/billing.lzi")).is_empty(),
            "handler-backed jobs lower to an executable Go function and must stay silent"
        );
    }

    #[test]
    fn mixed_bodies_fire_only_for_declarative() {
        let feature = mk_feature(vec![
            handler_job("send_receipt"),
            declarative_job("reconcile"),
            declarative_job("sweep"),
        ]);
        let findings = check(&feature, Path::new("features/billing/billing.lzi"));

        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.job_name.as_str())
                .collect::<Vec<_>>(),
            vec!["reconcile", "sweep"]
        );
    }

    #[test]
    fn no_jobs_is_silent() {
        let feature = mk_feature(vec![]);
        assert!(check(&feature, Path::new("features/billing/billing.lzi")).is_empty());
    }
}
