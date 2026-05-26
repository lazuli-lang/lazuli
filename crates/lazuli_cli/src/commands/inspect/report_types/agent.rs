//! Agent-axis projection shapes — Cut A / Cut A.7 / Cut A.8.
//!
//! Three families share this module because they all describe agent
//! behaviour or its derived projections:
//!
//! - `InspectAgent` + `InspectAgentExpose` — the per-agent record
//!   exposed under `--expand=agents` (always-on agent block).
//! - `InspectAgentToolsEntry` + `InspectAgentToolBinding` — the
//!   `--expand=tools` dispatch graph keyed by agent.
//! - `InspectExposeEntry` — the `--expand=expose` unified HTTP route
//!   table (every `api` block + every agent declaring `expose http`).
//! - `InspectBuiltInTraceEvent` + `InspectBuiltInTraceField` — the
//!   built-in trace events surfaced alongside authored `events` under
//!   `--expand=events` (today only `agent_run`).

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectBuiltInTraceEvent {
    pub(in crate::commands::inspect) name: String,
    pub(in crate::commands::inspect) fires_per: String,
    pub(in crate::commands::inspect) payload: Vec<InspectBuiltInTraceField>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectBuiltInTraceField {
    pub(in crate::commands::inspect) name: String,
    #[serde(rename = "type")]
    pub(in crate::commands::inspect) type_text: String,
    pub(in crate::commands::inspect) optional: bool,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectExposeEntry {
    /// `agent` or `api` — the kind of declaration that produced the route.
    pub(in crate::commands::inspect) kind: &'static str,
    /// `<feature>.<kind>.<name>` for stable cross-references.
    pub(in crate::commands::inspect) origin: String,
    pub(in crate::commands::inspect) method: String,
    pub(in crate::commands::inspect) path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) route_slots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rate_limit_override: Option<String>,
}

/// One per agent in the file. Carries every tool reference the agent
/// dispatches plus the local categorisation (kind, scope). Cross-feature
/// resolution lives in doctor; the projection records the symbol shape
/// so consumers can compose either path.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAgentToolsEntry {
    pub(in crate::commands::inspect) agent: String,
    pub(in crate::commands::inspect) tools: Vec<InspectAgentToolBinding>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAgentToolBinding {
    /// Canonical reference exactly as the author wrote it.
    pub(in crate::commands::inspect) reference: String,
    /// Local-categorisation of the reference: `query.list`, `query.lookup`,
    /// `query.sql`, `query`, `command`, `api`, `adapter`. Cross-feature
    /// resolution narrows `query` to one of the three subkinds.
    pub(in crate::commands::inspect) kind: &'static str,
    /// `local`, `cross_feature`, or `adapter` — the resolution scope.
    pub(in crate::commands::inspect) scope: &'static str,
    /// `read` / `write` / `unknown`. Adapter references rely on the
    /// registry; local kinds map directly (`command` is always `write`,
    /// queries default to `read`).
    pub(in crate::commands::inspect) derived_effect: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAgent {
    pub(in crate::commands::inspect) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) inputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rate_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) output: Option<String>,
    /// Cut A — `text` / `stream` / `discriminated_enum` /
    /// `discriminated_record`. Derived from the `output` declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) output_kind: Option<&'static str>,
    /// Cut A — the enum or record name the discriminator points at,
    /// when `output_kind` resolves to a discriminated form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) output_discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) temperature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) max_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) top_p: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) prompt: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) tools: Vec<String>,
    /// Cut A — eval `case <name>` headers under this agent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) evals: Vec<String>,
    /// Cut A — `pinned` when both `temperature 0` and `seed <int>` are
    /// declared (cases gate CI); `nondeterministic` otherwise (cases
    /// run as informational).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) eval_determinism: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) safety: Option<String>,
    /// Cut A.7 — `expose http` block summary. Always-on field
    /// (file-local; no cross-feature resolution).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) expose_http: Option<InspectAgentExpose>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAgentExpose {
    pub(in crate::commands::inspect) method: String,
    pub(in crate::commands::inspect) path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) route_slots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rate_limit_override: Option<String>,
}
