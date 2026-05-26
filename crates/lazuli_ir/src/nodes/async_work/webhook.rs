//! `Webhook` — inbound HTTP delivery contract.
//!
//! Two verifier shapes coexist:
//!
//! - **Legacy text-pattern** `verify: PathRef` (a file path to a Go
//!   verifier). Untouched for back-compat.
//! - **Canonical-indent typed** [`VerifySpec`] (closed scheme catalog,
//!   env-bound secret, header name). Coexists with the legacy slot — when
//!   present, the typed form takes precedence.
//!
//! Replay ([`ReplaySpec`] / [`ReplayMode`]) declares whether re-delivery
//! is allowed and the dedupe window; DLQ ([`DlqSpec`]) declares where
//! deliveries go after retry exhaustion (emit a tombstone event, hand to
//! a custom handler, or explicitly drop with a logged reason).

use serde::{Deserialize, Serialize};

use crate::{EmitPredicate, Path, PathRef, PolicyExpr, PolicyRef, SpanRef, TranslationKeyRef, TypeRef};

use super::shared::{IdempotencyKey, RetryPolicy, TenantFromSpec};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// Inbound HTTP path: `"/webhooks/stripe/invoice-paid"`.
    pub route: String,
    pub verify: PathRef,
    /// Phase L Tier 3 — structured `verify hmac <alg>` declaration.
    /// `None` for legacy text-pattern webhooks; `Some` when the
    /// canonical-indent parser lifted the structured form. Coexists
    /// with `verify: PathRef` because the legacy path uses a file
    /// reference for verifier bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_verify: Option<VerifySpec>,
    /// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    /// Explicit `scope global` + `reason "..."` escape hatch when the
    /// provider doesn't send a tenant key. Closes the doctor-side gap
    /// surfaced by multi-tenant pilot port (LSP rule at `lazuli_lsp/src/lib.rs:10720`
    /// already detected scope_global; IR was dropping it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_global: Option<WebhookScopeGlobalSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-webhook override for the `policy_denied`
    /// error message. Inbound webhook providers receive the error body
    /// the same way HTTP clients do, so customizing the message helps
    /// the integration author debug from the upstream's logs. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    pub handler: PathRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// B5 framework gap 2 — per-branch emit predicates. Same length
    /// as `emits` when present: `emit_predicates[i]` carries the
    /// `when <predicate>` clause authored on `emits[i]`, or `None`
    /// when the entry has no predicate (flat-list back-compat).
    /// Empty vec means "no predicates anywhere" (legacy fixtures).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emit_predicates: Vec<Option<EmitPredicate>>,
    /// Webhooks expanded cycle — `payload from webhook_events.<name>`
    /// typed envelope reference resolved against
    /// `AppRegistry.webhook_events`. Carried as a structured ref so
    /// doctor and inspect consumers do not have to re-parse the
    /// dotted form. Atrito #2 of the canonical proposal: this is a
    /// typed `WebhookEventRef`, not an opaque string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_from: Option<WebhookEventRef>,
    /// Webhooks expanded cycle — `replay` child declaring an inbound
    /// replay contract. `None` defers to the runtime default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplaySpec>,
    /// Webhooks expanded cycle — `dlq <variant>` child declaring how
    /// the runtime routes deliveries after retry exhaustion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dlq: Option<DlqSpec>,
    /// Webhooks expanded cycle — Atrito #5 of the canonical proposal:
    /// optional retry policy on inbound webhooks, reusing the jobs-side
    /// `RetryPolicy` verbatim. Surface form: `retry <n> backoff
    /// <strategy>`. The shared shape keeps the parser, doctor, and
    /// codegen single-pathed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Webhooks expanded cycle — typed reference to a
/// `registry.webhook_events.<name>` envelope. The `webhook_events.`
/// prefix is implicit (registry path); the language keeps only the
/// final identifier on disk so renames are local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEventRef {
    /// Catalog entry name within `AppRegistry.webhook_events`.
    pub name: String,
}

/// Webhooks expanded cycle — declarative replay contract on an inbound
/// webhook. `Allow` requires `within "<duration>"`; `Deny` rejects any
/// re-delivery whose dedupe key was seen before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySpec {
    pub mode: ReplayMode,
    /// `within "<duration>"` — verbatim duration literal. The runtime
    /// parses it; the language never normalises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
    /// `dedupe by <path>` — optional override for the dedupe key path.
    /// `None` reuses the webhook's `idempotency by ...` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_by: Option<Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// `replay allow within "<duration>"` — re-delivery accepted in
    /// the window; runtime returns 200 without re-running the handler.
    Allow,
    /// `replay deny` — re-delivery always rejected; runtime returns a
    /// 409 with `ErrWebhookReplayDenied`.
    Deny,
}

/// Webhooks expanded cycle — dead-letter routing after retry
/// exhaustion. Closed three-variant catalog; mutual exclusion is baked
/// into the discriminator so the parser fails on duplicate children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DlqSpec {
    /// `dlq emit <event>` — publish a tombstone event onto the bus.
    /// Doctor resolves the event name against the feature's declared
    /// events / `event.trace` set.
    Emit { event: String },
    /// `dlq handler "./path.go"` — adapter-side custom handler.
    Handler { path: PathRef },
    /// `dlq drop reason "..."` — explicit waiver. Mirrors
    /// `verify none reason "..."` for the silent-drop edge.
    Drop { reason: String },
}

/// `scope global` + `reason "..."` declaration on a webhook. IR
/// counterpart of the syntax-side `WebhookScopeGlobal`. Doctor reads
/// this to suppress `WEBHOOK-SCOPE-001` when the webhook explicitly
/// opts out of `tenant_from`. The `reason` text is captured for audit
/// surfaces so operators can see why this webhook escapes the
/// standard tenancy invariant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookScopeGlobalSpec {
    pub reason: String,
}

/// Phase L Tier 3 — structured webhook verification spec. Replaces the
/// legacy `verify: PathRef` for canonical-indent webhooks: the
/// algorithm is closed, the secret is an env binding, and the header is
/// a literal string. Bare `PathRef` `verify` stays in place for the
/// legacy text-pattern path until Tier 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifySpec {
    pub scheme: VerifyScheme,
    pub algorithm: String,
    pub secret_env: String,
    pub header: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyScheme {
    /// `verify hmac <alg>` — the canonical inbound verifier today.
    Hmac,
}
