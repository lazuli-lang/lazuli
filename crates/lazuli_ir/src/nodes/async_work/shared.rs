//! Shared operational backbone for jobs, webhooks, and notifications.
//!
//! These shapes appear identically across all three async-work primitives:
//! the same `retry <n> backoff <strategy>` clause runs the same on a job,
//! a webhook, and a notification; the same `tenant_from payload.<axis>`
//! extractor; the same `idempotency by <path>` key; the same
//! `calls <slot>.<op>` external-call reference. Keeping them in one
//! sibling module makes the shared backbone visible and prevents drift
//! between the three primitives.

use serde::{Deserialize, Serialize};

use crate::{NamedArg, Path, SpanRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    /// Path expression: `envelope.id`, `payload.batch_id`, `payload.external_id`.
    pub by: Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub count: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Fixed,
    Exponential,
}

/// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor used by
/// jobs, webhooks, and notifications. Captures the path verbatim;
/// doctor splits and validates against tenancy axes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantFromSpec {
    /// `payload.org_id`, `envelope.tenant_id`, etc.
    pub path: Path,
}

/// Phase L Tier 3 — `fanout tenants <axis>` scheduled-job fanout
/// directive. `scope` is closed (`Tenants` today); the `axis` carries
/// the partition key the doctor cross-checks against the feature's
/// tenancy axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanoutSpec {
    pub scope: FanoutScope,
    pub axis: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanoutScope {
    /// `fanout tenants <axis>` — one execution per tenant per fire.
    Tenants,
}

/// Phase L Tier 3 — `calls <slot>.<op>` reference surfaced from the
/// job body. The slot is a registry integration name and the op is the
/// adapter method; doctor pairs these against the feature's
/// `integrations` block. `args` carries the named-argument bindings
/// declared on the call site.
///
/// Phase L Tier 4 follow-up — `span_ref` carries the call site's AST
/// span so doctor anchors `INT-CALL-*` diagnostics on the `calls`
/// line directly instead of text-walking the job body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCallRef {
    pub slot: String,
    pub op: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

