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
    /// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — row-field
    /// predicate gates resolved against `lookup_my_<resource>` queries.
    /// Each entry emits a `fetchQuery + redirect-on-mismatch` branch
    /// chained after the lifecycle gate; reuses `__row` only when its
    /// `lookup_export` matches the lifecycle gate's (same query).
    pub(super) field_gates: Vec<FieldGateEmit>,
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
    /// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — when the
    /// authored `forbid_when` slot composed an `only_when lifecycle <R>
    /// = <state>` sub-slot, codegen wraps the atom-match redirect in a
    /// `if (lifecycleState === <state>) { … }` so the dispatch fires
    /// ONLY when BOTH the atom is satisfied AND the lifecycle state
    /// matches. None ⇒ legacy unconditional behavior.
    pub(super) only_when_lifecycle: Option<OnlyWhenLifecycleEmit>,
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — resolved
/// `only_when lifecycle <R> = <state>` sub-slot for `forbid_when`.
/// Carries the lifecycle gate references so the conditional branch
/// can fetch the lookup query if no top-level lifecycle gate already
/// did so on this route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OnlyWhenLifecycleEmit {
    pub(super) feature: String,
    pub(super) lookup_export: String,
    pub(super) required_state: String,
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
    /// redirect via the helper. Empty when [`Self::allowed_states`]
    /// is set (the two forms are mutually exclusive per IR doctor rule
    /// `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`).
    pub(super) required_state: String,
    /// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 —
    /// allow-list lifecycle states. When `Some`, codegen emits an
    /// `Array.includes` check against the listed states instead of the
    /// equality check against [`Self::required_state`]. Mutually
    /// exclusive with the exact-match form: when set, `required_state`
    /// is the empty string. The IR's authored order is preserved so
    /// the emitted array is diff-stable.
    pub(super) allowed_states: Option<Vec<String>>,
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — resolved
/// `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect
/// <path>` row-field predicate. Each entry knows the feature's
/// `lookup_my_<resource>` query export, the field to read, the
/// pre-rendered TS literal for the expected value, and the redirect
/// URL fired when the field doesn't match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldGateEmit {
    /// snake_case feature name (used for the SDK import path).
    pub(super) feature: String,
    /// camelCase export name of the lookup_my_<resource> query.
    pub(super) lookup_export: String,
    /// Field name read off the fetched row.
    pub(super) field: String,
    /// Pre-rendered TS literal for the expected value (e.g. `"true"`,
    /// `"\"approved\""`, `"42"`).
    pub(super) expected_literal_ts: String,
    /// Redirect URL fired when `row.<field> !== <expected>`.
    pub(super) on_unmet_redirect: String,
}
