//! Cell C.1 — audience-scoped SDK projection per L0 #3 §7.
//!
//! Given a frontend's audience declarations (each carrying a set of
//! `requires @scope.X` atoms), compute which commands/queries are
//! reachable from any of those audiences and emit a filtered
//! `<feat>.gen.ts`. The filter is enforced **at compile time** by the
//! generated TypeScript: a `public` frontend bundle simply does not
//! `export` admin-only mutations, so any import attempt fails the
//! `tsc` build.
//!
//! The audience<->command match is a set intersection of policy atoms.
//! For a command whose effective policy resolves to
//! `{@scope.workspace_admin}` and an audience block declaring
//! `requires @scope.workspace_admin`, the intersection is non-empty
//! and the command is admitted. If the audience required only
//! `@scope.workspace_member` instead, the intersection is empty and
//! the command is dropped.
//!
//! ## Query handling
//!
//! `lazuli_ir::Query` (and `RuntimeQuery`) does not carry an explicit
//! read-policy in v0 — `query.list` / `query.lookup` / `query.sql` are
//! universally readable inside a tenant, with row-level scoping handled
//! via `scope <predicate>` rather than gate atoms. Until policy gates
//! exist on queries, every query in the feature is admitted whenever
//! the audience set is non-empty, and the projection drops queries only
//! when the audience set is empty (matching the "Empty audience list
//! returns empty projection" test).
//!
//! When `Query.policy` lands in a future cell, the same intersection
//! algorithm flips on automatically — there is one place
//! (`policy_atoms_for_query`) to wire it.
//!
//! ## Module layout
//!
//! Projection (compute + filtered emission) lives in this file. The
//! sibling `route_guard` module carries every route-guard-related
//! emitter and helper.

mod route_guard;

use std::collections::BTreeSet;

use lazuli_codegen_spec::{RuntimeCommand, RuntimeFeature, RuntimeQuery};

pub use crate::lifecycle_gate_emit::{
    LifecycleGateIntegration, LifecycleGateTarget, emit_lifecycle_gate_artifacts,
    emit_lifecycle_gate_artifacts_from_json,
};
use crate::lzx_audience_slot::ir::Audience;
use crate::runtime::emit_feature_ts;

pub use route_guard::{RouteGuardTarget, emit_route_guard_artifacts};

/// Projection of the per-frontend audience set onto a feature's
/// SDK surface. `audiences` lists the audience names that drove the
/// projection (sorted, dedup'd); `allowed_commands` / `allowed_queries`
/// are the short-name sets retained from the feature.
///
/// The two `BTreeSet<String>` slots store the **short names** of the
/// commands/queries as authored in the DSL (e.g. `"create"`,
/// `"update_email"`, `"list"`, `"by_id"`). Using `BTreeSet` is
/// deliberate: it gives stable iteration order so two projections
/// computed with the same inputs serialize byte-for-byte identically
/// regardless of the order in which audiences were declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceProjection {
    /// Sorted, dedup'd audience names that participated in the
    /// projection. Empty when `audiences` parameter was empty.
    pub audiences: Vec<String>,
    /// Short names of commands admitted by at least one audience.
    pub allowed_commands: BTreeSet<String>,
    /// Short names of queries admitted by at least one audience.
    pub allowed_queries: BTreeSet<String>,
}

impl AudienceProjection {
    /// `true` when the projection admits nothing. Used by the doctor
    /// rule `AUDIENCE-EMPTY-SDK` (§11) once it lands; also exposed
    /// for the deterministic-empty tests.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_ts::lzx_audience_slot::audience_sdk::AudienceProjection;
    /// use std::collections::BTreeSet;
    /// let empty = AudienceProjection {
    ///     audiences: vec![],
    ///     allowed_commands: BTreeSet::new(),
    ///     allowed_queries: BTreeSet::new(),
    /// };
    /// assert!(empty.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.allowed_commands.is_empty() && self.allowed_queries.is_empty()
    }
}

/// Compute the projection of a feature module against a set of
/// audience declarations.
///
/// **Note on signature** — the L0 #3 §7 spec types this as
/// `audience_names: &[String]` (the list of names referenced by
/// `[frontends.X] audiences = [...]`). Resolving names to `requires`
/// atoms requires walking the `.lzx` Surface IR, which has not yet
/// landed in `lazuli_ir::Module` (parallel parser cell). Until then
/// this function takes the resolved `Audience` records directly so
/// Cell C.1 ships ahead of the parser cell. The orchestrator will add
/// a `compute_audience_projection_by_names(module, &[String])` thin
/// wrapper once the parser cell publishes audiences into `Module`.
///
/// Walks `module.commands` and `module.queries`, intersecting each
/// item's effective policy atom set against the union of every
/// audience's `requires` set. Items with any overlap are admitted.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx_audience_slot::audience_sdk::compute_audience_projection;
/// use lazuli_ir::runtime::RuntimeFeature;
/// use lazuli_ir::Audience;
///
/// let module: RuntimeFeature = /* … */ unimplemented!();
/// let audiences: Vec<Audience> = vec![];
/// let projection = compute_audience_projection(&module, &audiences);
/// assert!(projection.is_empty());
/// ```
pub fn compute_audience_projection(
    module: &RuntimeFeature,
    audiences: &[Audience],
) -> AudienceProjection {
    // Audience names — sorted + dedup'd for deterministic output.
    let mut audience_names: BTreeSet<String> = BTreeSet::new();
    for aud in audiences {
        audience_names.insert(aud.name.clone());
    }
    let audience_names: Vec<String> = audience_names.into_iter().collect();

    // Empty audiences → empty projection. Two callsites depend on this
    // contract: (a) the "Empty audience list" test below, and (b) the
    // future `AUDIENCE-EMPTY-SDK` doctor warning. Treat an empty set
    // as "this frontend exposes nothing" rather than "wildcard".
    if audiences.is_empty() {
        return AudienceProjection {
            audiences: audience_names,
            allowed_commands: BTreeSet::new(),
            allowed_queries: BTreeSet::new(),
        };
    }

    // Union of all required atoms across audiences (OR semantics per
    // §7.2). Stored as canonical `@namespace.name` strings so we can
    // compare against `RuntimeCommand.policy_atoms` (which carries
    // `(namespace, name)` tuples).
    let required: BTreeSet<String> = audiences
        .iter()
        .flat_map(|a| {
            a.requires
                .iter()
                .map(|p| format!("@{}.{}", p.namespace, p.name))
        })
        .collect();

    // Walk commands. Empty required set with non-empty audiences (an
    // audience block with no `requires` atoms) admits nothing — that's
    // an authoring smell, but we don't speculate beyond the projection.
    let mut allowed_commands = BTreeSet::new();
    for command in &module.commands {
        let command_atoms = policy_atoms_for_command(command);
        if command_atoms.is_empty() {
            // Command with no policy gate (e.g. `policy @policy.none`
            // or omitted) — universally admitted for any non-empty
            // audience. Matches the runtime spec semantics: such
            // commands carry no gate and are tenant-scoped only.
            allowed_commands.insert(command.short_name.clone());
            continue;
        }
        if command_atoms.iter().any(|atom| required.contains(atom)) {
            allowed_commands.insert(command.short_name.clone());
        }
    }

    // Walk queries. v0 IR has no `Query.policy`; treat all queries as
    // universally admitted when audiences is non-empty. See module
    // docstring for the upgrade path.
    let mut allowed_queries = BTreeSet::new();
    for query in &module.queries {
        let query_atoms = policy_atoms_for_query(query);
        if query_atoms.is_empty() {
            allowed_queries.insert(query.short_name.clone());
            continue;
        }
        if query_atoms.iter().any(|atom| required.contains(atom)) {
            allowed_queries.insert(query.short_name.clone());
        }
    }

    AudienceProjection {
        audiences: audience_names,
        allowed_commands,
        allowed_queries,
    }
}

/// Resolve the canonical `@namespace.name` atom strings on a command.
/// `RuntimeCommand.policy_atoms: Vec<(namespace, name)>` is already the
/// resolved form — the analyzer expanded `policy @policy.admin_only` to
/// its underlying scope/role atoms before lowering. We just stringify.
fn policy_atoms_for_command(command: &RuntimeCommand) -> Vec<String> {
    command
        .policy_atoms
        .iter()
        .map(|(ns, name)| format!("@{}.{}", ns, name))
        .collect()
}

/// Resolve policy atoms on a query. v0 IR carries `policy_atoms` on
/// the runtime spec (mirroring commands) for forward-compatibility,
/// even though the analyzer leaves it empty for query.list/lookup/sql
/// today. When the parser starts populating query policy, this
/// function picks it up with no further changes.
fn policy_atoms_for_query(query: &RuntimeQuery) -> Vec<String> {
    query
        .policy_atoms
        .iter()
        .map(|(ns, name)| format!("@{}.{}", ns, name))
        .collect()
}

/// Emit a feature's `<feat>.gen.ts` with the projection applied. The
/// output is structurally identical to `emit_feature_ts(feature)` but
/// with commands/queries NOT in `projection.allowed_*` filtered out
/// before emission.
///
/// Filtering happens at the spec level (not via string editing) so
/// determinism is preserved end-to-end: identical projections produce
/// identical bytes regardless of the input feature's command order.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx_audience_slot::audience_sdk::{
///     compute_audience_projection, emit_feature_sdk_filtered,
/// };
/// use lazuli_ir::runtime::RuntimeFeature;
/// use lazuli_ir::Audience;
///
/// let feature: RuntimeFeature = /* … */ unimplemented!();
/// let audiences: Vec<Audience> = vec![];
/// let projection = compute_audience_projection(&feature, &audiences);
/// let _src = emit_feature_sdk_filtered(&feature, &projection);
/// ```
pub fn emit_feature_sdk_filtered(
    feature: &RuntimeFeature,
    projection: &AudienceProjection,
) -> String {
    let filtered = RuntimeFeature {
        name: feature.name.clone(),
        source_path: feature.source_path.clone(),
        resources: feature.resources.clone(),
        commands: feature
            .commands
            .iter()
            .filter(|c| projection.allowed_commands.contains(&c.short_name))
            .cloned()
            .collect(),
        queries: feature
            .queries
            .iter()
            .filter(|q| projection.allowed_queries.contains(&q.short_name))
            .cloned()
            .collect(),
    };
    emit_feature_ts(&filtered)
}

// ---------------------------------------------------------------------------
// Tests — Cell C.1 ships ≥6 tests covering admission, exclusion, union,
// empty, filtered emission, and determinism.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    include!("audience_sdk_tests.rs");
}
