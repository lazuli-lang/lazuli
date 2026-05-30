//! Lifecycle-gate TS emitter (LAZ-88 / CODEGEN-1).
//!
//! The IR/parser/analyzer cells for this proposal land in parallel, so this
//! emitter reads the additive lifecycle-gate fields through serialized IR JSON.
//! That keeps this codegen cell compiling before `ResumeRouter` and
//! `ResolvedLifecycleGate` become concrete Rust types, while still consuming the
//! same field names the final IR shape serializes.

use std::collections::BTreeMap;

use lazuli_ir::Module;
use serde_json::Value;

use crate::GeneratedFile;

mod collect;
mod emit_group;
mod helpers;
mod parse_ir;
mod registry;

use collect::{
    collect_lifecycle_gates, collect_lifecycle_resources, collect_query_resources,
    collect_resume_routers,
};
use emit_group::emit_group_file;
use parse_ir::collect_view_paths;
use registry::emit_registry_file;

/// Which platform the lifecycle-gate codegen is targeting.
///
/// Drives the `dist/` prefix and the `platform` filter applied to
/// IR-declared gates, so a single feature can carry web + mobile gates
/// without one polluting the other's output tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateTarget {
    /// Emits under `dist/ts-web/`.
    Web,
    /// Emits under `dist/ts-mobile/`.
    Mobile,
}

impl LifecycleGateTarget {
    fn dist_prefix(self) -> &'static str {
        match self {
            LifecycleGateTarget::Web => "ts-web",
            LifecycleGateTarget::Mobile => "ts-mobile",
        }
    }

    fn platform_label(self) -> &'static str {
        match self {
            LifecycleGateTarget::Web => "web",
            LifecycleGateTarget::Mobile => "mobile",
        }
    }
}

/// How the emitted gate plugs into the host router.
///
/// `TanStack` integrates with TanStack Router's loader/beforeLoad
/// callbacks; `Hoc` wraps the view component in a higher-order
/// `<Gate>` that handles redirect on its own. Pilots that don't use
/// TanStack pick `Hoc` to stay portable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateIntegration {
    /// TanStack Router `beforeLoad` integration.
    TanStack,
    /// Component-level HOC integration (`withLifecycleGate(View)`).
    Hoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceLifecycle {
    feature: String,
    name: String,
    discriminator_field: String,
    state_type: String,
    states: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeRouter {
    feature: String,
    name: String,
    resource: ResourceLifecycle,
    source_feature: String,
    source_query: String,
    source_query_ident: String,
    arms: BTreeMap<String, String>,
    none_target: String,
    wildcard_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleGate {
    feature: String,
    platform: String,
    audience: String,
    view_name: String,
    path: String,
    component: String,
    route_const: String,
    resource: String,
    expected_state: String,
    expected_substep: Option<String>,
    resume_feature: String,
    resume_name: String,
    guard: RouteGuardShape,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RouteGuardShape {
    name: Option<String>,
    atoms: Vec<PolicyAtom>,
    on_unauthenticated: Option<String>,
    on_unauthorized: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyAtom {
    namespace: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeRef {
    feature: String,
    name: String,
}

/// Emit lifecycle-gate artifacts from the typed module.
///
/// Currently a thin shim that serializes the module to JSON and delegates
/// to [`emit_lifecycle_gate_artifacts_from_json`]; the indirection is
/// deliberate while LAZ-85/86/87 land the strongly-typed IR fields in
/// parallel cells. Returns an empty vector if the module declares no
/// resume routers or gates.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lifecycle_gate_emit::{
///     emit_lifecycle_gate_artifacts, LifecycleGateTarget, LifecycleGateIntegration,
/// };
/// use lazuli_ir::Module;
///
/// let module: Module = /* obtain from analyzer */ unimplemented!();
/// let files = emit_lifecycle_gate_artifacts(
///     &module,
///     LifecycleGateTarget::Web,
///     LifecycleGateIntegration::TanStack,
/// );
/// ```
pub fn emit_lifecycle_gate_artifacts(
    module: &Module,
    target: LifecycleGateTarget,
    integration: LifecycleGateIntegration,
) -> Vec<GeneratedFile> {
    let Ok(value) = serde_json::to_value(module) else {
        return Vec::new();
    };
    emit_lifecycle_gate_artifacts_from_json(&value, target, integration)
}

/// Same emitter, entered through a raw IR JSON value.
///
/// Exists so unit tests and the parallel IR-cell window can drive the
/// emitter without going through `Module` (which may not yet carry the
/// new fields). Production callers should prefer
/// [`emit_lifecycle_gate_artifacts`].
///
/// ## Examples
///
/// ```no_run
/// use lazuli_codegen_ts::lifecycle_gate_emit::{
///     emit_lifecycle_gate_artifacts_from_json, LifecycleGateTarget, LifecycleGateIntegration,
/// };
/// use serde_json::json;
///
/// let root = json!({ "features": [] });
/// let files = emit_lifecycle_gate_artifacts_from_json(
///     &root,
///     LifecycleGateTarget::Web,
///     LifecycleGateIntegration::Hoc,
/// );
/// assert!(files.is_empty());
/// ```
pub fn emit_lifecycle_gate_artifacts_from_json(
    root: &Value,
    target: LifecycleGateTarget,
    integration: LifecycleGateIntegration,
) -> Vec<GeneratedFile> {
    let view_paths = collect_view_paths(root);
    let resources = collect_lifecycle_resources(root);
    let query_resources = collect_query_resources(root, &resources);
    let resumes = collect_resume_routers(root, &resources, &query_resources, &view_paths);
    let gates = collect_lifecycle_gates(root, target, &view_paths);

    if resumes.is_empty() && gates.is_empty() {
        return Vec::new();
    }

    let resume_by_key: BTreeMap<(String, String), ResumeRouter> = resumes
        .iter()
        .map(|resume| {
            (
                (resume.feature.clone(), resume.name.clone()),
                resume.clone(),
            )
        })
        .collect();

    let mut groups: BTreeMap<(String, String, String), Vec<LifecycleGate>> = BTreeMap::new();
    for gate in gates {
        if gate.platform != target.platform_label() {
            continue;
        }
        groups
            .entry((
                gate.feature.clone(),
                gate.platform.clone(),
                gate.audience.clone(),
            ))
            .or_default()
            .push(gate);
    }

    for gates in groups.values_mut() {
        gates.sort_by(|a, b| a.route_const.cmp(&b.route_const));
        gates.dedup_by(|a, b| a.route_const == b.route_const);
    }

    let mut files = Vec::new();
    for ((feature, platform, audience), gates) in &groups {
        let mut used_resumes = BTreeMap::new();
        for gate in gates {
            if let Some(resume) =
                resume_by_key.get(&(gate.resume_feature.clone(), gate.resume_name.clone()))
            {
                used_resumes.insert(
                    (resume.feature.clone(), resume.name.clone()),
                    resume.clone(),
                );
            }
        }
        if used_resumes.is_empty() {
            continue;
        }
        files.push(GeneratedFile {
            path: format!(
                "dist/{}/{}/{}.{}.{}.gen.ts",
                target.dist_prefix(),
                feature,
                feature,
                platform,
                audience
            ),
            contents: emit_group_file(
                feature,
                gates,
                used_resumes
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
                    .as_slice(),
                integration,
            ),
        });
    }

    if !groups.is_empty() {
        let all_gates: Vec<LifecycleGate> = groups
            .values()
            .flat_map(|gates| gates.iter().cloned())
            .collect();
        files.push(GeneratedFile {
            path: format!("dist/{}/app/lifecycle_gates.gen.ts", target.dist_prefix()),
            contents: emit_registry_file(&all_gates),
        });
    }

    files
}

// Collectors, parse_ir helpers, emit_group writers, registry, and
// small JSON / name helpers live in sibling modules. See the `mod`
// declarations at the top of this file for the wiring.

#[cfg(test)]
mod tests {
    use super::helpers::query_ident;

    // Verb-prefix dedup — confirms the lifecycle-gate's query_ident
    // produces `lookupMyHost` (not `lookupHostByLookupMyHost`) when the
    // `[me]` synth feeds the gate a `lookup_my_<r>` query name. The
    // golden fixture in `tests/lifecycle_gate_golden.rs` still uses the
    // hand-crafted `my_host` shape, which is exercised by
    // `query_ident_preserves_legacy_shape_without_verb_prefix` below.

    #[test]
    fn query_ident_dedups_lookup_prefix() {
        assert_eq!(query_ident("host", "lookup_my_host"), "lookupMyHost");
        assert_eq!(query_ident("traveler", "lookup_traveler"), "lookupTraveler");
        assert_eq!(
            query_ident("user", "lookup_active_users"),
            "lookupActiveUsers"
        );
    }

    #[test]
    fn query_ident_preserves_legacy_shape_without_verb_prefix() {
        // Hand-crafted `my_host` (no `lookup_` prefix) — used by the
        // lifecycle-gate golden test.
        assert_eq!(query_ident("host", "my_host"), "lookupHostByMyHost");
        // `by_<key>` keeps its established strip.
        assert_eq!(query_ident("slug", "by_key"), "lookupSlugByKey");
        // Bare `lookup` falls back to legacy shape (no underscore tail).
        assert_eq!(query_ident("user", "lookup"), "lookupUserByLookup");
    }
}
