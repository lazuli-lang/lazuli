//! `--expand=event_groups` (Phase L Tier 3) + `--expand=aggregates`
//! (CL.C.4, roadmap §1.7) projection shapes.
//!
//! Event-groups carry the cross-cutting pattern + on_resource +
//! payload tuple plus the list of authored events that match the
//! pattern. Aggregates carry the root resource, the contained
//! resources, and the invariant catalog — each invariant stringifies
//! its `EvalPredicate` back so the projection remains stable across
//! `Closed` / `Unparsed` / `Contains` shapes.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectEventGroup {
    pub(in crate::commands::inspect) pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) on_resource: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) payload: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) audit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) events: Vec<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}

// CL.C.4 — `--expand=aggregates` projections (roadmap §1.7).
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAggregate {
    pub(in crate::commands::inspect) name: String,
    pub(in crate::commands::inspect) root: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) contains: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) invariants: Vec<InspectInvariant>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectInvariant {
    pub(in crate::commands::inspect) name: String,
    /// Closed-catalog predicate text. The IR carries an
    /// `EvalPredicate`; we stringify it back so the projection is
    /// stable across `Closed` / `Unparsed` / `Contains` shapes.
    pub(in crate::commands::inspect) when: String,
    /// Predicate kind as projected. Aids LLM/cold-reader inspection;
    /// stable closed catalog: `closed | contains | tools_calls | unparsed`.
    pub(in crate::commands::inspect) when_kind: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(in crate::commands::inspect) message: String,
}
