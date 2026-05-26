//! App-level route IR — declarative routing surface authored under
//! `experience { route … }` or `app { route … }`.
//!
//! A [`AppRoute`] is the lowered shape of a single route declaration: its
//! URL `path`, the audience/surface it targets, the loaders TanStack
//! prefetches, the pending/error component keys, the parent route (for
//! nested trees), and the per-route guard policy.
//!
//! The language does not talk transport — TanStack Router is the current
//! Lazuli Go target, but a hypothetical Yew/Flutter runtime would re-use
//! exactly these slots without any change to the IR.
//!
//! ## Catalog
//!
//! - [`AppRoute`] — one route declaration.
//! - [`RouteLoader`] — `loader <feature>.<query>` slot.

use serde::{Deserialize, Serialize};

use crate::SpanRef;
use crate::nodes::experience::guard::ViewGuard;
use crate::nodes::surface::RouteParam;

/// One `route <name> { … }` declaration in the app's `.lzx`. Carries
/// the path pattern, typed route params, optional view/surface
/// audience binding, lazy/prerender flags, guard policy, and loader
/// references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRoute {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    /// Wave §2 (2026-05-24) — typed path-param declarations
    /// authored as `route <name>: <Type>` on the route block.
    /// Mirrors `ViewDetail.route_params`. Codegen lowers to a
    /// `parse<Route>Params(raw)` factory in the generated
    /// `routes.gen.tsx`, replacing manual `Number(params.id)`
    /// coercion at consumer sites.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_params: Vec<RouteParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lazy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerender: Option<String>,
    /// `ir-route-guards` §3.5 — per-route policy + redirects. Layer 1 of
    /// the resolution chain (view → audience → app → built-in). When
    /// `None`, the runtime falls back to the audience/app defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<ViewGuard>,
    /// router-w5 — `loader <feature>.<query>` declarations. Each entry
    /// prefetches the named query via TanStack Query's
    /// `ensureQueryData` before the route component paints. Empty when
    /// the route declares no loaders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loaders: Vec<RouteLoader>,
    /// router-w6 — `pending_view <component_key>` declaration. When
    /// set, codegen wires the named component to the route's
    /// `pendingComponent` slot. TanStack renders it while the
    /// route's loader is pending; consumers register the component
    /// in the GeneratedRouterComponents map under this key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_view: Option<String>,
    /// router-w6 — `error_view <component_key>` declaration. Wires
    /// `errorComponent` on the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_view: Option<String>,
    /// router-w8 — `parent <route_name>` declaration. When set,
    /// codegen emits `getParentRoute: () => <parent>` instead of
    /// `rootRoute`. Enables nested route trees with shared chrome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// router-w5 — `loader <feature>.<query>` slot on a route. Codegen
/// emits a parallel `ensureQueryData` for every entry so the data
/// hydrates before the route component mounts. v1 supports zero-arg
/// queries only; route-param threading is a follow-up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteLoader {
    pub feature: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_route_minimal_round_trip() {
        let v = AppRoute {
            name: "home".into(),
            path: Some("/".into()),
            routes: vec![],
            route_params: vec![],
            to: None,
            surface: None,
            audience: None,
            lazy: None,
            prerender: None,
            guard: None,
            loaders: vec![],
            pending_view: None,
            error_view: None,
            parent: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppRoute = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn app_route_carries_loaders() {
        let v = AppRoute {
            name: "list".into(),
            path: Some("/items".into()),
            routes: vec![],
            route_params: vec![],
            to: None,
            surface: None,
            audience: None,
            lazy: None,
            prerender: None,
            guard: None,
            loaders: vec![RouteLoader {
                feature: "items".into(),
                query: "all".into(),
                span_ref: None,
            }],
            pending_view: Some("ItemsSkeleton".into()),
            error_view: None,
            parent: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppRoute = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn route_loader_omits_span_ref_when_none() {
        let v = RouteLoader {
            feature: "feat".into(),
            query: "all".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"feature\":\"feat\",\"query\":\"all\"}");
    }
}
