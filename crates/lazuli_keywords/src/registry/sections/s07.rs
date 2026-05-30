//! Registry `ALL` section 7/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};
use super::super::builders::*;
use super::super::facets::*;

pub(crate) const ROWS: &[CapabilitySpec] = &[
    stmt("outbox", Context::Job, STMT, "Outbox-pattern marker."),
    // ── webhook + verify/replay/dlq sub ──
    stmt("payload", Context::Webhook, STMT, "Webhook payload type."),
    stmt(
        "payload_group",
        Context::Webhook,
        STMT,
        "Webhook payload group.",
    ),
    stmt(
        "payload_from",
        Context::Webhook,
        STMT,
        "Payload source reference.",
    ),
    stmt(
        "verify",
        Context::Webhook,
        SECTION,
        "Signature-verification block.",
    ),
    stmt(
        "replay",
        Context::Webhook,
        SECTION,
        "Replay-protection block.",
    ),
    stmt("dlq", Context::Webhook, SECTION, "Dead-letter-queue block."),
    // H2 reconcile: `dedupe`/`within` belong to the webhook `replay` sub-block;
    // the grammar colors them `entity.name.function.statement.replay`. Promote
    // off the generic statement leaf so `#kw-replay` is faithful.
    stmt(
        "dedupe",
        Context::Webhook,
        REPLAY,
        "Replay deduplication key.",
    ),
    stmt("within", Context::Webhook, REPLAY, "Replay window."),
    stmt(
        "previous_version",
        Context::Webhook,
        STMT,
        "Previous payload version.",
    ),
    stmt("secret", Context::Webhook, STMT, "Verification secret."),
    stmt("header", Context::Webhook, STMT, "Signature header name."),
    // ── agent + tools/expose/io/evals ──
    stmt("model", Context::Agent, STMT, "LLM model."),
    // `output <Type>` / `output stream <Type>` / `output discriminator <Enum>`
    // on an `agent` body (`parser/lzi/agent/{mod,io}.rs`).
    stmt(
        "output",
        Context::Agent,
        STMT,
        "Agent output shape (bare / `stream` / `discriminator`).",
    ),
    stmt("prompt", Context::Agent, STMT, "Agent prompt."),
    stmt("safety", Context::Agent, STMT, "Safety constraints."),
    stmt("stream", Context::Agent, STMT, "Streaming mode."),
    stmt("temperature", Context::Agent, STMT, "Sampling temperature."),
    stmt("max_tokens", Context::Agent, STMT, "Max output tokens."),
    stmt("top_p", Context::Agent, STMT, "Top-p sampling."),
    stmt("seed", Context::Agent, STMT, "Deterministic seed."),
    stmt(
        "expose",
        Context::Agent,
        STMT,
        "Expose the agent over HTTP/MCP.",
    ),
    stmt(
        "discriminator",
        Context::Agent,
        STMT,
        "Output discriminator.",
    ),
    stmt(
        "tools",
        Context::Agent,
        SECTION,
        "Agent tool-binding block.",
    ),
    stmt(
        "tool",
        Context::Agent,
        STMT,
        "Declares a single tool / MCP tool.",
    ),
    // ── notification + digest/throttle ──
    stmt(
        "recipient",
        Context::Notification,
        STMT,
        "Notification recipient.",
    ),
    stmt(
        "template",
        Context::Notification,
        STMT,
        "Notification template.",
    ),
    stmt(
        "digest",
        Context::Notification,
        SECTION,
        "Digest-batching block.",
    ),
    stmt(
        "throttle",
        Context::Notification,
        SECTION,
        "Throttling block.",
    ),
    stmt(
        "every",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest interval.",
    ),
    stmt(
        "group_by",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest grouping key.",
    ),
    stmt(
        "template_strategy",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest template strategy.",
    ),
    stmt(
        "max_per",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Throttle max-per-window.",
    ),
    stmt(
        "per_recipient",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Per-recipient throttle.",
    ),
    stmt(
        "per_channel",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Per-channel throttle.",
    ),
    stmt(
        "burst",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Throttle burst allowance.",
    ),
    stmt(
        "max_size",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest max batch size.",
    ),
    // ── poller ──
    stmt("cursor", Context::Poller, STMT, "Poll cursor field."),
    stmt("tick", Context::Poller, STMT, "Poll interval."),
    stmt("backoff", Context::Poller, STMT, "Backoff policy."),
    stmt("counter", Context::Poller, STMT, "Poll counter."),
    stmt(
        "retry_quirk",
        Context::Poller,
        STMT,
        "Poller retry-quirk catalog entry.",
    ),
    stmt(
        "max_attempts",
        Context::Poller,
        STMT,
        "Max poll retry attempts.",
    ),
    // ── report + columns ──
    stmt("columns", Context::Report, SECTION, "Report column block."),
    stmt("formats", Context::Report, STMT, "Export formats."),
    stmt(
        "label",
        Context::Report,
        "entity.name.function.statement.columns.lazuli",
        "Column label.",
    ),
    stmt("visibility", Context::Report, STMT, "Report visibility."),
    // ── channel ──
    stmt(
        "payload_axis",
        Context::Channel,
        STMT,
        "Channel partition axis.",
    ),
    // ── tenant_migration ──
    stmt(
        "materialize_strategy",
        Context::TenantMigration,
        STMT,
        "Materialization strategy.",
    ),
    // ════════════════════════════════════════════════════════════════
    // api / operation body
    // ════════════════════════════════════════════════════════════════
    stmt("method", Context::Api, STMT, "HTTP method."),
    stmt("transport", Context::Api, STMT, "Transport (http)."),
    // `output <Type>` — required typed response shape on an `api`/`operation`
    // body (`parser/lzi/api.rs`). (Re-filed off the spurious `CommandBody`
    // row: command bodies have `input` but no `output`.)
    stmt(
        "output",
        Context::Api,
        STMT,
        "Typed response shape for the endpoint.",
    ),
    // ════════════════════════════════════════════════════════════════
    // auth block + sub
    // ════════════════════════════════════════════════════════════════
    stmt("identity", Context::Auth, STMT, "Identity strategy."),
    stmt("password", Context::Auth, STMT, "Password strategy."),
    stmt("oauth", Context::Auth, STMT, "OAuth provider."),
    stmt("mfa", Context::Auth, STMT, "Multi-factor auth."),
    stmt("sessions", Context::Auth, STMT, "Session settings."),
    stmt("access_ttl", Context::Auth, STMT, "Access-token TTL."),
    stmt("refresh_ttl", Context::Auth, STMT, "Refresh-token TTL."),
    stmt("rotation", Context::Auth, STMT, "Token-rotation policy."),
    stmt("grace", Context::Auth, STMT, "Rotation grace window."),
    stmt(
        "theft_detection_action",
        Context::Auth,
        STMT,
        "Token-theft action.",
    ),
    stmt("refresh", Context::Auth, STMT, "Refresh operation."),
    stmt("enroll", Context::Auth, STMT, "MFA enrollment."),
    stmt("hash", Context::Auth, STMT, "Password hash algorithm."),
    // `auth.sessions.cookie` — session-cookie transport block. Option (b)
    // from `docs/proposals/cookie-sessions-child.md`: the cookie attribute
    // vocabulary (`same_site`/`secure`/`http_only`/`domain`/`path` —
    // `Context::Cookie` rows under the app `cookie` SECTION above; `name`
    // is the generic `modifier`) is REUSED, not duplicated. This row is the
    // second anchor position: the `cookie` SECTION re-rooted under the
    // `sessions` parent (`Context::Auth`) instead of `Context::App`. The
    // parser dispatches the cookie children to the same closed catalog.
    kw(
        "cookie",
        Context::Auth,
        SECTION,
        "Session-cookie transport attributes block (name/same_site/secure/http_only/domain/path).",
    ),
    // ════════════════════════════════════════════════════════════════
    // errors block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "error",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error declaration.",
    ),
    stmt(
        "expose",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Expose error to client.",
    ),
    stmt(
        "hide",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Hide error from client.",
    ),
    stmt(
        "message",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error message.",
    ),
    stmt(
        "status",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "HTTP status.",
    ),
];
