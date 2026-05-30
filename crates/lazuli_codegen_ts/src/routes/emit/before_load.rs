//! TanStack `beforeLoad` + `loader` slot emission, plus the route-
//! tree composer.
//!
//! `emit_before_load`: combined policy + lifecycle gate. The policy
//! gate (W3 Tier 1) calls `tanstackBeforeLoadGuard` with decomposed
//! atoms; lifecycle gate (W3 Tier 2 + W4) fetches
//! `lookup_my_<resource>` and redirects via the resource's helper.
//! Both can coexist on a single route.
//!
//! `emit_loader` (router-w5): emit declarative `loader` slot when
//! the route declared one or more `loader <feature>.<query>` lines.
//!
//! `build_tree_expr` (router-w8): build the `routeTree` arg list,
//! recursively wrapping parents in
//! `<parent>.addChildren([<children>] as const)`.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::super::spec::RouteSpec;

/// W3 Tier 1/2 + W4 — decide whether to emit an inline `beforeLoad`
/// (driven by the IR-resolved guard + optional lifecycle gate) or
/// fall through to the Wave-2 escape hatch (`options.guards?.<key>`).
///
/// Combined emission ordering:
///   1. Policy gate (tanstackBeforeLoadGuard) — auth + role + redirect.
///   2. Lifecycle gate — fetch lookup_my_<resource>, redirect via
///      `<resource>LifecycleRoute(state)` when state mismatches.
///
/// Both shapes can coexist: if the route both declared `policy` in
/// .lzx AND the app passes a guard by key, the IR guard runs first;
/// the consumer's guard is never reached because TanStack's
/// beforeLoad slot is single-valued.
pub(super) fn emit_before_load(out: &mut String, spec: &RouteSpec) {
    if spec.guard_emit.is_none() && spec.lifecycle_emit.is_none() && spec.field_gates.is_empty() {
        writeln!(
            out,
            "    beforeLoad: options.guards?.{},",
            spec.component_key
        )
        .ok();
        return;
    }
    out.push_str("    beforeLoad: async (params) => {\n");
    if let Some(guard) = &spec.guard_emit {
        // W3 Tier 3 — forbid_when atoms run BEFORE the policy gate.
        // The actor is fetched once via resolveActor, then each atom
        // is checked against the closed-catalog evaluatePolicy helper.
        // A signed-out actor short-circuits to the policy gate
        // (signed-out users can't satisfy a role/scope atom).
        //
        // `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — when
        // a forbid_when slot composed `only_when lifecycle <R> =
        // <state>`, the conditional fetches the relevant lookup_my_<r>
        // row (cached via `__forbid_lifecycle_row_<feature>`) and
        // wraps the redirect in a state-equality check so the
        // dispatch fires only when BOTH the atom matches AND the
        // lifecycle state matches.
        if !guard.forbid_when.is_empty() {
            out.push_str("      const __forbidActor = await options.client.resolveActor();\n");
            out.push_str("      if (__forbidActor) {\n");
            // Hoist per-feature lifecycle-row fetches so multiple
            // forbid_when arms targeting the same resource don't
            // refetch. Authored order in the IR drives emit order.
            let mut emitted_lc_features: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for fw in &guard.forbid_when {
                if let Some(only) = &fw.only_when_lifecycle {
                    if emitted_lc_features.insert(only.feature.clone()) {
                        let cache_var = forbid_lc_row_var(&only.feature);
                        out.push_str(&format!(
                            "        const {cache} = await params.context.queryClient.fetchQuery({{\n",
                            cache = cache_var,
                        ));
                        out.push_str(&format!(
                            "          queryKey: queryKeyFor({}, {{}}),\n",
                            only.lookup_export
                        ));
                        out.push_str(&format!(
                            "          queryFn: () => params.context.client.runQuery({}, {{}}),\n",
                            only.lookup_export
                        ));
                        out.push_str("        });\n");
                        out.push_str(&format!(
                            "        const {state} = ({cache} as {{ lifecycleState?: string }}).lifecycleState ?? null;\n",
                            state = forbid_lc_state_var(&only.feature),
                            cache = cache_var,
                        ));
                    }
                }
            }
            for fw in &guard.forbid_when {
                out.push_str(&format!(
                    "        if (evaluatePolicy(__forbidActor, {{ name: \"@{ns}.{name}\", atoms: [{{ namespace: {ns:?}, name: {name:?} }}] }}) === \"authorized\") {{\n",
                    ns = fw.atom_namespace,
                    name = fw.atom_name,
                ));
                if let Some(only) = &fw.only_when_lifecycle {
                    out.push_str(&format!(
                        "          if ({state} === {required:?}) {{\n",
                        state = forbid_lc_state_var(&only.feature),
                        required = only.required_state,
                    ));
                    out.push_str(&format!(
                        "            throw redirect({{ to: {:?} }});\n",
                        fw.dispatch_to
                    ));
                    out.push_str("          }\n");
                } else {
                    out.push_str(&format!(
                        "          throw redirect({{ to: {:?} }});\n",
                        fw.dispatch_to
                    ));
                }
                out.push_str("        }\n");
            }
            out.push_str("      }\n");
        }
        let atoms_literal = guard
            .policy_atoms
            .iter()
            .map(|(ns, name)| format!("{{ namespace: {:?}, name: {:?} }}", ns, name))
            .collect::<Vec<_>>()
            .join(", ");
        let on_unauth = match guard.on_unauthenticated.as_deref() {
            Some(p) => format!("{:?}", p),
            None => "undefined".to_owned(),
        };
        let on_unauth_role = match guard.on_unauthorized.as_deref() {
            Some(p) => format!("{:?}", p),
            None => "undefined".to_owned(),
        };
        out.push_str("      await tanstackBeforeLoadGuard(options.client, {\n");
        out.push_str(&format!(
            "        policy: {{ name: {:?}, atoms: [{}] }},\n",
            guard.policy_name, atoms_literal
        ));
        out.push_str(&format!("        onUnauthenticated: {},\n", on_unauth));
        out.push_str(&format!("        onUnauthorized: {},\n", on_unauth_role));
        out.push_str("      })();\n");
    }
    if let Some(lc) = &spec.lifecycle_emit {
        // Fetch lookup_my_<resource> via the route context's
        // queryClient. The fetchQuery promise resolves (or rejects) the
        // route's beforeLoad atomically; TanStack waits for the redirect
        // throw before painting the route.
        out.push_str("      let __row;\n");
        out.push_str("      try {\n");
        out.push_str("        __row = await params.context.queryClient.fetchQuery({\n");
        out.push_str(&format!(
            "          queryKey: queryKeyFor({}, {{}}),\n",
            lc.lookup_export
        ));
        out.push_str(&format!(
            "          queryFn: () => params.context.client.runQuery({}, {{}}),\n",
            lc.lookup_export
        ));
        out.push_str("        });\n");
        out.push_str("      } catch (err) {\n");
        out.push_str("        if (isLazuliError(err) && err.status === 404) {\n");
        out.push_str(&format!(
            "          throw redirect({{ to: {}(null) }});\n",
            lc.helper_export
        ));
        out.push_str("        }\n");
        out.push_str("        throw err;\n");
        out.push_str("      }\n");
        out.push_str("      const __state = (__row as { lifecycleState?: string }).lifecycleState ?? null;\n");
        // `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 —
        // allow-list lifecycle (`requires_lifecycle_in <R> [s1, ...]`)
        // emits an `Array.includes` check instead of equality. Author
        // order is preserved so the emitted array is diff-stable.
        if let Some(allowed) = &lc.allowed_states {
            let literal = allowed
                .iter()
                .map(|s| format!("{:?}", s))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "      const __allowedStates = [{}] as const;\n",
                literal
            ));
            out.push_str(
                "      if (__state === null || !__allowedStates.includes(__state as typeof __allowedStates[number])) {\n",
            );
            out.push_str(&format!(
                "        throw redirect({{ to: {}(__state) }});\n",
                lc.helper_export
            ));
            out.push_str("      }\n");
        } else {
            out.push_str(&format!(
                "      if (__state !== {:?}) {{\n",
                lc.required_state
            ));
            out.push_str(&format!(
                "        throw redirect({{ to: {}(__state) }});\n",
                lc.helper_export
            ));
            out.push_str("      }\n");
        }
    }
    // `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 —
    // row-field predicate gates (`requires <feature>.lookup_my.<field>
    // = <literal> on_unmet redirect <path>`). Each gate fetches the
    // named lookup query (or reuses the lifecycle gate's `__row` when
    // the lookup export matches — same query, no extra round-trip)
    // and throws a redirect on field-value mismatch. Emitted in
    // authored order.
    for (idx, fg) in spec.field_gates.iter().enumerate() {
        let row_var = format!("__fieldRow{}", idx);
        let reuses_lifecycle_row = spec
            .lifecycle_emit
            .as_ref()
            .is_some_and(|lc| lc.lookup_export == fg.lookup_export);
        if reuses_lifecycle_row {
            out.push_str(&format!(
                "      const {row} = __row as {{ {field}?: unknown }};\n",
                row = row_var,
                field = fg.field
            ));
        } else {
            out.push_str(&format!(
                "      const {row} = await params.context.queryClient.fetchQuery({{\n",
                row = row_var
            ));
            out.push_str(&format!(
                "        queryKey: queryKeyFor({}, {{}}),\n",
                fg.lookup_export
            ));
            out.push_str(&format!(
                "        queryFn: () => params.context.client.runQuery({}, {{}}),\n",
                fg.lookup_export
            ));
            out.push_str("      }) as { ");
            out.push_str(&fg.field);
            out.push_str("?: unknown };\n");
        }
        out.push_str(&format!(
            "      if ({row}.{field} !== {expected}) {{\n",
            row = row_var,
            field = fg.field,
            expected = fg.expected_literal_ts,
        ));
        out.push_str(&format!(
            "        throw redirect({{ to: {:?} }});\n",
            fg.on_unmet_redirect
        ));
        out.push_str("      }\n");
    }
    out.push_str("    },\n");
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — per-feature
/// cache variable holding the lookup_my_<r> row fetched on behalf of a
/// `forbid_when ... only_when lifecycle` arm. Multiple arms targeting
/// the same feature share one fetch.
fn forbid_lc_row_var(feature: &str) -> String {
    format!("__forbidLcRow_{}", feature.replace('-', "_"))
}

/// Companion to [`forbid_lc_row_var`] — the cached `lifecycleState`.
fn forbid_lc_state_var(feature: &str) -> String {
    format!("__forbidLcState_{}", feature.replace('-', "_"))
}

/// router-w8 — build the comma-joined `routeTree` children list,
/// recursively wrapping each parent in
/// `<parent>.addChildren([<children>] as const)`. Children of a
/// parent are sorted by route_const to keep emission stable.
pub(super) fn build_tree_expr(specs: &[RouteSpec]) -> String {
    let mut children_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut top_level: Vec<String> = Vec::new();
    for spec in specs {
        if let Some(parent) = &spec.parent_route_const {
            children_of
                .entry(parent.clone())
                .or_default()
                .push(spec.route_const.clone());
        } else {
            top_level.push(spec.route_const.clone());
        }
    }
    // Sort each child list for deterministic emission.
    for children in children_of.values_mut() {
        children.sort();
    }
    fn render(route_const: &str, children_of: &BTreeMap<String, Vec<String>>) -> String {
        if let Some(children) = children_of.get(route_const) {
            let parts: Vec<String> = children.iter().map(|c| render(c, children_of)).collect();
            format!("{route_const}.addChildren([{}] as const)", parts.join(", "))
        } else {
            route_const.to_owned()
        }
    }
    top_level
        .iter()
        .map(|c| render(c, &children_of))
        .collect::<Vec<_>>()
        .join(", ")
}

/// router-w5 — emit `loader: ({ context }) => ...` when the route
/// declared one or more `loader <feature>.<query>` slots. Multiple
/// loaders run in parallel via `Promise.all`; each calls
/// `queryClient.ensureQueryData` so the data is hydrated before the
/// route component paints.
pub(super) fn emit_loader(out: &mut String, spec: &RouteSpec) {
    if spec.loaders.is_empty() {
        return;
    }
    out.push_str("    loader: async ({ context }) => {\n");
    out.push_str("      await Promise.all([\n");
    for loader in &spec.loaders {
        out.push_str(&format!(
            "        context.queryClient.ensureQueryData({{ queryKey: queryKeyFor({q}, {{}}), queryFn: () => context.client.runQuery({q}, {{}}) }}),\n",
            q = loader.query_export
        ));
    }
    out.push_str("      ]);\n");
    out.push_str("    },\n");
}
