//! `Feature` — the IR root for one `.lzi` source file.
//!
//! A feature is the unit of product capability authored in one file.
//! It composes every declarative slot the language exposes
//! (resources, commands, queries, events, jobs, webhooks,
//! notifications, channels, pollers, mcp_servers, aggregates,
//! surfaces, auth, agents, reports, caches, …) into a single
//! container the analyzer lowers and downstream consumers
//! (codegen, doctor, inspect, LSP) read top-down.
//!
//! `Eq` is intentionally omitted: Cut A's `Agent.temperature` /
//! `top_p` fields are `Option<f64>`, and `f64` has no `Eq` impl.
//! Consumers that need equality use `PartialEq` (`assert_eq!`-style
//! comparisons still work).
//!
//! `FeatureRequirement` is the small companion type that records a
//! feature's `requires <kind> <name>` lines so the cross-feature
//! analyzer can resolve them against the workspace catalog.

use serde::{Deserialize, Serialize};

use crate::{
    Agent, Aggregate, Api, Auth, CacheProfile, Channel, Command, ConventionOrigin, Defaults,
    EnumDecl, EscapeRoute, Event, EventGroup, Extension, FeatureErrors, Job, MCPServerSpec,
    NonGoal, Notification, Policies, Poller, Query, Record, Report, Resource, ResumeRouter, Rule,
    SpanRef, Surface, TenantMigration, Translation, Webhook, Workflow,
};

/// A feature is the unit of product capability authored in one `.lzi` file.
///
/// `Eq` is intentionally omitted: Cut A's `Agent.temperature` / `top_p`
/// fields are `Option<f64>`, and `f64` has no `Eq` impl. Consumers that
/// need equality use `PartialEq` (`assert_eq!`-style comparisons still
/// work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_goals: Vec<NonGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    /// Iron-hand `knowledge <sector>` — the bareword sector slug naming
    /// the `.lazuli/knowledge/<sector>/` vault this feature draws from.
    /// `None` when the feature declares no `knowledge` line; absent =>
    /// `None` keeps pre-knowledge fixtures byte-identical on the wire.
    /// Sector ↔ vault cross-checks live in the planned `VOCAB-KNOWLEDGE-*`
    /// doctor lints (a later stage). See
    /// `docs/proposals/knowledge-sector-field.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<String>,
    pub defaults: Defaults,
    pub uses: Vec<String>,
    /// Span anchors for each entry in `uses`. Same length as `uses`. Populated
    /// by the analyzer when lowering from `.lzi` source; empty when `Feature`
    /// is constructed programmatically (tests, manual IR fixtures).
    ///
    /// Used by `lazuli_analyzer::build_symbol_origin_index` to anchor each
    /// `ImportEdge.uses_at`. See `docs/proposals/lsp-symbol-origin.md` §6.5.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses_spans: Vec<SpanRef>,
    /// Optional consumer-side version pin per `uses` entry. Same length as
    /// `uses` when populated; entries are `Some(N)` for `uses <feature> version v<N>`
    /// and `None` for unpinned entries. Empty when the feature has no `uses`
    /// lines or when constructed programmatically.
    ///
    /// Drives `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` per
    /// `docs/proposals/cross-feature-contracts.md` §5.4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses_versions: Vec<Option<u16>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<FeatureRequirement>,
    pub enums: Vec<EnumDecl>,
    pub resources: Vec<Resource>,
    pub events: Vec<Event>,
    pub rules: Vec<Rule>,
    pub policies: Policies,
    /// IR Error-Vocab — `errors` block lifted into IR. Carries both
    /// exposure rules (legacy LSP surface, now lowered) and typed
    /// per-code message overrides. `None` when the feature uses the
    /// runtime defaults. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<FeatureErrors>,
    pub commands: Vec<Command>,
    /// Phase L Tier 4b — `api <name>` declarations lifted from the
    /// canonical-indent slice. Legacy lowering leaves this empty;
    /// `lower_feature_skeleton` populates it from
    /// `FeatureSkeleton.apis`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apis: Vec<Api>,
    /// Phase L Tier 4d — `record <Name>` declarations lifted from the
    /// canonical-indent slice. Legacy lowering leaves this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<Record>,
    pub queries: Vec<Query>,
    /// `resume <name>` blocks for lifecycle-aware route gates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_routers: Vec<ResumeRouter>,
    pub workflows: Vec<Workflow>,
    pub jobs: Vec<Job>,
    pub webhooks: Vec<Webhook>,
    /// Phase L Tier 3 — `notification <name>` declarations lifted from
    /// the canonical-indent slice. Legacy lowering leaves this empty;
    /// the inspect projection used to harvest notifications via
    /// text-pattern and now reads from IR when populated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<Notification>,
    /// Phase L Tier 3 — `event_group <pattern> on <Resource>`
    /// declarations lifted from the canonical-indent slice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_groups: Vec<EventGroup>,
    /// Migrations bucket cycle Route C — `tenant_migration <name>`
    /// declarations lifted from the canonical-indent slice. Mirrors the
    /// `jobs` slot exactly: one entry per declared tenant migration.
    /// Doctor `TM-*` diagnostics consume this slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tenant_migrations: Vec<TenantMigration>,
    /// i18n bucket cycle — `translation` block lifted from the
    /// canonical-indent slice. `None` when the feature does not author
    /// translation keys. Surfaces declared catalog path + typed keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<Translation>,
    /// L0 #8 — `poller <name>` declarations (docs/proposals/poller-vocab.md).
    /// Sibling slot of `jobs` / `webhooks` / `notifications`. Each
    /// entry models a persistent-cursor resolution loop over a same-
    /// feature resource. Additive: existing fixtures deserialize with
    /// an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pollers: Vec<Poller>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    pub surfaces: Vec<Surface>,
    pub extensions: Vec<Extension>,
    pub escape_routes: Vec<EscapeRoute>,
    /// Cut A: `agent <name>` declarations. The legacy lowering path
    /// produces an empty `Vec`; the canonical-indent slice in
    /// `lazuli_syntax::parse_feature_skeletons` is the producer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Agent>,
    /// Report vocab — `report <name>` declarations. See
    /// `docs/proposals/report-vocab.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<Report>,
    /// Realtime bucket cycle MVP — `channel <name>` declarations
    /// (see `docs/proposals/bucket-realtime-cycle.md`). Sibling slot
    /// of `events` / `notifications` / `pollers`. Each entry models
    /// a typed, tenant-scoped, policy-gated push stream. Additive:
    /// pre-realtime fixtures deserialize with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profiles. Sibling slot of `jobs`/`webhooks`/`notifications`.
    /// Queries reference profiles by name (`cache product_view`); the
    /// inline `cache { key, ttl }` form on a query stays for one-off
    /// ttl/key pairs. Additive: pre-CL.C.3 fixtures deserialize with
    /// an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caches: Vec<CacheProfile>,
    /// CL.C.4 — `aggregate <Name>` declarations (DDD consistency
    /// boundary). Each entry pins a root resource + a closed set of
    /// member resources + invariants spanning the cluster. Sibling
    /// slot of `resources`/`commands`/`policies`. Additive: features
    /// without `aggregate` blocks deserialize with an empty vec.
    /// See roadmap §1.7 + spec wave-c-cl4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<Aggregate>,
    /// MCP bucket cycle — `mcp_server <name>` declarations. Sibling
    /// slot of `notifications` / `channels` / `pollers`. Each entry
    /// projects a feature's surface over MCP (tools / resources /
    /// prompts) per `docs/proposals/bucket-mcp-cycle.md`. Codegen
    /// emits `*.mcp.gen.go` and wires the Go runtime's
    /// `runtime/go/lazuli/mcp` package against the SDK. Additive:
    /// pre-MCP fixtures deserialize with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<MCPServerSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// `conventions [crud]` synth origin map — keys are `Command.name` /
    /// `Query.name()` strings the synthesis pass either appended or
    /// would have appended (when the author wrote an override). Values
    /// describe whether the entry was synthesized or skipped because of
    /// an author-side override. Populated by Cell C3's synthesizer;
    /// consumed by Cell C4's `lazuli inspect features` annotation.
    /// Inlined by Cell C4 ahead of Cell C1's IR landing. Additive:
    /// pre-conventions fixtures deserialize empty.
    /// See `docs/proposals/ir-resource-conventions-crud.md` §11.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub synth_origins: std::collections::BTreeMap<String, ConventionOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `requires <kind> <name> contract <X>` entry declared on a
/// feature. Names a sibling feature / integration the host must
/// provide for this feature to function, plus the contract id the
/// host must satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRequirement {
    pub kind: String,
    pub name: String,
    pub contract: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_requirement_round_trips() {
        let r = FeatureRequirement {
            kind: "feature".into(),
            name: "billing".into(),
            contract: "v1".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: FeatureRequirement = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
