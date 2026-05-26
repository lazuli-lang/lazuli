//! `Job` — declarative async work, either handler-backed or declarative.
//!
//! A [`Job`] is **either** handler-backed ([`JobHandler`]) or declaratively
//! bound ([`JobDeclarative`]) — that is enforced by [`JobBody`]'s
//! discriminated enum. The author never sets [`JobOperationalKind`]; the
//! analyzer derives it from `trigger` + `queue`:
//!
//! - `Schedule` → `Scheduled`
//! - event trigger + no queue → `Reactor`
//! - event trigger + queue → `QueuedWorker`

use serde::{Deserialize, Serialize};

use crate::{
    CommandEffect, LetBinding, PathRef, PolicyExpr, PolicyRef, SpanRef, TargetExpr,
    TranslationKeyRef, TypeRef,
};

use super::shared::{ExternalCallRef, FanoutSpec, IdempotencyKey, RetryPolicy, TenantFromSpec};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// Execution lane for queued workers. `None` runs the reactor inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — reserved-slot per-job override for the
    /// `policy_denied` error message. v1 codegen does not consume this
    /// slot (jobs do not reach end users directly); the IR shape exists
    /// so v2 promotion is purely additive. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    /// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor.
    /// Lowered from the canonical-indent slice; doctor cross-checks
    /// the path against the resource tenancy axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    /// Phase L Tier 3 — `fanout tenants <axis>` scheduled-job
    /// declaration. `None` for non-scheduled or single-tenant jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fanout: Option<FanoutSpec>,
    /// Phase L Tier 3 — `timeout "<duration>"`. Adapter-parsed string;
    /// language keeps the literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Phase L Tier 3 — `calls <slot>.<op>` external-call references
    /// surfaced from the job body. Doctor uses these for cross-feature
    /// integration coverage (`INT-CALL-*`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_calls: Vec<ExternalCallRef>,
    pub body: JobBody,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.customer_archived` — feature-qualified or local.
    Event { event: crate::QualifiedName },
    /// `trigger schedule "0 2 * * *"` — cron expression.
    Schedule { cron: String },
}

/// Derived operational kind for inspect output. Authoring never sets this;
/// the analyzer resolves `Schedule` -> Scheduled, event without queue ->
/// Reactor, event with queue -> QueuedWorker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobOperationalKind {
    Scheduled,
    Reactor,
    QueuedWorker,
}

/// A job has exactly one body style. Handler-backed jobs may still declare
/// `emits`; declarative bodies bind a target and apply one write effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarative),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    pub path: PathRef,
    /// `handler "./..." returns Customer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarative {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
}
