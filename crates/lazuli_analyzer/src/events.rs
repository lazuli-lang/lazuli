//! Event-driven envelope lowerings — `webhook`, `channel`,
//! `notification`, `mcp_server`, `event_group`, `tenant_migration`.
//!
//! ## Why these ship together
//!
//! Each block in this module is an envelope around the shared workflow
//! leaf catalog: `retry`, `tenant_from`, `idempotency`, `external_call`,
//! `emit_predicate`, `notification_digest`, `notification_throttle`,
//! `event_variant_field`, `mcp_*`. The envelopes differ in their
//! trigger surface (inbound HTTP webhook, outbound notification,
//! realtime channel, MCP server, declarative event group, cross-tenant
//! migration) but the body shapes converge on the same handful of
//! shared specs. Grouping them in one module makes the trigger-vs-spec
//! split auditable.
//!
//! ## Closed-catalog enforcement at lowering
//!
//! Two diagnostics fire here:
//! * `UnsupportedVerifyScheme` — `webhook verify <scheme>` outside
//!   `{hmac}` (Phase L Tier 3).
//! * `UnknownEnum` — `mcp_server transport <transport>` outside
//!   `{stdio, http_sse, http_streamable}` (MCP-TRANSPORT-001).
//!
//! Everything else is mechanical projection; doctor handles
//! cross-feature reference resolution downstream
//! (`CHANNEL-PAYLOAD-001`, `MCP-AUTH-001`, `TM-IDEMP-001`).
//!
//! ## What does NOT live here
//!
//! `lower_event_group`'s per-variant `EventField` builder
//! (`lower_event_variant_field`) lives in `workflow.rs` along with the
//! MCP leaf builders (`lower_mcp_tool`, `lower_mcp_resource`,
//! `lower_mcp_prompt`) and the workflow leaves the envelopes call into
//! (`lower_retry`, `lower_fanout`, `lower_job_trigger`,
//! `lower_external_call`, `lower_emit_predicate`,
//! `lower_notification_digest`, `lower_notification_throttle`,
//! `lower_tenant_migration_target`).

use crate::errors::AnalyzeError;
use crate::expr::{lower_path_string, lower_policy_atom, lower_policy_expr};
use crate::helpers::span_of;
use crate::types::type_ref_from_text;
use crate::workflow::{
    lower_emit_predicate, lower_event_variant_field, lower_job_trigger, lower_mcp_prompt,
    lower_mcp_resource, lower_mcp_tool, lower_notification_digest, lower_notification_throttle,
    lower_retry, lower_tenant_migration_target,
};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

include!("events_p1.rs");
include!("events_p2.rs");
