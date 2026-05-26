//! Experience IR — the lowered shape of `.lzx` documents.
//!
//! An *experience* is an audience-agnostic, platform-agnostic description
//! of what a feature shows the user. Each [`Experience`] holds a list of
//! [`ExperienceView`]s (the typed UI surfaces) plus [`ExperienceAction`]s
//! (the typed verbs the audience can fire). Per-platform projections live
//! in `*.web.lzx` / `*.mobile.lzx` and lower into [`PlatformSurface`]s
//! re-exported from [`surface`].
//!
//! ## Catalog
//!
//! - [`ExperienceModule`] — root of every `.lzx` document; holds the
//!   optional [`crate::AppManifest`], all routes, experiences, and
//!   platform surfaces.
//! - [`Experience`] — one named experience: views + resume routers +
//!   anchor extensions.
//! - [`ExperienceView`] — one declarative view: source query, submit
//!   command, blocks, actions, opens, guard, tests, resolved gate.
//! - [`ExperienceAction`] — typed action target binding.
//! - [`ViewExtension`] / [`ViewExtensionSlot`] / [`ViewExtensionOrder`] —
//!   anchor-based extension surface.
//! - [`ViewTestAssertion`] — closed catalog `accepted by` / `rejected
//!   by` extensibility assertion.
//!
//! ## Wave R3-F split
//!
//! Supporting IR families live in submodules by concern:
//!
//! - [`route`] — [`AppRoute`] + [`RouteLoader`].
//! - [`guard`] — [`ViewGuard`] + [`ForbidWhen`] + [`RouteGuardDefaults`].
//! - [`lifecycle_gate`] — [`RequiresLifecycle`] + [`ResumeRouter`] +
//!   [`ResumeArm`] + [`ResumeArmKind`] + [`ResolvedLifecycleGate`].
//! - [`surface`] — [`PlatformSurface`] + [`Platform`] + [`AudienceSurface`] +
//!   [`PlatformView`].
//!
//! The crate root re-exports every public type so the ABI surface stays
//! `lazuli_ir::Experience`, `lazuli_ir::AppRoute`, …

pub mod guard;
pub mod lifecycle_gate;
pub mod route;
pub mod surface;

pub use guard::{ForbidWhen, RouteGuardDefaults, ViewGuard};
pub use lifecycle_gate::{
    RequiresLifecycle, ResolvedLifecycleGate, ResumeArm, ResumeArmKind, ResumeRouter,
};
pub use route::{AppRoute, RouteLoader};
pub use surface::{AudienceSurface, Platform, PlatformSurface, PlatformView};

use serde::{Deserialize, Serialize};

use crate::PolicyAtom;
use crate::SpanRef;
use crate::nodes::app_manifest::AppManifest;

/// Root of every `.lzx` document.
///
/// Observability bucket cycle row 36 — `Eq` dropped to match the
/// downstream `AppManifest` change (which now carries `Option<f64>`
/// sample-rate fields). `PartialEq` is preserved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperienceModule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<AppRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub experiences: Vec<Experience>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<PlatformSurface>,
}

/// `experience <name> { … }` block — one feature-scoped UX surface in
/// the `.lzx` family. Collects the views, resume routers, and view
/// extensions the feature contributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experience {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ExperienceView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_routers: Vec<ResumeRouter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ViewExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One view declared inside an [`Experience`]. Holds the routes that
/// open into the view, the optional source/submit bindings, blocks +
/// actions, declarative tests, and the resolved policy / lifecycle
/// guards the analyzer caches for codegen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperienceView {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ExperienceAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opens: Vec<String>,
    /// Wave 4 — view test assertions. Previously `Vec<String>` raw lines;
    /// now a typed enum carrying `accepted by <feature>` / `rejected by
    /// <feature>` semantics with span back-references for diagnostic
    /// surfaces. The closed catalog is **extensibility-only**: views do
    /// NOT participate in policy / predicate testing (those belong to
    /// `command.tests`, `rule.tests`, `transition.tests`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<ViewTestAssertion>,
    /// `ir-route-guards` §3.2 — per-view policy + redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<ViewGuard>,
    /// LAZ-67 — analyzer-cached guard policy after walking
    /// view/route -> audience -> app -> built-in defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_guard_policy: Option<Vec<PolicyAtom>>,
    /// LAZ-85 — analyzer-cached lifecycle route gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_lifecycle_gate: Option<ResolvedLifecycleGate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Wave 4 (TDD/BDD proposal) — typed view test assertion.
///
/// View tests use a closed extensibility vocabulary; they do NOT
/// participate in policy / predicate testing. Only `accepted by
/// <feature>` and `rejected by <feature>` are admissible. The parser
/// rejects any other shape; the analyzer passes the typed value through
/// to doctor rules `TEST-VIEW-EXTENSIBILITY-001` and `TEST-VIEW-DRIFT-001`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewTestAssertion {
    /// `accepted by <feature>` — declares that `<feature>` may extend
    /// the view via its anchor.
    AcceptedBy {
        feature: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span_ref: Option<SpanRef>,
    },
    /// `rejected by <feature>` — declares that `<feature>` must NOT
    /// extend the view via its anchor.
    RejectedBy {
        feature: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span_ref: Option<SpanRef>,
    },
}

impl ViewTestAssertion {
    /// Returns the asserted feature name (the value to the right of
    /// `accepted by` / `rejected by`).
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::ViewTestAssertion;
    ///
    /// let a = ViewTestAssertion::AcceptedBy { feature: "checkout".into(), span_ref: None };
    /// assert_eq!(a.feature(), "checkout");
    /// ```
    pub fn feature(&self) -> &str {
        match self {
            ViewTestAssertion::AcceptedBy { feature, .. }
            | ViewTestAssertion::RejectedBy { feature, .. } => feature,
        }
    }

    /// Returns the source span for this assertion, when known.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::ViewTestAssertion;
    ///
    /// let a = ViewTestAssertion::AcceptedBy { feature: "checkout".into(), span_ref: None };
    /// assert!(a.span_ref().is_none());
    /// ```
    pub fn span_ref(&self) -> Option<&SpanRef> {
        match self {
            ViewTestAssertion::AcceptedBy { span_ref, .. }
            | ViewTestAssertion::RejectedBy { span_ref, .. } => span_ref.as_ref(),
        }
    }
}

/// One `actions.<name>` entry inside an [`ExperienceView`]. Names the
/// command/api target the view exposes as an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperienceAction {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `view_extension <anchor>` block — a feature contributes blocks
/// into another feature's view via the named anchor. Slots scope the
/// contribution to specific named slots, platforms, audiences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtension {
    pub anchor: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<ViewExtensionSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One named slot inside a [`ViewExtension`]. Carries the blocks
/// contributed, optional ordering constraint, and platform/audience
/// filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtensionSlot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<ViewExtensionOrder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audiences: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Optional ordering constraint inside a [`ViewExtensionSlot`].
/// `relation` is `"before"` / `"after"` / `"replace"`; `target` names
/// the sibling block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtensionOrder {
    pub relation: String,
    pub target: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_module_minimal_round_trip() {
        let v = ExperienceModule {
            app: None,
            routes: vec![],
            experiences: vec![],
            surfaces: vec![],
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{}");
        let back: ExperienceModule = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn view_test_assertion_serialises_with_snake_case_tag() {
        let v = ViewTestAssertion::AcceptedBy {
            feature: "checkout".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"accepted_by\""));
        let back: ViewTestAssertion = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
        assert_eq!(v.feature(), "checkout");
    }

    #[test]
    fn view_test_assertion_rejected_by_round_trip() {
        let v = ViewTestAssertion::RejectedBy {
            feature: "abandoned".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"rejected_by\""));
        let back: ViewTestAssertion = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn experience_round_trips_with_one_view() {
        let v = Experience {
            name: "Customer".into(),
            imports: vec![],
            views: vec![ExperienceView {
                name: "all".into(),
                anchor: None,
                routes: vec![],
                extensible_by: vec![],
                source: Some("query.all".into()),
                submit: None,
                blocks: vec![],
                actions: vec![],
                opens: vec![],
                tests: vec![],
                guard: None,
                resolved_guard_policy: None,
                resolved_lifecycle_gate: None,
                span_ref: None,
            }],
            resume_routers: vec![],
            extensions: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: Experience = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn view_extension_round_trips_with_slots() {
        let v = ViewExtension {
            anchor: "footer".into(),
            blocks: vec!["@client.footer".into()],
            slots: vec![ViewExtensionSlot {
                name: "primary".into(),
                order: Some(ViewExtensionOrder {
                    relation: "before".into(),
                    target: "@client.legal".into(),
                }),
                blocks: vec!["@client.help".into()],
                platforms: vec![],
                audiences: vec![],
                span_ref: None,
            }],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ViewExtension = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
