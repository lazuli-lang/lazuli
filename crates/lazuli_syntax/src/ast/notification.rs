//! Notification, channel, and cache-profile AST surfaces.
//!
//! These three feature-scoped declarations share the same wiring zone
//! (event-driven side outputs) and a common dependency on Job's
//! trigger/retry types, so they live together.
//!
//! - `Notification` — outbound notification (email, in_app, etc.).
//!   `docs/proposals/bucket-notifications-expanded.md`. Adds `digest`
//!   and `throttle` sub-blocks beyond the bare `trigger / recipient /
//!   template / policy` spine.
//! - `Channel` — realtime broadcast channel,
//!   `docs/proposals/bucket-realtime-scope.md`. Closed three-child
//!   body (`tenant_from / policy / payload`), all required.
//! - `CacheProfileDecl` — cache lookup profile (CL.C.3). Two required
//!   children (`key / ttl`) plus a closed-catalog of optional knobs.
//!
//! All three use `PolicyExprAst` (RB.S6 structured-policy form) and
//! Notification additionally re-uses Job's `JobTrigger` + `JobRetry`.

use serde::{Deserialize, Serialize};

use super::{JobRetry, JobTrigger, PolicyExprAst, Span};

/// `notification <name>` block — outbound notification (email, in_app,
/// push, ...).
///
/// Feature-scoped sibling of `Job` / `Channel` / `Poller`. Reuses Job's
/// trigger/retry vocabulary so authors only learn one shape for
/// background work. See module-level docs and
/// `docs/proposals/bucket-notifications-expanded.md` for the `digest`
/// and `throttle` sub-blocks specific to notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `channel email, in_app` — comma-split list.
    pub channels: Vec<String>,
    /// `recipient target.email` — path captured verbatim.
    pub recipient: String,
    /// `trigger event ...` or `trigger schedule "..."`.
    pub trigger: JobTrigger,
    /// `tenant_from payload.<axis>_id`.
    pub tenant_from: Option<String>,
    /// `idempotency by <path>`.
    pub idempotency_by: Option<String>,
    pub retry: Option<JobRetry>,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    pub emits: Vec<String>,
    /// Notifications expanded bucket cycle — optional `digest` sub-block.
    /// Captured verbatim from the canonical-indent slice and lowered to
    /// `ir::NotificationDigest` in the analyzer.
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` sub-block.
    /// Distinct from scalar `rate_limit`; lowered to
    /// `ir::NotificationThrottle` in the analyzer.
    pub throttle: Option<NotificationThrottle>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the `digest`
/// sub-block. Fields are captured verbatim; closed-catalog validation
/// for `template_strategy` and the `every`/`max_size` shape lives in
/// the analyzer/doctor layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "<duration>"` — required.
    pub every: String,
    /// `group_by <path>` — optional.
    pub group_by: Option<String>,
    /// `max_size <N>` — optional.
    pub max_size: Option<u32>,
    /// `template_strategy <merge|append>` — optional.
    pub template_strategy: Option<String>,
    pub span: Span,
}

/// Realtime bucket cycle MVP — `channel <name>` AST surface.
///
/// Closed three-child body: `tenant_from <axis>`, `policy @policy.<name>`,
/// `payload <RecordType>`. All three are required; missing any one
/// yields a parse error at lowering time so authors get a precise
/// diagnostic. Optional children (audit, rate_limit, broadcast wiring)
/// are deferred per `docs/proposals/bucket-realtime-scope.md` pending
/// pilot evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// `tenant_from <axis>` — axis name verbatim.
    pub tenant_from: String,
    /// `policy @policy.<name>` — verbatim atom.
    pub policy: String,
    /// `payload <RecordType>` — verbatim type-name reference.
    /// Doctor `CHANNEL-PAYLOAD-001` resolves it.
    pub payload: String,
    pub span: Span,
}

/// Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile
/// AST. Required body children: `key <expr>`, `ttl <literal-or-prose>`.
/// Optional: `namespace <label>`, `tags <l1>[, <l2>...]`,
/// `stale_while_revalidate <literal>`, `coalesce <bool>`, `sliding <bool>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheProfileDecl {
    /// Profile identifier (lowercase, dashes allowed).
    pub name: String,
    /// `key <expr>` — opaque template stored verbatim.
    pub key: String,
    /// `ttl <literal>` — raw token (e.g. `5m`, `"5 minutes"`).
    /// Lowering parses it into the typed `CacheTtl` enum.
    pub ttl: String,
    /// `namespace <label>` — single label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `tags <l1>[, <l2>, ...]` — comma-separated labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `stale_while_revalidate <literal>` — raw token; lowered into
    /// `CacheTtl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_while_revalidate: Option<String>,
    /// `coalesce <bool>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce: Option<bool>,
    /// `sliding <bool>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding: Option<bool>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the
/// `throttle` sub-block. Distinct keyword from scalar `rate_limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "<duration>"` — required.
    pub max_per: String,
    /// `per_recipient` flag — bare child line.
    pub per_recipient: bool,
    /// `per_channel` flag — bare child line.
    pub per_channel: bool,
    /// `burst <N>` — optional.
    pub burst: Option<u32>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_serde_roundtrips_closed_body() {
        let c = Channel {
            name: "realtime".into(),
            tenant_from: "workspace".into(),
            policy: "@policy.member".into(),
            payload: "RealtimeUpdate".into(),
            span: Span::new(0, 0),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Channel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn cache_profile_decl_optional_fields_serde() {
        let p = CacheProfileDecl {
            name: "list_recent".into(),
            key: "tenant".into(),
            ttl: "5m".into(),
            namespace: None,
            tags: vec![],
            stale_while_revalidate: None,
            coalesce: None,
            sliding: None,
            span: Span::new(0, 0),
        };
        let s = serde_json::to_string(&p).unwrap();
        // Empty optional fields elided thanks to `skip_serializing_if`.
        assert!(!s.contains("namespace"));
        assert!(!s.contains("tags"));
    }
}
