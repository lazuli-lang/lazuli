//! Resolved per-route emit payloads. `RouteSpec` is the intermediate
//! shape the top-level driver builds from `AppRoute` + surfaces +
//! experiences, then hands to the `emit` pass. The nested
//! `LoaderEmit` / `GuardEmit` / `ForbidEmit` / `LifecycleEmit`
//! records carry the codegen-ready strings (camelCase exports,
//! atom decompositions, resolved feature names) so the emit pass
//! doesn't re-parse references.

use lazuli_ir::RouteParam;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteSpec {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) audience: String,
    pub(super) component_key: String,
    pub(super) route_const: String,
    /// W3 Tier 1 — when the .lzx route block declared `policy @policy.X`
    /// (plus optional `on_unauthenticated`/`on_unauthorized`), codegen
    /// emits a `beforeLoad` that calls the runtime's
    /// `tanstackBeforeLoadGuard`. None ⇒ route's `beforeLoad` falls
    /// through to `options.guards?.<key>` (consumer-supplied closure;
    /// the Wave 2 escape hatch stays available for app-bespoke logic).
    pub(super) guard_emit: Option<GuardEmit>,
    /// W3 Tier 2 + W4 — when the .lzx route block declared
    /// `requires_lifecycle <Resource> = <state>` and the resource
    /// authored a `lifecycle_routes` table, codegen chains the policy
    /// gate with a lifecycle fetch + dispatch via the per-resource
    /// helper.
    pub(super) lifecycle_emit: Option<LifecycleEmit>,
    /// router-w5 — declarative loaders. Each entry prefetches a
    /// feature-level zero-arg query via TanStack Query's
    /// `ensureQueryData`. Multiple loaders run in parallel.
    pub(super) loaders: Vec<LoaderEmit>,
    /// router-w6 — pending_view component-key. None when the route
    /// doesn't declare one. Consumers register the component in
    /// ROUTE_COMPONENTS under the same key (joins ClientComponentKey).
    pub(super) pending_component_key: Option<String>,
    /// router-w6 — error_view component-key.
    pub(super) error_component_key: Option<String>,
    /// router-w8 — parent route name (used to compute the parent's
    /// route const). None means the route mounts under rootRoute.
    pub(super) parent_route_const: Option<String>,
    /// router-w9 — `lazy true` on the route. When set, codegen emits
    /// `lazy: () => options.lazyComponents?.<key>?.()` and drops the
    /// synchronous `component` field. Consumer registers a `()
    /// => import('./Foo')` thunk in the `lazyComponents` map.
    pub(super) lazy: bool,
    /// Wave §2 — typed path-param declarations from
    /// `route <name>: <Type>` on the .lzx route block. Empty when
    /// the route declared no typed params (codegen falls back to
    /// the untyped `Record<string, string>` shape; consumers can
    /// still hand-parse if they need to).
    pub(super) route_params: Vec<RouteParam>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoaderEmit {
    pub(super) feature: String,
    /// camelCase TS export of the query (e.g. `lookupMyHost`).
    pub(super) query_export: String,
}

/// Resolved per-route guard payload for codegen. Atoms are decomposed
/// from `@<ns>.<name>` strings at IR-resolution time so the emitted
/// TS doesn't need a runtime lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardEmit {
    pub(super) policy_name: String,
    pub(super) policy_atoms: Vec<(String, String)>,
    pub(super) on_unauthenticated: Option<String>,
    pub(super) on_unauthorized: Option<String>,
    /// W3 Tier 3 — forbid_when checks that run BEFORE the main
    /// policy gate (signed-in actors who already satisfy the listed
    /// atom redirect away rather than seeing the route).
    pub(super) forbid_when: Vec<ForbidEmit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ForbidEmit {
    pub(super) atom_namespace: String,
    pub(super) atom_name: String,
    pub(super) dispatch_to: String,
}

/// W3 Tier 2 + W4 — lifecycle dispatch payload. Carries the
/// `lookup_my_<resource>` query reference + the per-resource
/// lifecycle-route helper export so the emitted beforeLoad can fetch
/// + redirect deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LifecycleEmit {
    /// snake_case feature name owning the resource (used as the
    /// import path: `../<feature>/<feature>.gen.js`).
    pub(super) feature: String,
    /// camelCase export name of the lookup_my_<resource> query.
    pub(super) lookup_export: String,
    /// camelCase export name of the lifecycle_route helper.
    pub(super) helper_export: String,
    /// Required lifecycle state. The route renders only when the
    /// actor's row reports this state; any other state triggers a
    /// redirect via the helper.
    pub(super) required_state: String,
}
