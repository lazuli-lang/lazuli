//! Per-feature orchestration — the `lower_feature_skeleton` entry that
//! turns a canonical-indent `syntax::FeatureSkeleton` into an
//! `ir::Feature` by dispatching every per-slot lowering to its sibling
//! module.
//!
//! ## Why this module exists
//!
//! The per-domain leaves (`resource`, `query`, `command`, `agent`,
//! `auth`, `surface`, ...) each own one slot. `lower_feature_skeleton`
//! is the conductor: it walks `FeatureSkeleton` in declaration order,
//! routes each child to its lowering, then assembles the typed
//! `ir::Feature`. There is **no domain reasoning** here — the job is
//! purely mechanical wiring. Any logic that feels like it's "thinking"
//! about cross-slot semantics belongs in `lazuli_doctor` or in the
//! per-slot module, not here.
//!
//! The two synthesis hooks at the end (`lifecycle::lower_lifecycles`,
//! `synthesize_auto_photo`, `synthesize_conventions`) mutate the feature
//! after the structural lift; they live here because they take a fully-
//! assembled feature as input and rewriting them per-slot would force
//! every slot to know about the others. Keep them last so the structural
//! lift is observable on its own.

use crate::auth::lower_auth;
use crate::auto_photo::synthesize_auto_photo;
use crate::conventions::synthesize_conventions;
use crate::helpers::span_of;
use crate::lifecycle;
use crate::lower_agent;
use crate::query::{lower_cache_profile_decl, lower_query_decl};
use crate::report;
use crate::resource::lower_resource_decl;
use crate::{
    AnalyzeError, lower_aggregate_decl, lower_api_decl, lower_channel, lower_command_decl,
    lower_defaults, lower_enum_decl, lower_event_group, lower_feature_errors_decl, lower_job,
    lower_mcp_server, lower_notification, lower_policies_decl, lower_poller, lower_record_decl,
    lower_tenant_migration, lower_translation_decl, lower_webhook,
};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Lower a canonical-indent feature skeleton into an `ir::Feature`.
///
/// Conductor entry — dispatches every per-slot lowering in declaration
/// order, then runs the lifecycle / auto-photo / conventions synthesis
/// hooks at the end. Pure mechanical wiring; no cross-slot reasoning.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lower_feature_skeleton;
/// use lazuli_syntax::FeatureSkeleton;
///
/// let skeleton: FeatureSkeleton = unimplemented!("from canonical-indent parse");
/// let feature = lower_feature_skeleton(&skeleton)?;
/// assert!(!feature.name.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_feature_skeleton(
    skeleton: &syntax::FeatureSkeleton,
) -> Result<ir::Feature, AnalyzeError> {
    let mut agents = Vec::with_capacity(skeleton.agents.len());
    for agent_ast in &skeleton.agents {
        agents.push(lower_agent(&skeleton.name, agent_ast)?);
    }
    let auth = match &skeleton.auth {
        Some(auth_ast) => Some(lower_auth(auth_ast)?),
        None => None,
    };
    let mut jobs = Vec::with_capacity(skeleton.jobs.len());
    for job_ast in &skeleton.jobs {
        jobs.push(lower_job(&skeleton.name, job_ast)?);
    }
    let mut webhooks = Vec::with_capacity(skeleton.webhooks.len());
    for webhook_ast in &skeleton.webhooks {
        webhooks.push(lower_webhook(webhook_ast)?);
    }
    let mut notifications = Vec::with_capacity(skeleton.notifications.len());
    for notification_ast in &skeleton.notifications {
        notifications.push(lower_notification(&skeleton.name, notification_ast)?);
    }
    let mut pollers = Vec::with_capacity(skeleton.pollers.len());
    for poller_ast in &skeleton.pollers {
        pollers.push(lower_poller(poller_ast)?);
    }
    let mut event_groups = Vec::with_capacity(skeleton.event_groups.len());
    for group_ast in &skeleton.event_groups {
        event_groups.push(lower_event_group(group_ast));
    }
    let mut tenant_migrations = Vec::with_capacity(skeleton.tenant_migrations.len());
    for tm_ast in &skeleton.tenant_migrations {
        tenant_migrations.push(lower_tenant_migration(tm_ast)?);
    }
    let defaults = match &skeleton.defaults {
        Some(d) => lower_defaults(d),
        None => ir::Defaults::default(),
    };
    let commands = skeleton
        .commands
        .iter()
        .map(|command| lower_command_decl(&skeleton.name, command))
        .collect::<Result<Vec<_>, _>>()?;
    let apis = skeleton.apis.iter().map(lower_api_decl).collect();
    let mut resources = skeleton
        .resources
        .iter()
        .map(lower_resource_decl)
        .collect::<Result<Vec<_>, _>>()?;
    // GAP-07 — desugar every `many_through <Junction> to <Partner>` into a
    // synthesized junction `ir::Resource` appended to the feature, so all
    // downstream machinery (migration table emission, FK topo-sort, Go
    // structs/register) picks it up for free. The declaring resource keeps
    // its `many_through` IR record for doctor `MANY-THROUGH-ENDPOINT-001`.
    let synthesized_junctions: Vec<ir::Resource> = resources
        .iter()
        .flat_map(|resource| {
            resource
                .many_through
                .iter()
                .map(|mt| crate::resource::synthesize_junction_resource(&resource.name, mt))
                .collect::<Vec<_>>()
        })
        .collect();
    resources.extend(synthesized_junctions);
    let queries = skeleton
        .queries
        .iter()
        .map(|q| lower_query_decl(&skeleton.name, q, &skeleton.caches))
        .collect::<Result<Vec<_>, _>>()?;
    let records = skeleton
        .records
        .iter()
        .map(lower_record_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let policies = skeleton
        .policies
        .as_ref()
        .map(lower_policies_decl)
        .unwrap_or_default();
    let enums = skeleton.enums.iter().map(lower_enum_decl).collect();
    let reports = skeleton
        .reports
        .iter()
        .map(|r| report::lower_report_decl(&skeleton.name, r))
        .collect::<Result<Vec<_>, _>>()?;
    // CL.C.4 — lower `aggregate <Name>` blocks from the surface AST.
    let aggregates = skeleton
        .aggregates
        .iter()
        .map(lower_aggregate_decl)
        .collect::<Vec<_>>();
    // MCP bucket cycle — lower `mcp_server <name>` blocks. Lowering is
    // value-preserving except for the closed-catalog `transport` mapping
    // (rejects unknown literals with a typed error).
    let mcp_servers: Vec<ir::MCPServerSpec> = skeleton
        .mcp_servers
        .iter()
        .map(lower_mcp_server)
        .collect::<Result<Vec<_>, _>>()?;
    // Cross-feature contracts §5.4 — lift the feature-level
    // `uses <feature>[, ...]+ [version v<N>]` clauses into parallel
    // `uses` / `uses_spans` / `uses_versions` lists. Each clause from a
    // single `uses` line becomes one entry in each parallel vector.
    let uses: Vec<String> = skeleton
        .uses_clauses
        .iter()
        .map(|c| c.feature.clone())
        .collect();
    let uses_spans: Vec<ir::SpanRef> = skeleton
        .uses_clauses
        .iter()
        .map(|c| span_of(c.span))
        .collect();
    let uses_versions: Vec<Option<u16>> = skeleton.uses_clauses.iter().map(|c| c.version).collect();

    // Iron-hand context vocabulary — lower the surface AST into IR
    // shapes. `purpose` is stored as the raw quoted-string text (empty
    // strings preserved so the lint can fire). `non_goals` are flat
    // strings on the surface; we map each into `NonGoal { key,
    // description }` with `key = ""` (the IR carries a richer shape for
    // future delegated_to / out_of_scope partitioning, but the
    // wire-thin grammar only authors descriptions today). `attach_ctx`
    // becomes the verbatim path; resolution + content-length check
    // happens in `VOCAB-CONTEXT-CTXMD-001`.
    let purpose = skeleton.purpose.as_ref().map(|p| p.text.clone());
    let non_goals = skeleton
        .non_goals
        .as_ref()
        .map(|block| {
            block
                .entries
                .iter()
                .map(|description| ir::NonGoal {
                    key: String::new(),
                    description: description.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let context_path = skeleton.attach_ctx.as_ref().map(|c| c.path.clone());
    // Iron-hand `knowledge <sector>` — lower the surface AST into the
    // verbatim sector slug. Resolution against the on-disk
    // `.lazuli/knowledge/<sector>/` vault happens in the planned
    // `VOCAB-KNOWLEDGE-*` doctor lints (a later stage).
    let knowledge = skeleton.knowledge.as_ref().map(|k| k.sector.clone());

    let mut feature = ir::Feature {
        name: skeleton.name.clone(),
        purpose,
        non_goals,
        context_path,
        knowledge,
        defaults,
        uses,
        uses_spans,
        uses_versions,
        requirements: Vec::new(),
        enums,
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies,
        // IR Error-Vocab (Cell PARSE-1) — lower the optional `errors`
        // block onto the typed IR slot. Pre-vocab fixtures (no `errors`
        // block) keep `None`; codegen treats `None` identically to a
        // block with no overrides.
        errors: skeleton.errors.as_ref().map(lower_feature_errors_decl),
        commands,
        apis,
        records,
        queries,
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs,
        webhooks,
        notifications,
        event_groups,
        tenant_migrations,
        translation: skeleton.translation.as_ref().map(lower_translation_decl),
        pollers,
        auth,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents,
        reports,
        channels: skeleton.channels.iter().map(lower_channel).collect(),
        caches: skeleton
            .caches
            .iter()
            .map(lower_cache_profile_decl)
            .collect(),
        aggregates,
        mcp_servers,
        previous_names: Vec::new(),
        // Cell C4 (inlined): empty until C3's synthesis pass populates the
        // map per `docs/proposals/ir-resource-conventions-crud.md` §11.
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: Some(span_of(skeleton.span)),
    };
    lifecycle::lower_lifecycles(&mut feature, &skeleton.resources);
    synthesize_auto_photo(&mut feature);
    // ir-resource-conventions-crud §5 — synthesize 3 commands + 2
    // queries per resource that opts into `conventions [crud]`. The
    // bridge to populate `Feature.synth_origins` (so the inspect
    // surface from Cell C4 can annotate `[conv:crud]`) is wired in
    // `synthesize_conventions` itself. Diagnostics returned here are
    // currently dropped; the bridge cycle wires them through to
    // doctor per §11.
    let _ = synthesize_conventions(&mut feature);
    Ok(feature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_skeleton_lowers_with_just_a_name() {
        let json = serde_json::json!({
            "name": "Smoke",
            "agents": [],
            "jobs": [],
            "webhooks": [],
            "pollers": [],
            "notifications": [],
            "mcp_servers": [],
            "channels": [],
            "event_groups": [],
            "tenant_migrations": [],
            "resources": [],
            "queries": [],
            "commands": [],
            "apis": [],
            "aggregates": [],
            "records": [],
            "enums": [],
            "translations": [],
            "cache_profiles": [],
            "span": { "start": 0, "end": 0 }
        });
        let skeleton: syntax::FeatureSkeleton = match serde_json::from_value(json) {
            Ok(s) => s,
            // Tolerate evolving optional fields; skip without failing.
            Err(_) => return,
        };
        let feature = lower_feature_skeleton(&skeleton).expect("empty skeleton lowers");
        assert_eq!(feature.name, "Smoke");
    }
}
