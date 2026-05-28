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
    use super::*;

    use lazuli_codegen_spec::{
        FieldKind, QueryKind, RuntimeArg, RuntimeCommand, RuntimeEffect, RuntimeFeature,
        RuntimeField, RuntimeInput, RuntimeQuery, RuntimeResource, Tenancy,
    };

    use crate::lzx_audience_slot::ir::{Audience, PolicyAtom, View};

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn slug_resource() -> RuntimeResource {
        RuntimeResource {
            name: "slug".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![
                RuntimeField {
                    name: "key".to_owned(),
                    kind: FieldKind::Text,
                },
                RuntimeField {
                    name: "title".to_owned(),
                    kind: FieldKind::Text,
                },
            ],
        }
    }

    fn admin_only_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.admin_only".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_admin".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn member_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.member_read".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_member".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn ungated_query(name: &str, kind: QueryKind) -> RuntimeQuery {
        RuntimeQuery {
            short_name: name.to_owned(),
            kind,
            policy_name: String::new(),
            policy_atoms: vec![],
            args: vec![RuntimeArg {
                field_name: "ID".to_owned(),
                kind: FieldKind::Integer,
                optional: false,
            }],
            cache: None,
            paginate: 0,
            filters: vec![],
            search: None,
            lookup_by: vec![],
        }
    }

    fn slug_feature() -> RuntimeFeature {
        RuntimeFeature {
            name: "slug".to_owned(),
            source_path: "features/slug/slug.lzi".to_owned(),
            resources: vec![slug_resource()],
            commands: vec![
                admin_only_command("create"),
                admin_only_command("delete"),
                member_command("rename"),
            ],
            queries: vec![
                ungated_query("list", QueryKind::List),
                ungated_query("by_key", QueryKind::Lookup),
            ],
        }
    }

    fn admin_audience() -> Audience {
        Audience {
            name: "admin".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_admin".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            ux: Default::default(),
            span_ref: None,
        }
    }

    fn public_audience() -> Audience {
        Audience {
            name: "public".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_member".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            ux: Default::default(),
            span_ref: None,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Spec §7.2 — admin audience admits a command whose effective
    /// policy resolves to `@scope.workspace_admin`.
    #[test]
    fn admin_audience_admits_admin_command() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[admin_audience()]);

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert_eq!(projection.audiences, vec!["admin".to_owned()]);
    }

    /// Spec §7.2 — public audience does NOT admit an admin_only
    /// command (intersection of required atoms is empty).
    #[test]
    fn public_audience_excludes_admin_only_command() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[public_audience()]);

        assert!(!projection.allowed_commands.contains("create"));
        assert!(!projection.allowed_commands.contains("delete"));
        assert!(
            projection.allowed_commands.contains("rename"),
            "member-gated command should be admitted for public audience"
        );
    }

    /// Multiple audiences union — admin + public together admit both
    /// admin-only and member commands. Audience names appear sorted
    /// in the projection regardless of input order.
    #[test]
    fn multiple_audiences_union_correctly() {
        let feature = slug_feature();
        let projection =
            compute_audience_projection(&feature, &[public_audience(), admin_audience()]);

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert!(projection.allowed_commands.contains("rename"));
        assert_eq!(
            projection.audiences,
            vec!["admin".to_owned(), "public".to_owned()],
            "audience names should be sorted regardless of input order"
        );
    }

    /// Empty audiences → empty projection. No commands or queries
    /// admitted. This drives the `AUDIENCE-EMPTY-SDK` doctor warning
    /// when it lands.
    #[test]
    fn empty_audience_list_returns_empty_projection() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[]);

        assert!(projection.allowed_commands.is_empty());
        assert!(projection.allowed_queries.is_empty());
        assert!(projection.audiences.is_empty());
        assert!(projection.is_empty());
    }

    /// `emit_feature_sdk_filtered` strips filtered commands from the
    /// emitted TS. The public bundle has no `deleteSlug` const because
    /// the projection excluded `delete`.
    #[test]
    fn emit_filtered_sdk_excludes_admin_commands_for_public() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[public_audience()]);
        let output = emit_feature_sdk_filtered(&feature, &projection);

        // `rename` is the only member-gated command — its identifier
        // is `renameSlug` per the runtime emitter's convention.
        assert!(
            output.contains("renameSlug"),
            "expected renameSlug to be emitted; got:\n{output}"
        );
        // `create` and `delete` MUST be absent (admin-only).
        assert!(
            !output.contains("createSlug"),
            "createSlug should be filtered out of public bundle; got:\n{output}"
        );
        assert!(
            !output.contains("deleteSlug"),
            "deleteSlug should be filtered out of public bundle; got:\n{output}"
        );
    }

    /// Deterministic emission — same projection produces identical
    /// bytes regardless of how the audiences vec is permuted.
    #[test]
    fn projection_emission_is_deterministic() {
        let feature = slug_feature();
        let proj_a = compute_audience_projection(&feature, &[admin_audience(), public_audience()]);
        let proj_b = compute_audience_projection(&feature, &[public_audience(), admin_audience()]);
        assert_eq!(proj_a, proj_b);

        let out_a = emit_feature_sdk_filtered(&feature, &proj_a);
        let out_b = emit_feature_sdk_filtered(&feature, &proj_b);
        assert_eq!(out_a, out_b);
    }

    /// All queries are admitted whenever the audience set is
    /// non-empty (v0 IR — queries have no policy gate). This locks
    /// the documented contract so a future tightening is an explicit
    /// IR change, not a silent regression.
    #[test]
    fn queries_admitted_for_any_nonempty_audience() {
        let feature = slug_feature();

        for aud in [admin_audience(), public_audience()] {
            let projection = compute_audience_projection(&feature, &[aud]);
            assert!(projection.allowed_queries.contains("list"));
            assert!(projection.allowed_queries.contains("by_key"));
        }
    }
}
