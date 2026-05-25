//! `.lzx` document AST — the legacy/composite surface tree that
//! `lzx_*.lzx` files parse into.
//!
//! An `.lzx` file declares one app's UI shell: routes, experiences (UX
//! flows grouped under a name), and platform-specific surfaces (web /
//! mobile). The router contract here is what makes Lazuli's frontend
//! codegen tractable — the runtime materialises actual TanStack /
//! expo-router code from these declarations.
//!
//! Authoring shape (excerpt):
//!
//! ```text
//! app full_capsule
//!   title "Full Capsule"
//!   targets web mobile
//!   default_locale pt-BR
//!   route_guard
//!     default_policy @policy.authenticated
//!     on_unauthenticated "/login"
//!
//! route customers list
//!   path "/customers"
//!   to customers.list
//!   guard
//!     policy @policy.staff
//!     forbid_when @scope.suspended dispatch_to "/billing"
//!   loader customers.list
//!   pending_view CustomersListPending
//!
//! experience customer_management
//!   view list
//!     anchor customer.list
//!     blocks list
//!     actions create_customer
//!
//! surface customer_management web
//!   audience admin
//!     view list
//!       view_type Table
//!       columns name, email, owner
//! ```
//!
//! Closed catalogs to know about:
//! - `LzxPlatform`: `web | mobile`. New platforms require a proposal.
//! - `LzxResumeArmKind`: arm match in a resume router
//!   (`State(<name>) | None | Wildcard`).
//! - `LzxViewTestAssertion`: surface AST mirror of
//!   `lazuli_ir::ViewTestAssertion` — only `accepted by <feature>` /
//!   `rejected by <feature>` are admissible at parse time (Wave 4).

use serde::{Deserialize, Serialize};

use super::{RouteParamAst, Span};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxDocument {
    pub app: Option<LzxApp>,
    pub routes: Vec<LzxRoute>,
    pub experiences: Vec<LzxExperience>,
    pub surfaces: Vec<LzxSurface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxApp {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub targets: Vec<String>,
    pub default_locale: Option<String>,
    pub default_timezone: Option<String>,
    pub auth_failed_redirect: Option<String>,
    pub route_guard: Option<LzxRouteGuardDefaults>,
    pub actor_query: Option<String>,
    pub not_found: Option<String>,
    pub error_pages: Vec<LzxErrorPage>,
    pub uses: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteGuardDefaults {
    pub default_policy: Option<String>,
    pub on_unauthenticated: Option<String>,
    pub on_unauthorized: Option<String>,
    pub skeleton: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewGuard {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy: Vec<String>,
    pub on_unauthenticated: Option<String>,
    pub on_unauthorized: Option<String>,
    pub requires_lifecycle: Option<LzxRequiresLifecycle>,
    pub on_lifecycle_pending: Option<String>,
    /// router-w3 Tier 3 — `forbid_when <atom> dispatch_to "<url>"`
    /// children. Ordered; codegen emits checks in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid_when: Vec<LzxForbidWhen>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxForbidWhen {
    pub atom_ref: String,
    pub dispatch_to: String,
    pub span: Span,
}

/// router-w5 — `loader <feature>.<query>` slot under a route block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteLoader {
    pub feature: String,
    pub query: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresLifecycle {
    pub resource: String,
    pub state: String,
    pub substep: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxErrorPage {
    pub status: u16,
    pub template: String,
    pub audience: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRoute {
    pub name: String,
    pub path: Option<String>,
    pub routes: Vec<String>,
    pub to: Option<String>,
    pub surface: Option<String>,
    pub audience: Option<String>,
    pub lazy: Option<bool>,
    pub prerender: Option<String>,
    pub guard: Option<LzxViewGuard>,
    /// router-w5 — `loader <feature>.<query>` declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loaders: Vec<LzxRouteLoader>,
    /// router-w6 — `pending_view <component_key>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_view: Option<String>,
    /// router-w6 — `error_view <component_key>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_view: Option<String>,
    /// router-w8 — `parent <route_name>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Wave §2 (2026-05-24) — typed path-param declarations on the
    /// route block. Authored as `route <name>: <Type>` (e.g.
    /// `route id: ID`). Surfaced in `ir::AppRoute.route_params`;
    /// codegen emits a typed `parse<Route>Params` per app-level
    /// route, replacing the manual `Number(params.id)` coercion at
    /// the consumer site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_params: Vec<RouteParamAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperience {
    pub name: String,
    pub imports: Vec<String>,
    pub views: Vec<LzxExperienceView>,
    pub resume_routers: Vec<LzxResumeRouter>,
    pub extensions: Vec<LzxViewExtension>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeRouter {
    pub name: String,
    pub source_query: String,
    pub arms: Vec<LzxResumeArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeArm {
    pub kind: LzxResumeArmKind,
    pub substep: Option<String>,
    pub target_view: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LzxResumeArmKind {
    State(String),
    None,
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperienceView {
    pub name: String,
    pub anchor: Option<String>,
    pub routes: Vec<String>,
    pub extensible_by: Vec<String>,
    pub source: Option<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub actions: Vec<LzxAction>,
    pub opens: Vec<String>,
    /// Wave 4 — typed view test assertions parsed from the `tests` block.
    /// Only `accepted by <feature>` / `rejected by <feature>` shapes are
    /// admissible; the parser rejects any other line as a `ParseError`.
    pub tests: Vec<LzxViewTestAssertion>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}

/// Wave 4 — surface-AST mirror of `lazuli_ir::ViewTestAssertion`. The
/// analyzer lowers each variant 1:1 to the IR enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LzxViewTestAssertion {
    AcceptedBy { feature: String, span: Span },
    RejectedBy { feature: String, span: Span },
}

impl LzxViewTestAssertion {
    pub fn feature(&self) -> &str {
        match self {
            LzxViewTestAssertion::AcceptedBy { feature, .. }
            | LzxViewTestAssertion::RejectedBy { feature, .. } => feature,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            LzxViewTestAssertion::AcceptedBy { span, .. }
            | LzxViewTestAssertion::RejectedBy { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAction {
    pub name: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewExtension {
    pub anchor: String,
    pub blocks: Vec<String>,
    pub slots: Vec<LzxExtensionSlot>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionSlot {
    pub name: String,
    pub order: Option<LzxExtensionOrder>,
    pub blocks: Vec<String>,
    pub platforms: Vec<String>,
    pub audiences: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionOrder {
    pub relation: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxSurface {
    pub experience: String,
    pub platform: LzxPlatform,
    pub uses_experience: Option<String>,
    pub audiences: Vec<LzxAudience>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LzxPlatform {
    Web,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAudience {
    pub name: String,
    pub qualifiers: Vec<String>,
    pub views: Vec<LzxPlatformView>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxPlatformView {
    pub name: String,
    pub view_type: String,
    pub columns: Vec<String>,
    pub fields: Vec<String>,
    pub sections: Vec<String>,
    pub search: Vec<String>,
    pub filter: Vec<String>,
    pub cells: Vec<String>,
    pub actions: Vec<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}
