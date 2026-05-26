//! Job AST surface — async/queued/scheduled work declared at feature
//! scope.
//!
//! Phase L Tier 3 lineage (`docs/proposals/phase-l-tier-3-job-effect-scope.md`).
//! Authoring shape:
//!
//! ```text
//! job process_customer_import
//!   trigger event customer.import_requested
//!   queue customer_imports
//!   tenant_from payload.tenant_id
//!   fanout tenants tenant
//!   idempotency by payload.import_id
//!   retry 3 backoff exponential
//!   policy @policy.background
//!   timeout "5m"
//!   calls @adapter.crm.fetch_customer(account_id = payload.account_id)
//!   handler "./jobs/process_import.go"
//!   emits customer.imported
//! ```
//!
//! Body grammar (`JobBody`): a job either points at a Go handler
//! (`Handler`) or carries the typed declarative spine (`Declarative`,
//! Phase L Tier 4b — `target`/`let`/`updates|creates|deletes` lifted to
//! `TargetExprDecl` + `LetBindingDecl` + `CommandEffectDecl`). Reactor-
//! style jobs that only `emits` events without a body land on `None` and
//! lower successfully.
//!
//! `JobTrigger` is closed (`event <name>` | `schedule "<cron>"`).
//! `JobRetry` is shared with webhook + notification + tenant migration —
//! that single shape keeps backoff catalog uniform.

use serde::{Deserialize, Serialize};

use super::{CommandEffectDecl, LetBindingDecl, PolicyExprAst, Span, TargetExprDecl};

/// `job <name>` block — async / queued / scheduled work declared at
/// feature scope.
///
/// Phase L Tier 3 lineage. The body is either handler-backed
/// ([`JobBody::Handler`]) or carries the typed declarative spine
/// ([`JobBody::Declarative`]). Reactor-style jobs that only `emits`
/// without a body land on [`JobBody::None`]. See module-level docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// `queue customer_imports` — execution lane for queued workers.
    pub queue: Option<String>,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `fanout tenants <axis>` — scheduled-job fanout directive.
    pub fanout: Option<JobFanout>,
    /// `idempotency by <path>` — path captured verbatim.
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — pair captured directly.
    pub retry: Option<JobRetry>,
    /// `policy @policy.<...>` — captured verbatim for lowering.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `timeout "30s"` — adapter-parsed duration literal.
    pub timeout: Option<String>,
    /// `calls <slot>.<op>` blocks lifted as `ExternalCallRef` shapes.
    pub external_calls: Vec<JobExternalCall>,
    /// Body of the job. Handler-backed bodies fully lower; declarative
    /// bodies stay as raw lines until Tier 4.
    pub body: JobBody,
    /// `emits <event>` lines. Each is one event name (qualified or not).
    pub emits: Vec<String>,
    pub span: Span,
}

/// Closed two-arm catalog for `trigger <kind>` on a [`Job`] /
/// [`Notification`](crate::ast::Notification).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.activated`.
    Event(String),
    /// `trigger schedule "0 2 * * *"`.
    Schedule(String),
}

/// `fanout tenants <axis>` directive on a scheduled [`Job`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFanout {
    /// `tenants` — closed scope catalog today.
    pub scope: String,
    /// `axis` — name of the tenancy axis to fan out over.
    pub axis: String,
}

/// `retry <count> backoff <strategy>` clause shared by jobs / webhooks /
/// notifications / tenant migrations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRetry {
    /// Attempts after the initial failure (max retries, not total tries).
    pub count: u32,
    /// `fixed` or `exponential` — closed strategy catalog today.
    pub backoff: String,
}

/// One `calls <slot>.<op>(args)` external-call reference on a [`Job`] /
/// [`CommandDecl`](crate::ast::CommandDecl).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCall {
    /// Adapter / connector slot name.
    pub slot: String,
    /// Op name on the slot.
    pub op: String,
    /// `arg_name = path.expr` pairs captured verbatim. Tier 4 lifts
    /// the right-hand-side expressions.
    pub args: Vec<JobExternalCallArg>,
    pub span: Span,
}

/// One named-arg pair inside [`JobExternalCall`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCallArg {
    pub name: String,
    /// Right-hand side captured verbatim until Tier 4.
    pub value: String,
    pub span: Span,
}

/// Body of a job. `Handler` is a path reference; `Declarative` is the
/// typed spine (Phase L Tier 4b lifted; previously a raw-line carve-out
/// in `JobDeclarativeRaw`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarativeTyped),
    /// No `handler` and no `target` / `updates` / `creates` / `deletes`
    /// authored. Some fixture jobs ship only `emits` (event reactors
    /// with no declarative body); analyzer treats this as a parse error
    /// only when neither effect nor emits is declared.
    None,
}

/// Handler-backed body for a [`Job`] or [`CommandDecl`](crate::ast::CommandDecl) —
/// the Go file the runtime invokes, plus an optional return-type pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    /// `"./jobs/process_import.go"` — quotes stripped.
    pub path: String,
    /// Optional `returns <Type>` suffix.
    pub returns: Option<String>,
}

/// Phase L Tier 4b — declarative job body using the typed spine helpers
/// (`TargetExprDecl`, `LetBindingDecl`, `CommandEffectDecl`). Replaces
/// the Tier 3 `JobDeclarativeRaw` carve-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarativeTyped {
    pub target: Option<TargetExprDecl>,
    pub lets: Vec<LetBindingDecl>,
    pub effect: Option<CommandEffectDecl>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_trigger_event_serde_tagged() {
        let t = JobTrigger::Event("customer.activated".into());
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["kind"], "Event");
        assert_eq!(v["value"], "customer.activated");
    }

    #[test]
    fn job_body_none_variant_roundtrips() {
        let b = JobBody::None;
        let s = serde_json::to_string(&b).unwrap();
        let back: JobBody = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, JobBody::None));
    }

    #[test]
    fn job_retry_count_and_backoff_preserved() {
        let r = JobRetry {
            count: 5,
            backoff: "exponential".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: JobRetry = serde_json::from_str(&s).unwrap();
        assert_eq!(back.count, 5);
        assert_eq!(back.backoff, "exponential");
    }
}
