//! Runtime-form TS emitter — produces a TypeScript file structurally
//! identical to the hand-written `dist/web/customer/src/customer.gen.ts`.
//! Output consumes `@lazuli/runtime`: typed `Customer` interface, one
//! `defineCommand<I, O>(name, { invalidates })` per command, and one
//! `defineQuery<A, R>(name)` per query.
//!
//! Driven by `lazuli_codegen_spec::RuntimeFeature` so both this crate and
//! `lazuli_codegen_go` consume the same canonical spec.
//!
//! ## Module layout
//!
//! - `header` — file preamble, import block, section banners, the
//!   `format_string_array` helper.
//! - `naming` — casing primitives (`pascal_case`, `lower_camel`,
//!   `field_kind_ts`); `lower_camel_export` is re-exported from the
//!   crate root for the CLI zod emitter.
//! - `resource` — `export interface <Resource> { … }` emission.
//! - `command` — `defineCommand<I, O>` + deprecation metadata.
//! - `query` — `defineQuery<A, R>` + verb/resource-suffix naming dedup.
//! - `invalidates` — cache-invalidation target list merge (author +
//!   derived).
//! - `lifecycle` — `useLazuliCommand` action maps + `router-w4`
//!   `<resource>LifecycleRoute(state)` helpers.

mod command;
mod header;
mod invalidates;
mod lifecycle;
mod naming;
mod query;
mod resource;

#[cfg(test)]
mod tests;

pub use naming::{lifecycle_route_helper_name, lower_camel_export};

use lazuli_codegen_spec::RuntimeFeature;
use lazuli_ir as ir;

use command::write_command;
use header::{write_header, write_imports};
use lifecycle::{write_lifecycle_action_maps, write_lifecycle_route_helper};
use query::write_query;
use resource::write_resource;

/// Emit the canonical `<feature>.gen.ts` for a feature. The emitted source
/// matches the layout of `dist/web/customer/src/customer.gen.ts`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::emit_feature_ts;
/// use lazuli_ir::runtime::RuntimeFeature;
///
/// let feature: RuntimeFeature = /* … */ unimplemented!();
/// let src = emit_feature_ts(&feature);
/// assert!(src.contains("export"));
/// ```
pub fn emit_feature_ts(feature: &RuntimeFeature) -> String {
    let mut s = String::new();
    write_header(&mut s, feature);
    write_imports(&mut s);
    for resource in &feature.resources {
        write_resource(&mut s, resource);
    }
    for command in &feature.commands {
        write_command(&mut s, feature, command);
    }
    for query in &feature.queries {
        write_query(&mut s, feature, query);
    }
    s
}

/// Emit the lifecycle action-map footer for an IR feature SDK.
///
/// The runtime-spec projection has not yet gained `Resource.lifecycle`; this
/// helper keeps the TS lifecycle surface tied to the canonical IR shape and can
/// be appended by the IR-backed SDK emitter when that projection is wired.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::runtime::emit_lifecycle_action_maps_ts;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = /* … */ unimplemented!();
/// let _ = emit_lifecycle_action_maps_ts(&feature);
/// ```
pub fn emit_lifecycle_action_maps_ts(feature: &ir::Feature) -> String {
    let mut s = String::new();
    write_lifecycle_action_maps(&mut s, feature);
    s
}

/// router-w4 — per-resource `<resource>LifecycleRoute(state)` helper.
/// Emitted from each `Resource.lifecycle_routes` table. The function
/// is a flat switch over the declared arms; `null`/`undefined` state
/// falls through to the `none` arm if present, otherwise to `*`.
/// Routes that declared `requires_lifecycle X = <state>` with
/// `on_lifecycle_pending dispatch_via X.lifecycle_route` consume this
/// helper from routes.gen.tsx beforeLoad closures.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::emit_lifecycle_route_helpers_ts;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = /* … */ unimplemented!();
/// if let Some(src) = emit_lifecycle_route_helpers_ts(&feature) {
///     assert!(src.contains("function"));
/// }
/// ```
pub fn emit_lifecycle_route_helpers_ts(feature: &ir::Feature) -> Option<String> {
    let resources: Vec<&ir::Resource> = feature
        .resources
        .iter()
        .filter(|r| r.lifecycle_routes.is_some())
        .collect();
    if resources.is_empty() {
        return None;
    }
    let mut s = String::new();
    s.push_str("// router-w4 — lifecycle_routes helpers. One function per\n");
    s.push_str("// resource that authored a `lifecycle_routes` block.\n");
    for resource in resources {
        // Canonical helper name (snake_case the PascalCase resource name
        // FIRST, then camelCase) — must match the route-file import side
        // in `routes::resolve`. Feeding the verbatim PascalCase name into
        // `lower_camel` preserved the leading segment and emitted
        // `HostLifecycleRoute` while the import expected `hostLifecycleRoute`
        // → 2× TS2724 on every fresh `generate ts`.
        let helper = lifecycle_route_helper_name(&resource.name);
        // Filter at line 120 guarantees lifecycle_routes is Some.
        let Some(table) = resource.lifecycle_routes.as_ref() else {
            continue;
        };
        write_lifecycle_route_helper(&mut s, &helper, table);
    }
    Some(s)
}
