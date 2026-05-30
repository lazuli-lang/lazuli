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
//!   `lazuli_ir::ViewTestAssertion` — only `allows extension <feature>` /
//!   `denies extension <feature>` are admissible at parse time (Wave 4).

use serde::{Deserialize, Serialize};

use super::{AudienceUxAst, FilterDeclAst, RouteParamAst, Span, ViewUxAst};

/// One parsed `.lzx` document — the app's UI shell tree.
///
/// Top-level slots: optional `app` header, route declarations,
/// experience flows, and platform-specific surface declarations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxDocument {
    pub app: Option<LzxApp>,
    pub routes: Vec<LzxRoute>,
    pub experiences: Vec<LzxExperience>,
    pub surfaces: Vec<LzxSurface>,
    pub span: Span,
}

/// `app <name>` header at the top of a `.lzx` document — pins title,
/// targets, locale/timezone defaults, route-guard defaults, error pages,
/// and cross-app `uses` references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxApp {
    pub name: String,
    /// `title "..."` — human-facing app title.
    pub title: Option<String>,
    /// `version "..."` — app version string.
    pub version: Option<String>,
    /// `targets web mobile` — closed-catalog targets.
    pub targets: Vec<String>,
    /// `default_locale <BCP-47>`.
    pub default_locale: Option<String>,
    /// `default_timezone <tz>`.
    pub default_timezone: Option<String>,
    /// `auth_failed_redirect "..."` — legacy fallback (subsumed by `route_guard`).
    pub auth_failed_redirect: Option<String>,
    /// `route_guard` defaults block.
    pub route_guard: Option<LzxRouteGuardDefaults>,
    /// `actor_query <query>` — query bound to the runtime actor session.
    pub actor_query: Option<String>,
    /// `not_found "<template>"` — 404 component key.
    pub not_found: Option<String>,
    /// `error_pages` — per-status error page bindings.
    pub error_pages: Vec<LzxErrorPage>,
    /// `uses <app>` — cross-app references.
    pub uses: Vec<String>,
    pub span: Span,
}

/// `route_guard` defaults block inside [`LzxApp`] — the fallback guard
/// chain applied to every route that doesn't author its own `guard`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteGuardDefaults {
    /// `default_policy @policy.<name>` — verbatim atom.
    pub default_policy: Option<String>,
    /// `on_unauthenticated "<url>"` — redirect target.
    pub on_unauthenticated: Option<String>,
    /// `on_unauthorized "<url>"` — redirect target.
    pub on_unauthorized: Option<String>,
    /// `skeleton <component_key>` — pending-state placeholder component.
    pub skeleton: Option<String>,
    pub span: Span,
}

/// `guard` sub-block on a [`LzxRoute`] / [`LzxAudience`] / view.
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
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — allow-list
    /// variant of `requires_lifecycle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_lifecycle_in: Option<LzxRequiresLifecycleIn>,
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — row-field
    /// predicate slots; one per `requires <feature>.lookup_my.<field>
    /// = <literal> on_unmet redirect "<path>"` declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires_field: Vec<LzxRequiresField>,
    pub span: Span,
}

/// One `forbid_when @scope.<name> dispatch_to "<url>"` row inside an
/// [`LzxViewGuard`]. Codegen emits checks in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxForbidWhen {
    /// `@scope.<name>` reference verbatim.
    pub atom_ref: String,
    /// `dispatch_to "<url>"` — redirect target verbatim.
    pub dispatch_to: String,
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — optional
    /// `only_when lifecycle <R> = <state>` sub-slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_when_lifecycle: Option<LzxRequiresLifecycle>,
    pub span: Span,
}

/// `requires_lifecycle_in <Resource> [<state>, ...]` — allow-list
/// lifecycle gate per `ir-route-guard-escape-hatch-2026-05-28.md`
/// §4.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresLifecycleIn {
    pub resource: String,
    pub allowed_states: Vec<String>,
    pub span: Span,
}

/// `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect
/// "<path>"` row-field predicate per
/// `ir-route-guard-escape-hatch-2026-05-28.md` §4.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresField {
    pub feature: String,
    pub field: String,
    pub expected: LzxScalarLiteral,
    pub on_unmet_redirect: String,
    pub span: Span,
}

/// Scalar literal accepted on the right-hand side of `requires
/// <feature>.lookup_my.<field> = <literal>`. Mirrors the IR's
/// [`lazuli_ir::DefaultValue`] (minus enum literals — the route-guard
/// surface admits only primitive scalars per §4.1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LzxScalarLiteral {
    String(String),
    Integer(i64),
    Boolean(bool),
    /// Explicit `null` — distinct from a missing literal.
    Null,
}

/// router-w5 — `loader <feature>.<query>` slot under a route block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteLoader {
    pub feature: String,
    pub query: String,
    pub span: Span,
}

/// `requires_lifecycle <resource>.<state>[.<substep>]` predicate on a
/// view guard — gates the view on resource lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresLifecycle {
    pub resource: String,
    pub state: String,
    pub substep: Option<String>,
    pub span: Span,
}

/// One `error_page <status> <template> [audience <name>]` row inside [`LzxApp`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxErrorPage {
    /// HTTP status code (e.g. 403, 404, 500).
    pub status: u16,
    /// Template / component key for this status.
    pub template: String,
    /// Optional `audience <name>` scoping.
    pub audience: Option<String>,
    pub span: Span,
}

/// `route <name> ...` block inside an `.lzx` document. Drives the
/// generated TanStack / expo-router shell.
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

/// `experience <name>` block — UX flow grouping inside an `.lzx`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperience {
    pub name: String,
    /// `imports <ref>` — pulled-in experiences.
    pub imports: Vec<String>,
    pub views: Vec<LzxExperienceView>,
    pub resume_routers: Vec<LzxResumeRouter>,
    pub extensions: Vec<LzxViewExtension>,
    pub span: Span,
}

/// `resume_router <name>` block on an [`LzxExperience`] — picks the
/// initial view based on the source query's returned state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeRouter {
    pub name: String,
    /// Query that returns the resume state.
    pub source_query: String,
    pub arms: Vec<LzxResumeArm>,
    pub span: Span,
}

/// One arm of an [`LzxResumeRouter`] — `state <name>[.<substep>] -> <view>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeArm {
    pub kind: LzxResumeArmKind,
    pub substep: Option<String>,
    pub target_view: String,
    pub span: Span,
}

/// Closed three-arm catalog for a resume-router arm matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LzxResumeArmKind {
    /// `state <name>` — match a specific lifecycle state.
    State(String),
    /// `none` — match the absence of a state (no resume in progress).
    None,
    /// `*` — wildcard fallback arm.
    Wildcard,
}

/// One view declaration inside an [`LzxExperience`] — name + anchor +
/// routes + blocks + actions + guard.
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
    /// Only `allows extension <feature>` / `denies extension <feature>` shapes are
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
    AllowsExtension { feature: String, span: Span },
    DeniesExtension { feature: String, span: Span },
}

impl LzxViewTestAssertion {
    /// The feature name carried by the assertion, irrespective of variant.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{LzxViewTestAssertion, Span};
    ///
    /// let a = LzxViewTestAssertion::AllowsExtension {
    ///     feature: "customer".into(),
    ///     span: Span::new(0, 0),
    /// };
    /// assert_eq!(a.feature(), "customer");
    /// ```
    pub fn feature(&self) -> &str {
        match self {
            LzxViewTestAssertion::AllowsExtension { feature, .. }
            | LzxViewTestAssertion::DeniesExtension { feature, .. } => feature,
        }
    }

    /// Source span for diagnostics — extracts the inner span regardless
    /// of variant.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{LzxViewTestAssertion, Span};
    ///
    /// let a = LzxViewTestAssertion::DeniesExtension {
    ///     feature: "billing".into(),
    ///     span: Span::new(10, 20),
    /// };
    /// assert_eq!(a.span(), Span::new(10, 20));
    /// ```
    pub fn span(&self) -> Span {
        match self {
            LzxViewTestAssertion::AllowsExtension { span, .. }
            | LzxViewTestAssertion::DeniesExtension { span, .. } => *span,
        }
    }
}

/// One `actions <name> -> <target>` row on an [`LzxExperienceView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAction {
    pub name: String,
    /// Action target reference (qualified command/view name).
    pub target: String,
    pub span: Span,
}

/// `view_extension <anchor>` block — adds blocks/slots into an existing
/// view declared elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewExtension {
    /// Anchor identifier of the host view being extended.
    pub anchor: String,
    pub blocks: Vec<String>,
    pub slots: Vec<LzxExtensionSlot>,
    pub span: Span,
}

/// One `slot <name>` row inside a [`LzxViewExtension`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionSlot {
    pub name: String,
    pub order: Option<LzxExtensionOrder>,
    pub blocks: Vec<String>,
    pub platforms: Vec<String>,
    pub audiences: Vec<String>,
    pub span: Span,
}

/// `order <relation> <target>` clause on an [`LzxExtensionSlot`] —
/// controls placement relative to a sibling slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionOrder {
    /// `before` / `after` / `replace` — relation keyword verbatim.
    pub relation: String,
    /// Target slot identifier.
    pub target: String,
}

/// `surface <experience> <platform>` block — platform-specific
/// materialisation of an experience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxSurface {
    pub experience: String,
    pub platform: LzxPlatform,
    pub uses_experience: Option<String>,
    pub audiences: Vec<LzxAudience>,
    pub span: Span,
}

/// Closed two-arm catalog of `.lzx` platforms. New platforms require a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LzxPlatform {
    /// `web` — React / browser target.
    Web,
    /// `mobile` — React Native / Expo target.
    Mobile,
}

/// `audience <name>` block on an [`LzxSurface`] — UI scoping by policy
/// audience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAudience {
    pub name: String,
    pub qualifiers: Vec<String>,
    pub views: Vec<LzxPlatformView>,
    pub guard: Option<LzxViewGuard>,
    /// §7a audience-level UX containers (`tabs`, `wizard <name> steps`).
    /// Shares the surface-dialect AST (`AudienceUxAst`) so both `.lzx`
    /// dialects lower the same primitives.
    #[serde(default, skip_serializing_if = "AudienceUxAst::is_empty")]
    pub ux: AudienceUxAst,
    pub span: Span,
}

/// One view declaration inside an [`LzxAudience`] — platform-flavored
/// view contract (columns / fields / sections / cells / actions /
/// search / filter / submit / blocks / guard).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxPlatformView {
    pub name: String,
    /// `view_type <Table|Form|Detail|...>` — UI kind selector verbatim.
    pub view_type: String,
    pub columns: Vec<String>,
    pub fields: Vec<String>,
    pub sections: Vec<String>,
    pub search: Vec<String>,
    pub filter: Vec<String>,
    /// Typed `filters { <name>: [list of | date_range] <Type> [from query] }`
    /// block declarations (G-A1). Shares the surface-dialect AST
    /// (`FilterDeclAst`); the single-line `filter <list>` form above stays
    /// separate. Empty by default — additive over the §7a primitives F5
    /// brought into this dialect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterDeclAst>,
    pub cells: Vec<String>,
    pub actions: Vec<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub guard: Option<LzxViewGuard>,
    /// §7a view-level UX primitives (`wizard_steps`, `tab_group`,
    /// `view_mode`, `view.inline_table`, `view.board`, `repeatable
    /// input`). Shares the surface-dialect AST (`ViewUxAst`).
    #[serde(default, skip_serializing_if = "ViewUxAst::is_empty")]
    pub ux: ViewUxAst,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzx_platform_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(LzxPlatform::Web).unwrap(),
            serde_json::json!("web")
        );
    }

    #[test]
    fn lzx_resume_arm_kind_wildcard_serde_tagged() {
        let v = serde_json::to_value(LzxResumeArmKind::Wildcard).unwrap();
        assert_eq!(v["kind"], "wildcard");
    }

    #[test]
    fn lzx_view_test_assertion_feature_and_span() {
        let a = LzxViewTestAssertion::AllowsExtension {
            feature: "billing".into(),
            span: Span::new(5, 10),
        };
        assert_eq!(a.feature(), "billing");
        assert_eq!(a.span(), Span::new(5, 10));
    }
}
