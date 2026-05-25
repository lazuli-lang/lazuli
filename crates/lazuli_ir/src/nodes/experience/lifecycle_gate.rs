//! Lifecycle-gate IR — `requires_lifecycle` view slots, `resume` blocks,
//! and the analyzer-cached resolved gate.
//!
//! A "lifecycle gate" expresses: *the actor must have advanced
//! `<Resource>.<state>` before this view may paint*. When the gate fails
//! the runtime consults a `resume` router to decide where to send the
//! actor instead. Both shapes lower to span-tagged IR so doctor
//! diagnostics can point at the exact authored line.
//!
//! ## Catalog
//!
//! - [`RequiresLifecycle`] — `requires_lifecycle <Resource> = <state>`
//!   slot on a view / route guard.
//! - [`ResumeRouter`] / [`ResumeArm`] / [`ResumeArmKind`] — `resume <name>
//!   from <query> { … }` block.
//! - [`ResolvedLifecycleGate`] — analyzer-populated cache; codegen reads
//!   this rather than walking the chain at every site.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// `requires_lifecycle <Resource> = <state>` slot on a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiresLifecycle {
    /// PascalCase resource name matching `Resource.name`.
    pub resource: String,
    /// snake_case state name matching `LifecycleState.name`.
    pub state: String,
    /// Optional sub-step tag for multiple route screens that share one
    /// lifecycle state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substep: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `resume <name>` block at feature top-level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeRouter {
    /// snake_case identifier.
    pub name: String,
    /// `query.lookup` name, e.g. `my_host`.
    pub source_query: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arms: Vec<ResumeArm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One match arm in a `resume` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeArm {
    pub kind: ResumeArmKind,
    /// Optional sub-step tag used when multiple resume arms target
    /// screens within the same lifecycle state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substep: Option<String>,
    /// View name in the same feature, or `<feature>.<view>` cross-feature.
    pub target_view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Lifecycle state name, `none` marker, or wildcard arm kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ResumeArmKind {
    /// A lifecycle state name.
    State(String),
    /// 404 / no-row arm.
    None,
    /// Catch-all `* -> view <v>` arm.
    Wildcard,
}

/// Cached analyzer output: per-view resolved lifecycle gate. Populated
/// during analyzer pass; null when the view has no `requires_lifecycle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLifecycleGate {
    pub resource: String,
    pub state: String,
    /// Optional sub-step tag copied from `RequiresLifecycle.substep`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substep: Option<String>,
    /// Name of the resume router resolved from `on_lifecycle_pending`.
    pub resume_router: String,
    /// Qualified source query, e.g. `host.query.my_host`.
    pub source_query_qualified: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_lifecycle_round_trips() {
        let v = RequiresLifecycle {
            resource: "Host".into(),
            state: "approved".into(),
            substep: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: RequiresLifecycle = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn resume_arm_kind_serialises_state_arm_with_tag_and_value() {
        let v = ResumeArmKind::State("approved".into());
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"kind\":\"state\",\"value\":\"approved\"}");
        let back: ResumeArmKind = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn resume_arm_kind_wildcard_has_no_value() {
        let v = ResumeArmKind::Wildcard;
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"kind\":\"wildcard\"}");
    }

    #[test]
    fn resume_router_round_trips_with_arms() {
        let v = ResumeRouter {
            name: "host_resume".into(),
            source_query: "my_host".into(),
            arms: vec![ResumeArm {
                kind: ResumeArmKind::State("pending".into()),
                substep: None,
                target_view: "pending_screen".into(),
                span_ref: None,
            }],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ResumeRouter = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn resolved_lifecycle_gate_round_trips() {
        let v = ResolvedLifecycleGate {
            resource: "Host".into(),
            state: "approved".into(),
            substep: None,
            resume_router: "host_resume".into(),
            source_query_qualified: "host.query.my_host".into(),
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ResolvedLifecycleGate = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
