//! Typed nav helpers + `path_params` (TanStack `$param` segment
//! extractor used by both the nav-helper signatures and the
//! `before_load` consumer).
//!
//! router-w7 — typed nav helpers: one `nav.<key>(...)` factory per
//! route, keyed by the camelCase route name. Routes with path
//! params accept a typed `params` argument (string by default,
//! tightened to `ID` / `number` / `boolean` when the .lzx declared
//! `route <name>: <Type>`). Routes without path params accept no
//! argument. Rename a route in .lzx → call sites fail typecheck.

use std::collections::BTreeMap;

use lazuli_ir::RouteParam;

use super::super::spec::RouteSpec;
use super::params::{nav_arg_needs_string_coercion, nav_arg_ts_type};
use super::ts_string;

pub(super) fn emit_nav_helpers(out: &mut String, specs: &[RouteSpec]) {
    if specs.is_empty() {
        return;
    }
    out.push_str("export const nav = {\n");
    for spec in specs {
        let params = path_params(&spec.path);
        if params.is_empty() {
            out.push_str(&format!(
                "  {}: () => ({{ to: {} as const }}),\n",
                spec.component_key,
                ts_string(&spec.path),
            ));
        } else {
            // Wave §2 — when the .lzx declared typed
            // `route <name>: <Type>` params, tighten the nav helper's
            // arg signature so callers pass `ID` / `number` etc.
            // instead of stringly-typed values. Path-only segments
            // (declared in `path "/x/:foo"` but without a
            // `route foo: ...` line) fall through to `string` (no
            // typecheck regression for routes that haven't lifted yet).
            let typed_by_name: BTreeMap<String, &RouteParam> = spec
                .route_params
                .iter()
                .map(|p| (p.name.clone(), p))
                .collect();
            let arg_fields = params
                .iter()
                .map(|p| {
                    let ty = typed_by_name
                        .get(p.as_str())
                        .map(|rp| nav_arg_ts_type(&rp.type_ref))
                        .unwrap_or_else(|| "string".to_owned());
                    format!("{p}: {ty}")
                })
                .collect::<Vec<_>>()
                .join("; ");
            // TanStack's `params` field expects strings; coerce typed
            // values to string at the wire boundary. `String(id)`
            // handles the `ID` (= number) case and is a no-op for
            // strings.
            let params_field = params
                .iter()
                .map(|p| {
                    let needs_coerce = typed_by_name
                        .get(p.as_str())
                        .is_some_and(|rp| nav_arg_needs_string_coercion(&rp.type_ref));
                    if needs_coerce {
                        format!("{p}: String(params.{p})")
                    } else {
                        format!("{p}: params.{p}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  {}: (params: {{ {} }}) => ({{ to: {} as const, params: {{ {} }} }}),\n",
                spec.component_key,
                arg_fields,
                ts_string(&spec.path),
                params_field,
            ));
        }
    }
    out.push_str("};\n");
}

/// Extract the path-param names from a TanStack-formatted path
/// (`/host/services/$id` → `["id"]`). Order preserved.
pub(super) fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(name) = segment.strip_prefix('$')
            && !name.is_empty()
        {
            out.push(name.to_owned());
        }
    }
    out
}
