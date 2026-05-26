//! Inbound `webhook <name>` AST surface.
//!
//! Authoring shape:
//!
//! ```text
//! webhook stripe_payment
//!   path "/webhooks/stripe"
//!   verify hmac sha256
//!     secret env.STRIPE_WEBHOOK_SECRET
//!     header "Stripe-Signature"
//!   tenant_from payload.account_id
//!   idempotency by event.id
//!   policy @policy.webhook
//!   handler "./webhooks/stripe.go"
//!   emits payment_received when payload.event == "charge.succeeded"
//!   payload from webhook_events.stripe
//!   replay allow within "24h"
//!   dlq emit payment_failed
//!   retry 3 backoff exponential
//! ```
//!
//! Two structural nuances:
//!
//! 1. **Branch dispatch.** `emits ... when <predicate>` lifts into
//!    parallel arrays (`emits`, `emits_predicates`). When every
//!    predicate is `None`, the codegen falls back to the legacy flat
//!    emit. Otherwise it wires a dispatch table on the generated
//!    contract.
//! 2. **`scope global`.** WAR-VOCAB-WEBHOOK-01 closure: when the
//!    provider doesn't ship a tenant key, the webhook declares
//!    `scope global reason "..."` to escape the
//!    `tenant_from payload.<axis>_id` invariant. The reason is audited.
//!
//! Retry reuses `JobRetry` (single source of truth for backoff).

use serde::{Deserialize, Serialize};

use super::{JobRetry, PolicyExprAst, Span};

/// `webhook <name>` block — inbound webhook surface.
///
/// Ties together verification, tenant resolution, idempotency, branch
/// dispatch (`emits ... when ...`), replay, DLQ and retry. Reuses
/// `JobRetry` for backoff. See module-level docs for the authoring shape
/// and the two structural nuances (branch dispatch + `scope global`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// `path "/webhooks/..."` — raw HTTP route literal.
    pub route: String,
    /// `verify hmac sha256` + nested `secret`/`header`. Required.
    pub verify: WebhookVerify,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `scope global` declaration — set when the provider doesn't send
    /// a tenant key and the handler reconciles the tenant from another
    /// source (e.g. external_reference lookup). Closes WAR-VOCAB-WEBHOOK-01.
    /// Requires a paired `reason` line so the operator-of-record can
    /// audit why this webhook escapes the standard tenant-from invariant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_global: Option<WebhookScopeGlobal>,
    /// `idempotency by <path>` — captured verbatim.
    pub idempotency_by: Option<String>,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `handler "./..."` — required for canonical webhooks today.
    pub handler: Option<WebhookHandler>,
    /// `emits <event>` lines — flat list of event names (back-compat
    /// shape). Populated even when per-branch predicates are authored
    /// so the existing doctor / cross-feature pipeline stays oblivious.
    pub emits: Vec<String>,
    /// B5 framework gap 2 — per-branch `emits ... when <predicate>`
    /// bindings. Parallel to `emits`: `emits_predicates[i]` is the
    /// `when` predicate authored on `emits[i]` (or `None` when no
    /// predicate was authored). When every entry is `None` the
    /// surface is unchanged from the flat shape; when any predicate
    /// is present the codegen wires a dispatch table on the
    /// generated webhook contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits_predicates: Vec<Option<String>>,
    /// Webhooks expanded cycle — `payload from webhook_events.<name>`
    /// (verbatim suffix after `webhook_events.`). `None` when the
    /// inbound webhook does not declare a typed envelope yet.
    pub payload_from: Option<String>,
    /// Webhooks expanded cycle — `replay` block (short or long form).
    pub replay: Option<WebhookReplay>,
    /// Webhooks expanded cycle — `dlq` block (three closed variants).
    pub dlq: Option<WebhookDlq>,
    /// Webhooks expanded cycle — `retry <count> backoff <strategy>`
    /// inbound retry policy. Reuses the jobs-side `JobRetry` shape so
    /// codegen and doctor diagnostics stay single-pathed (Atrito #5
    /// of the canonical proposal).
    pub retry: Option<JobRetry>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `replay` on an inbound
/// webhook.
///
/// Short form: `replay allow within "24h"` (single line).
/// Long form: a `replay` header with nested `allow`/`deny` + `within
/// "..."` + optional `dedupe by <path>` children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookReplay {
    /// `allow` or `deny` — closed catalog enforced by the parser.
    pub mode: String,
    /// `within "<duration>"` — quoted duration verbatim.
    pub within: Option<String>,
    /// `dedupe by <path>` — path expression captured verbatim. `None`
    /// reuses the webhook's `idempotency by ...` path.
    pub dedupe_by: Option<String>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `dlq` on an inbound
/// webhook. The parser fails if more than one variant is authored on
/// the same webhook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookDlq {
    /// `dlq emit <event>` — publish a tombstone event after retry
    /// exhaustion.
    Emit { event: String, span: Span },
    /// `dlq handler "./path.go"` — adapter-side handler.
    Handler { path: String, span: Span },
    /// `dlq drop reason "..."` — explicit waiver. Mirrors `verify
    /// none reason "..."`.
    Drop { reason: String, span: Span },
}

/// `verify <scheme> <algorithm>` block on a [`Webhook`] — signature
/// scheme + optional secret env binding + header name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookVerify {
    /// `hmac` — closed scheme catalog today.
    pub scheme: String,
    /// `sha256`, etc. — adapter-parsed algorithm token.
    pub algorithm: String,
    /// `secret env.<NAME>` — env binding for the shared secret.
    pub secret_env: Option<String>,
    /// `header "X-..."` — quoted header literal.
    pub header: Option<String>,
    pub span: Span,
}

/// `handler "./path.go" [returns <Type>]` reference on a [`Webhook`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookHandler {
    /// Path literal verbatim (quotes stripped).
    pub path: String,
    /// Optional `returns <Type>` suffix.
    pub returns: Option<String>,
}

/// `scope global` declaration on an inbound webhook (WAR-VOCAB-WEBHOOK-01
/// closure). The webhook is intentionally allowed to escape the
/// standard `tenant_from payload.<axis>_id` invariant because the
/// provider doesn't send a tenant key in the payload. The required
/// `reason` is an authored explanation captured for audit + doctor
/// surfaces so the operator-of-record sees why this exception exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookScopeGlobal {
    /// Quoted reason text (parser strips quotes). MUST be non-empty.
    pub reason: String,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_scope_global_reason_preserved() {
        let s = WebhookScopeGlobal {
            reason: "provider sends no tenant key".into(),
            span: Span::new(0, 0),
        };
        assert!(!s.reason.is_empty());
    }

    #[test]
    fn webhook_verify_optional_fields_default_to_none() {
        let v = WebhookVerify {
            scheme: "hmac".into(),
            algorithm: "sha256".into(),
            secret_env: None,
            header: None,
            span: Span::new(0, 0),
        };
        assert!(v.secret_env.is_none() && v.header.is_none());
    }
}
