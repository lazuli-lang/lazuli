//! Per-feature Tier-3/Tier-4 IR slice + auth collector.
//!
//! `Tier3FeatureSlice` is the typed bundle the per-feature projector
//! reads when an `--expand=*` flag is set. `collect_tier3_by_feature_with_aliases`
//! lowers each feature once and applies the plugin-alias resolver
//! pass so `SemanticPluginType` carriers replace the
//! `UserDefined` placeholders before the projection runs.
//! `collect_auth_by_feature` is the sibling pass that powers
//! `--expand=auth` — same parse-and-lower path, different output.
//!
//! Lifted out of the `canonical_source` god-file in the rails-style
//! R9 split.

use std::collections::BTreeMap;

use crate::{plugin_manifest, plugin_semantic_resolver};

pub(in crate::commands::inspect) struct Tier3FeatureSlice {
    pub(in crate::commands::inspect) jobs: Vec<lazuli_ir::Job>,
    pub(in crate::commands::inspect) webhooks: Vec<lazuli_ir::Webhook>,
    pub(in crate::commands::inspect) event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `tenant_migration`
    /// declarations for `--expand=migrations`.
    pub(in crate::commands::inspect) tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Notifications expanded bucket cycle — lifted `notification`
    /// declarations. Powers the typed `digest`/`throttle` projection
    /// in `inspect_notifications`; the text-walker keeps owning the
    /// scalar fields so the projection stays additive.
    pub(in crate::commands::inspect) notifications: Vec<lazuli_ir::Notification>,
    /// Tier 4 follow-up — lifted `policies` block. Powers the typed
    /// `category -> atoms` lookup that `inspect_policies` and
    /// `inspect_tests` consume; retires the `collect_policy_atoms`
    /// text walker.
    pub(in crate::commands::inspect) policies: lazuli_ir::Policies,
    /// Cache bucket cycle (CL.C.3) — lifted feature-level
    /// `cache <name>` profile declarations. Powers `--expand=caches`.
    pub(in crate::commands::inspect) caches: Vec<lazuli_ir::CacheProfile>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations. Powers
    /// `--expand=aggregates`.
    pub(in crate::commands::inspect) aggregates: Vec<lazuli_ir::Aggregate>,
    /// Phase L Tier 4a — lifted feature-level `defaults` block.
    /// Powers `--expand=defaults` IR-driven projection; replaces the
    /// text-pattern walker for the canonical-indent code path.
    pub(in crate::commands::inspect) defaults: lazuli_ir::Defaults,
    /// Phase L Tier 4a — resource names lifted from
    /// `Feature.resources`. Used by `--expand=defaults` to compute
    /// `applies_to` for `tenancy`/`timestamps` defaults without
    /// re-walking the source text.
    pub(in crate::commands::inspect) resource_names: Vec<String>,
    /// Phase L Tier 4b — lifted `command <name>` declarations on the
    /// feature. Powers `--expand=commands`; emitted verbatim from IR
    /// so downstream consumers see the typed Command shape (with
    /// audit, approval, invalidates, etc.) without re-deriving from
    /// text.
    pub(in crate::commands::inspect) commands: Vec<lazuli_ir::Command>,
    /// Phase L Tier 4b — lifted `api <name>` declarations on the
    /// feature. Powers `--expand=apis` (accepting `api` or `apis`).
    pub(in crate::commands::inspect) apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4c — lifted `resource <Name>` declarations on the
    /// feature. Powers `--expand=resources`.
    pub(in crate::commands::inspect) resources: Vec<lazuli_ir::Resource>,
    /// Phase L Tier 4d — lifted `query.{list,lookup,sql}` declarations
    /// on the feature. Powers `--expand=queries`.
    pub(in crate::commands::inspect) queries: Vec<lazuli_ir::Query>,
    /// Phase L Tier 4d — lifted `record <Name>` declarations on the
    /// feature. Powers `--expand=records`.
    pub(in crate::commands::inspect) records: Vec<lazuli_ir::Record>,
    /// IR Error-Vocab (Cell PARSE-1) — lifted `errors` block. `None`
    /// when the feature declared no `errors` block. Powers
    /// `--expand=errors` projection.
    pub(in crate::commands::inspect) errors: Option<lazuli_ir::FeatureErrors>,
    /// `knowledge <sector>` (iron-hand context) — lifted `purpose`
    /// text, `non_goals` entries, and the `knowledge` sector slug. The
    /// intent triad `--expand=knowledge` projects from the lowered IR.
    /// `purpose` / `knowledge` are `None` when unset; `non_goals` is an
    /// empty vec. See `docs/proposals/knowledge-sector-field.md`.
    pub(in crate::commands::inspect) purpose: Option<String>,
    pub(in crate::commands::inspect) non_goals: Vec<lazuli_ir::NonGoal>,
    pub(in crate::commands::inspect) knowledge: Option<String>,
}

/// B3 — variant that applies the plugin alias map to lifted features
/// so inspect's `--expand=resources` projection surfaces
/// `SemanticPluginType` carriers rather than `UserDefined` placeholders.
pub(super) fn collect_tier3_by_feature_with_aliases(
    source: &str,
    alias_map: &BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
) -> BTreeMap<String, Tier3FeatureSlice> {
    let mut map = BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature_ast in features {
        let Ok(mut feature_ir) = lazuli_analyzer::lower_feature_skeleton(&feature_ast) else {
            continue;
        };
        if !alias_map.is_empty() {
            // Reuse the package-level resolver pass on this single
            // feature. Wrap in a transient module so the walker
            // signature is stable across both callers.
            let mut transient = lazuli_ir::Module {
                workspace: None,
                contracts: Vec::new(),
                app: None,
                registry: None,
                profiles: Vec::new(),
                design: None,
                rbac: None,
                features: vec![feature_ir],
            };
            plugin_semantic_resolver::apply_plugin_semantic_resolution(&mut transient, alias_map);
            // Invariant: we pushed exactly one feature above, so pop() yields Some.
            // Fall back to continuing — unreachable in practice but keeps the loop safe.
            let Some(popped) = transient.features.pop() else {
                continue;
            };
            feature_ir = popped;
        }
        map.insert(
            feature_ir.name.clone(),
            Tier3FeatureSlice {
                jobs: feature_ir.jobs,
                webhooks: feature_ir.webhooks,
                event_groups: feature_ir.event_groups,
                tenant_migrations: feature_ir.tenant_migrations,
                notifications: feature_ir.notifications,
                policies: feature_ir.policies,
                caches: feature_ir.caches,
                aggregates: feature_ir.aggregates,
                defaults: feature_ir.defaults,
                resource_names: feature_ir
                    .resources
                    .iter()
                    .map(|r| r.name.clone())
                    .collect(),
                commands: feature_ir.commands,
                apis: feature_ir.apis,
                resources: feature_ir.resources,
                queries: feature_ir.queries,
                records: feature_ir.records,
                errors: feature_ir.errors,
                purpose: feature_ir.purpose,
                non_goals: feature_ir.non_goals,
                knowledge: feature_ir.knowledge,
            },
        );
    }
    map
}

/// Phase L — run the canonical-indent slice and build a `feature_name ->
/// IR Auth` lookup. Failures in either parse or lower silently degrade
/// to an empty lookup: `--expand=auth` is a projection, not a check,
/// so it must not flip inspect into an error path.
pub(super) fn collect_auth_by_feature(source: &str) -> BTreeMap<String, lazuli_ir::Auth> {
    let mut map = BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature in features {
        if let Some(auth_ast) = feature.auth.as_ref()
            && let Ok(auth_ir) = lazuli_analyzer::lower_auth(auth_ast)
        {
            map.insert(feature.name.clone(), auth_ir);
        }
    }
    map
}
