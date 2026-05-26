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

/// `idempotency by <path>` key — names the field whose value the
/// runtime hashes to dedupe re-runs of an async unit. Shared across
/// jobs, webhooks, and notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    /// Path expression: `envelope.id`, `payload.batch_id`, `payload.external_id`.
    pub by: Path,
}

/// `retry <count> backoff <strategy>` clause shared by every async
/// primitive. `count` is the attempt cap; `backoff` selects the
/// rescheduling curve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub count: u32,
    pub backoff: BackoffStrategy,
}

/// Closed catalog of retry backoff curves. `Fixed` reschedules at a
/// constant delay; `Exponential` doubles after every attempt. Runtime
/// adapters own the concrete timing constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// Constant delay between attempts.
    Fixed,
    /// Exponentially-growing delay (doubling).
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

/// Closed catalog of fanout scopes. v0 supports `Tenants` only —
/// one execution per tenant per fire. Extending the catalog requires
/// a proposal.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_strategy_round_trips() {
        let s = serde_json::to_string(&BackoffStrategy::Exponential).unwrap();
        let back: BackoffStrategy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, BackoffStrategy::Exponential);
    }

    #[test]
    fn fanout_scope_round_trips() {
        let s = serde_json::to_string(&FanoutScope::Tenants).unwrap();
        assert_eq!(s, "\"tenants\"");
    }
}
