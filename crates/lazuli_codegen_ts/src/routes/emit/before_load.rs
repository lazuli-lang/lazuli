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
    if spec.guard_emit.is_none() && spec.lifecycle_emit.is_none() {
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
        if !guard.forbid_when.is_empty() {
            out.push_str("      const __forbidActor = await options.client.resolveActor();\n");
            out.push_str("      if (__forbidActor) {\n");
            for fw in &guard.forbid_when {
                out.push_str(&format!(
                    "        if (evaluatePolicy(__forbidActor, {{ name: \"@{ns}.{name}\", atoms: [{{ namespace: {ns:?}, name: {name:?} }}] }}) === \"authorized\") {{\n",
                    ns = fw.atom_namespace,
                    name = fw.atom_name,
                ));
                out.push_str(&format!(
                    "          throw redirect({{ to: {:?} }});\n",
                    fw.dispatch_to
                ));
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
    out.push_str("    },\n");
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
            let parts: Vec<String> = children
                .iter()
                .map(|c| render(c, children_of))
                .collect();
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
