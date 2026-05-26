//! `--expand=jobs` projection shapes (Phase L Tier 3).
//!
//! Per-feature lifted `ir::Job` records. Operational kind
//! (`scheduled`/`reactor`/`queued_worker`) is derived; the trigger
//! enum carries the `event` / `schedule` discriminator; the body
//! enum carries the three legal shapes (`handler`, declarative spine,
//! and emits-only reactors). Shape mirrors `InspectAgent` /
//! `InspectNotification` so a consumer can read the job/webhook/event
//! triple cold without joining tables.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJob {
    pub(in crate::commands::inspect) name: String,
    /// Derived operational kind: `scheduled` / `reactor` / `queued_worker`.
    pub(in crate::commands::inspect) operational_kind: &'static str,
    pub(in crate::commands::inspect) trigger: InspectJobTrigger,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) queue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) idempotency_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) retry: Option<InspectJobRetry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) fanout: Option<InspectJobFanout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) timeout: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) external_calls: Vec<InspectJobExternalCall>,
    pub(in crate::commands::inspect) body: InspectJobBody,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) emits: Vec<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value")]
pub(in crate::commands::inspect) enum InspectJobTrigger {
    /// `trigger event <feature>.<event>`.
    Event(String),
    /// `trigger schedule "<cron>"`.
    Schedule(String),
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJobRetry {
    pub(in crate::commands::inspect) count: u32,
    pub(in crate::commands::inspect) backoff: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJobFanout {
    pub(in crate::commands::inspect) scope: &'static str,
    pub(in crate::commands::inspect) axis: String,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJobExternalCall {
    pub(in crate::commands::inspect) slot: String,
    pub(in crate::commands::inspect) op: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) args: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value")]
pub(in crate::commands::inspect) enum InspectJobBody {
    /// `handler "./..."` — declarative path with optional return type.
    Handler(InspectJobHandler),
    /// Declarative body with the typed declarative spine (Phase L Tier
    /// 4b). Replaces the previous raw-string carve-out.
    Declarative(InspectJobDeclarative),
    /// Job declares no body — emits-only reactor.
    None,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJobHandler {
    pub(in crate::commands::inspect) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) returns: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectJobDeclarative {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) lets: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) effect: Option<String>,
}
