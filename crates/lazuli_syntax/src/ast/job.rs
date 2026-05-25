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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.activated`.
    Event(String),
    /// `trigger schedule "0 2 * * *"`.
    Schedule(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFanout {
    /// `tenants` — closed scope catalog today.
    pub scope: String,
    pub axis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRetry {
    pub count: u32,
    /// `fixed` or `exponential` — closed strategy catalog today.
    pub backoff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCall {
    pub slot: String,
    pub op: String,
    /// `arg_name = path.expr` pairs captured verbatim. Tier 4 lifts
    /// the right-hand-side expressions.
    pub args: Vec<JobExternalCallArg>,
    pub span: Span,
}

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
