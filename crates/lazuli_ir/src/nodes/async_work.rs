//! Async work — jobs, webhooks, and notifications.
//!
//! All three primitives share the same operational backbone (trigger,
//! tenant_from, idempotency, retry, emits, policy) but expose distinct
//! authoring surfaces. Modeling them together in one IR module makes that
//! shared backbone visible: [`RetryPolicy`] is the same shape on a
//! [`Job`], a [`Webhook`], and a [`Notification`]; [`TenantFromSpec`] is
//! reused across all three; [`IdempotencyKey`] travels with every
//! trigger that could fire twice.
//!
//! ## Jobs
//!
//! A [`Job`] is **either** handler-backed ([`JobHandler`]) or
//! declaratively bound ([`JobDeclarative`]) — that is enforced by
//! [`JobBody`]'s discriminated enum. The author never sets
//! [`JobOperationalKind`]; the analyzer derives it from `trigger` + `queue`:
//!
//! - `Schedule` → `Scheduled`
//! - event trigger + no queue → `Reactor`
//! - event trigger + queue → `QueuedWorker`
//!
//! ## Webhooks
//!
//! [`Webhook`] is the inbound side. Two verifier shapes coexist:
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
//!
//! ## Notifications
//!
//! [`Notification`] is the outbound counterpart of webhooks: a multi-
//! channel dispatch keyed on a recipient path. Two structured batching
//! controls are sibling-bucketed to keep their semantics separate from
//! the scalar `rate_limit`:
//!
//! - [`NotificationDigest`] — aggregate N triggers within `every` into one
//!   dispatch per `group_by` key, capped at `max_size`. Template merge
//!   strategy via [`DigestStrategy`] (`merge` collapses payloads;
//!   `append` emits a list).
//! - [`NotificationThrottle`] — per-recipient / per-channel rate limit
//!   with optional immediate `burst`. Keyed on the notification's axes,
//!   not on the caller (which `rate_limit` already covers).
//!
//! ## Why "async_work" and not three modules
//!
//! Splitting jobs/webhooks/notifications would force [`RetryPolicy`],
//! [`IdempotencyKey`], [`TenantFromSpec`], [`JobTrigger`] (notifications
//! reuse this), and [`ExternalCallRef`] into a fourth common module —
//! adding indirection without payoff. The async-work boundary is real:
//! these three primitives differ in *direction* (inbound/outbound/scheduled)
//! but share the *operational discipline*. The module reflects that.
//!
//! ## See also
//!
//! - `docs/proposals/notifications-expanded-bucket-cycle.md` — digest +
//!   throttle design
//! - `docs/proposals/webhooks-expanded-cycle.md` — replay + dlq +
//!   typed `payload_from` design
//! - `docs/proposals/ir-error-messages-vocab.md` §3.3 — per-callable
//!   `policy_when_denied` slot rationale
//! - [`crate::Command`] — shares many of these shapes (retry,
//!   idempotency, external_calls) by intent

use serde::{Deserialize, Serialize};

use crate::{
    CommandEffect, EmitPredicate, LetBinding, NamedArg, Path, PathRef, PolicyExpr, PolicyRef,
    SpanRef, TargetExpr, TranslationKeyRef, TypeRef, is_false,
};

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

/// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor used by
/// jobs, webhooks, and notifications. Captures the path verbatim;
/// doctor splits and validates against tenancy axes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantFromSpec {
    /// `payload.org_id`, `envelope.tenant_id`, etc.
    pub path: Path,
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

/// Phase L Tier 3 — `notification <name>` declarative contract.
///
/// `channel`, `recipient`, `template`, and `trigger` are the
/// notification-specific axes; `tenant_from`, `idempotency`, `retry`,
/// `emits`, and `policy` reuse the same shapes as jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `trigger event <feature>.<event>` or `trigger schedule "<cron>"`.
    pub trigger: JobTrigger,
    /// `channel email, in_app`. Closed catalog enforced by
    /// `NOTIF-CHANNEL-001`: `email`, `in_app`, `sms`, `push`, `slack`,
    /// `discord`, `webhook`.
    pub channels: Vec<String>,
    /// `recipient target.email` — a path captured verbatim. Lowering
    /// keeps the literal so the adapter resolves against the live
    /// payload.
    pub recipient: String,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Notifications expanded bucket cycle — optional `digest` block.
    /// Aggregates triggers into a single dispatch per window per
    /// `group_by` key. Distinct from `rate_limit` (scalar, per-call) —
    /// digest is per-recipient/per-group structured batching. Doctor:
    /// `NOTIF-DIGEST-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` block.
    /// Per-recipient / per-channel structured rate-limit with burst.
    /// Distinct from scalar `rate_limit "N per <window>"` used on
    /// `agent` / `auth password` / `command` / `expose http`; throttle
    /// keys on the notification's recipient/channel axes, not on the
    /// caller. Doctor: `NOTIF-THROTTLE-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<NotificationThrottle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Notifications expanded bucket cycle — `digest` sub-block on
/// `notification`. Aggregates N triggers within `every` into one
/// dispatch per `group_by` value, capped at `max_size`. The
/// `template_strategy` closed catalog (`merge` | `append`) describes
/// how the adapter combines the per-trigger payloads when rendering
/// the digest template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "15 minutes"` / `every "1 hour"` / `every "1 day"`.
    /// Captured verbatim; doctor `NOTIF-DIGEST-001` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub every: String,
    /// `group_by <payload-path>` — typically the recipient axis
    /// (`customer_id`, `target.email`). Optional: when absent, the
    /// digest groups globally per notification kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// `max_size <N>` — hard cap on items per digest. Doctor
    /// `NOTIF-DIGEST-002` rejects `<= 0` or `> 10000`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
    /// `template_strategy merge|append` — closed catalog. None defaults
    /// to `merge` at the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_strategy: Option<DigestStrategy>,
    /// Raw authored `template_strategy` when it was outside the closed
    /// catalog. Kept only so doctor can report `NOTIF-DIGEST-003`
    /// after lowering without widening `DigestStrategy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid_template_strategy: Option<String>,
}

/// Notifications expanded bucket cycle — closed catalog for
/// `digest template_strategy`. `merge` collapses per-trigger payloads
/// into a single object (last-write-wins per key); `append` emits a
/// list the template iterates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestStrategy {
    Merge,
    Append,
}

/// Notifications expanded bucket cycle — `throttle` sub-block on
/// `notification`. Distinct from scalar `rate_limit "N per <window>"`:
/// throttle keys on recipient and/or channel and supports an
/// `immediate burst` before the bucket starts rejecting. The shape is
/// per-recipient / per-channel / per-burst, not per-caller — that is
/// why it does not reuse the `rate_limit` keyword.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "1 hour"` / `max_per "1 day"`. Window over which the
    /// bucket refills. Doctor `NOTIF-THROTTLE-003` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub max_per: String,
    /// `per_recipient` — when set, the throttle bucket is keyed on the
    /// notification's `recipient <path>` value. At least one of
    /// `per_recipient` or `per_channel` is required by doctor
    /// `NOTIF-THROTTLE-001`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_recipient: bool,
    /// `per_channel` — when set, each channel of a multi-channel
    /// notification gets its own bucket. Email and `in_app` are then
    /// throttled independently.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_channel: bool,
    /// `burst <N>` — number of immediate dispatches the bucket allows
    /// before throttling starts. Useful for OTP/login flows where the
    /// first 1-3 sends must go through without delay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_round_trips_through_json() {
        let rp = RetryPolicy {
            count: 5,
            backoff: BackoffStrategy::Exponential,
        };
        let json = serde_json::to_string(&rp).unwrap();
        let back: RetryPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(rp, back);
    }

    #[test]
    fn job_trigger_event_round_trips_with_kind_value_envelope() {
        let trig = JobTrigger::Event {
            event: crate::QualifiedName {
                feature: Some("customer".into()),
                name: "customer_archived".into(),
            },
        };
        let json = serde_json::to_string(&trig).unwrap();
        let back: JobTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trig, back);
        assert!(json.contains("\"kind\":\"event\""));
    }

    #[test]
    fn job_trigger_schedule_round_trip() {
        let trig = JobTrigger::Schedule {
            cron: "0 2 * * *".into(),
        };
        let json = serde_json::to_string(&trig).unwrap();
        let back: JobTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trig, back);
        assert!(json.contains("\"kind\":\"schedule\""));
    }

    #[test]
    fn replay_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ReplayMode::Allow).unwrap(),
            "\"allow\""
        );
        assert_eq!(
            serde_json::to_string(&ReplayMode::Deny).unwrap(),
            "\"deny\""
        );
    }

    #[test]
    fn dlq_spec_variants_round_trip() {
        let cases = [
            DlqSpec::Emit {
                event: "delivery_failed".into(),
            },
            DlqSpec::Drop {
                reason: "provider sends 429 storms".into(),
            },
        ];
        for dlq in cases {
            let json = serde_json::to_string(&dlq).unwrap();
            let back: DlqSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(dlq, back);
        }
    }

    #[test]
    fn digest_strategy_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&DigestStrategy::Merge).unwrap(),
            "\"merge\""
        );
        assert_eq!(
            serde_json::to_string(&DigestStrategy::Append).unwrap(),
            "\"append\""
        );
    }

    #[test]
    fn throttle_default_bools_skipped_in_json() {
        let throttle = NotificationThrottle {
            max_per: "1 hour".into(),
            per_recipient: false,
            per_channel: false,
            burst: None,
        };
        let json = serde_json::to_string(&throttle).unwrap();
        // Both default-false bools must be skipped.
        assert!(!json.contains("per_recipient"));
        assert!(!json.contains("per_channel"));
        assert!(!json.contains("burst"));
    }
}
