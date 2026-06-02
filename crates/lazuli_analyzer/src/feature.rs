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
use crate::helpers::span_of;
use crate::lifecycle;
use crate::lower_agent;
use crate::query::{lower_cache_profile_decl, lower_query_decl};
use crate::report;
use crate::resource::lower_resource_decl;
use crate::synthesize_conventions_with_overlays;
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
    let mut commands = skeleton
        .commands
        .iter()
        .map(|command| lower_command_decl(&skeleton.name, command))
        .collect::<Result<Vec<_>, _>>()?;
    // 0004 — apply the hoisted `defaults rate_limit` / `defaults audit`
    // onto each command. The per-command value always wins; a command
    // that declared its own `rate_limit` / `audit` (including the `audit
    // none` opt-out, which lowers to a non-default `AuditSpec`) is left
    // untouched. Resolving here — rather than at codegen — bakes the
    // inherited value into the IR `Command`, so the emitted Go is
    // byte-identical to the fully-explicit form (the migration's safety
    // anchor) and no codegen call-site has to thread the defaults.
    apply_command_defaults(&mut commands, &defaults);
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
    // Spec 0014 — derive the `tenant_scoped` / `soft_delete` flags on every
    // `restrict on_delete` guard from the referencing relation's schema, now
    // that every resource (including synthesized junctions) is present. This
    // is what guarantees the emitted `EXISTS` carries the tenant + soft-delete
    // predicates by construction rather than by the author remembering them.
    crate::resource::resolve_restrict_on_delete_scopes(&mut resources);
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
    // wire-thin grammar only authors descriptions today).
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
    // Feature context is resolved by CONVENTION — a co-located
    // `<feature>.ctx.md` sidecar next to the `.lzi` file. The retired
    // `attach_ctx` keyword no longer feeds this field; the path-aware
    // resolution lives in `resolve_ctx_convention` (the analyzer is the
    // SOLE writer of `context_path`), invoked by the path-aware callers
    // (doctor package-load). `lower_feature_skeleton` has no source path,
    // so it leaves the field `None` until convention resolution runs.
    let context_path = None;
    // Iron-hand `knowledge <sector>` — lower the surface AST into the
    // verbatim sector slug. Resolution against the on-disk
    // `knowledge/<sector>/` vault happens in the planned
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
    // Spec 0018 — collect the per-resource `crud` overlays from the
    // surface AST (analyzer-only; never reaches IR) and thread them into
    // the synth pass so the synthesized commands carry the author's
    // policy / assigns / emits / input-excludes. Resources without a
    // `crud` block are absent from the map → today's synth, byte-identical.
    let crud_overlays = crate::collect_crud_overlays(&skeleton.resources);
    let _ = synthesize_conventions_with_overlays(&mut feature, &crud_overlays);
    resolve_enum_literal_bindings(&mut feature);
    Ok(feature)
}

/// Lift hand-authored enum literals in command-effect bindings from
/// `Expr::Path` to the typed `Expr::Enum`, in SET assignments and
/// authored `where` clauses. Two forms:
///
/// 1. **Qualified** — `status = CustomerStatus.lead` →
///    `Expr::Enum { type_name: Some(CustomerStatus), variant: "lead" }`.
///    Lifted only when the head is a declared enum (and not a runtime
///    source head).
/// 2. **Bare variant** — `status = ready` →
///    `Expr::Enum { type_name: None, variant: "ready" }`. Mirrors the
///    query-filter `WAR-VOCAB-QUERY-ENUM-01` closure: a single-segment
///    identifier that is not a runtime source head and not a `let`
///    binding is a bare enum variant whose type is inferred from the
///    target column (codegen emits the TEXT const `FromConst("ready")`).
///
/// ## Why this pass exists
///
/// `lower_raw_expr` is a context-free text→IR projection: it cannot tell
/// `CustomerStatus.lead` / `ready` (enum literals) from `ctx.user` (a
/// runtime source) or a `let` name, so it lowers them ALL to
/// `Expr::Path`. Codegen's binding emitter only knows four source heads
/// (`input`/`ctx`/`target`/`route`) plus `let` names, and previously
/// fell through any other path to a silent `FromConst("<raw>")` string.
/// For a bare variant that string was actually correct
/// (`FromConst("ready")`), but for a qualified literal it was garbage
/// (`FromConst("CustomerStatus.lead")`). This pass resolves BOTH
/// legitimate enum forms to the typed `Expr::Enum` so codegen renders
/// the proper const, and leaves genuinely-unknown multi-segment heads as
/// `Expr::Path` for the `CODEGEN-UNRESOLVED-BINDING-SOURCE-001` doctor
/// rule (and the emitter's hard fallback) to reject.
fn resolve_enum_literal_bindings(feature: &mut ir::Feature) {
    let enum_names: std::collections::BTreeSet<String> =
        feature.enums.iter().map(|e| e.name.clone()).collect();
    for command in feature.commands.iter_mut() {
        let let_names: std::collections::BTreeSet<String> =
            command.lets.iter().map(|l| l.name.clone()).collect();
        match &mut command.effect {
            ir::CommandEffect::Creates(create) => {
                resolve_enum_assignments(&mut create.assignments, &enum_names, &let_names);
            }
            ir::CommandEffect::Updates(update) => {
                resolve_enum_assignments(&mut update.assignments, &enum_names, &let_names);
                resolve_enum_assignments(&mut update.where_clause, &enum_names, &let_names);
            }
            ir::CommandEffect::Deletes(delete) => {
                resolve_enum_assignments(&mut delete.where_clause, &enum_names, &let_names);
            }
            ir::CommandEffect::Reorders(_)
            | ir::CommandEffect::Returns(_)
            | ir::CommandEffect::None => {}
        }
    }
}

fn resolve_enum_assignments(
    assignments: &mut [ir::Assignment],
    enum_names: &std::collections::BTreeSet<String>,
    let_names: &std::collections::BTreeSet<String>,
) {
    const SOURCE_HEADS: [&str; 4] = ["input", "ctx", "target", "route"];
    for assignment in assignments.iter_mut() {
        let ir::Expr::Path(path) = &assignment.value else {
            continue;
        };
        let lifted = match path.segments.as_slice() {
            // Qualified `EnumType.variant` — lift only when the head is a
            // declared enum (and not a runtime source head).
            [head, variant]
                if !SOURCE_HEADS.contains(&head.as_str()) && enum_names.contains(head) =>
            {
                Some(ir::Expr::Enum(ir::EnumLiteral {
                    type_name: Some(ir::QualifiedName {
                        feature: None,
                        name: head.clone(),
                    }),
                    variant: variant.clone(),
                }))
            }
            // Bare single-segment variant `ready` — lift unless it is a
            // runtime source head or a `let`-bound name (both resolved by
            // the emitter's own dispatch). Type inferred from the column.
            [name]
                if !SOURCE_HEADS.contains(&name.as_str()) && !let_names.contains(name) =>
            {
                Some(ir::Expr::Enum(ir::EnumLiteral {
                    type_name: None,
                    variant: name.clone(),
                }))
            }
            _ => None,
        };
        if let Some(expr) = lifted {
            assignment.value = expr;
        }
    }
}

/// 0004 — apply the feature-level `defaults rate_limit` / `defaults audit`
/// hoist onto each command.
///
/// The inheritance rule mirrors `policy_for`/`tenancy`: the per-command
/// value always wins, the default only fills the gap.
///
/// * `rate_limit` — a command with no own `rate_limit` inherits
///   `defaults.rate_limit`, lowered the same way a per-command
///   `rate_limit "X"` lowers (`RateLimitSpec::from_default`). Codegen
///   only reads `RateLimitSpec.default` / `.by_env`, so the emitted Go is
///   byte-identical to the explicit form.
/// * `audit` — a command with no own `audit` inherits `defaults audit
///   default` as an `AuditSpec` with subjects `["default"]` (exactly what
///   a per-command `audit default` lowers to). A command that authored
///   any `audit ...` — including the `audit none` opt-out, which lowers to
///   a non-default `AuditSpec { subjects: ["none"] }` — keeps its own
///   value and does NOT inherit.
fn apply_command_defaults(commands: &mut [ir::Command], defaults: &ir::Defaults) {
    if defaults.rate_limit.is_none() && defaults.audit.is_none() {
        return;
    }
    for command in commands.iter_mut() {
        if command.rate_limit.is_none()
            && let Some(spec) = defaults.rate_limit.as_ref()
        {
            command.rate_limit = Some(ir::RateLimitSpec::from_default(spec.clone()));
        }
        if command.audit.is_none() {
            match &defaults.audit {
                Some(ir::DefaultsAudit::Default) => {
                    command.audit = Some(ir::AuditSpec {
                        subjects: vec!["default".to_owned()],
                        emit_to: None,
                        data_subject: None,
                        record_before: false,
                        record_after: false,
                        retain_for: None,
                        materialize: None,
                    });
                }
                None => {}
            }
        }
    }
}

/// Resolve a feature's context sidecar by CONVENTION — the co-located
/// `<feature>.ctx.md` file next to its `.lzi` source. This is the SOLE
/// resolution rule for `Feature.context_path` after the `attach_ctx`
/// keyword was retired: a SINGLE base (the `.lzi` file's directory), with
/// NO project-root fallback (the determinism win — one base, not two).
///
/// `lzi_path` is the `.lzi` source file; `feature_name` is the feature
/// declared inside it. Returns `Some(path)` when the convention file
/// exists on disk, else `None`. Path-aware callers (the doctor
/// package-load) invoke this to populate `Feature.context_path`, keeping
/// the analyzer the single owner of the resolution rule.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_analyzer::resolve_ctx_convention;
///
/// // <dir>/customer.ctx.md next to <dir>/customer.lzi, if present.
/// let resolved = resolve_ctx_convention(Path::new("features/customer/customer.lzi"), "customer");
/// ```
pub fn resolve_ctx_convention(
    lzi_path: &std::path::Path,
    feature_name: &str,
) -> Option<std::path::PathBuf> {
    let dir = lzi_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let candidate = dir.join(format!("{feature_name}.ctx.md"));
    candidate.exists().then_some(candidate)
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

    fn lower_one(source: &str) -> ir::Feature {
        let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        lower_feature_skeleton(&skeletons[0]).expect("lowers")
    }

    fn create_assignments(feature: &ir::Feature, command: &str) -> Vec<ir::Assignment> {
        let cmd = feature
            .commands
            .iter()
            .find(|c| c.name == command)
            .unwrap_or_else(|| panic!("command {command} not found"));
        match &cmd.effect {
            ir::CommandEffect::Creates(create) => create.assignments.clone(),
            other => panic!("expected creates effect, got {other:?}"),
        }
    }

    fn assigned<'a>(assignments: &'a [ir::Assignment], field: &str) -> &'a ir::Expr {
        &assignments
            .iter()
            .find(|a| a.field == field)
            .unwrap_or_else(|| panic!("no assignment for {field}"))
            .value
    }

    const ENUM_BINDING_SRC: &str = r#"
feature billing
  domain
    enum CustomerStatus
      lead
      active
  command capture_lead
    input
      name: Text required
    creates Customer
      name = input.name
      status = CustomerStatus.lead
      tier = mystery.value
      owner = ctx.user
"#;

    #[test]
    fn enum_literal_path_lifts_to_enum_expr() {
        // `status = CustomerStatus.lead` lowers to Expr::Path; the pass
        // must lift it to Expr::Enum so codegen emits the enum const
        // (not the silent `FromConst("CustomerStatus.lead")` garbage).
        let feature = lower_one(ENUM_BINDING_SRC);
        let assignments = create_assignments(&feature, "capture_lead");
        match assigned(&assignments, "status") {
            ir::Expr::Enum(literal) => {
                assert_eq!(literal.type_name.as_ref().unwrap().name, "CustomerStatus");
                assert_eq!(literal.variant, "lead");
            }
            other => panic!("expected Expr::Enum for status, got {other:?}"),
        }
    }

    #[test]
    fn unknown_head_stays_path_for_doctor_to_reject() {
        // `mystery.value` is NOT a declared enum — must stay Expr::Path so
        // CODEGEN-UNRESOLVED-BINDING-SOURCE-001 can reject it downstream.
        let feature = lower_one(ENUM_BINDING_SRC);
        let assignments = create_assignments(&feature, "capture_lead");
        assert!(matches!(assigned(&assignments, "tier"), ir::Expr::Path(_)));
    }

    #[test]
    fn source_head_path_is_not_lifted() {
        // A recognized source head (`ctx.user`) must stay Path so the
        // runtime source dispatch (FromCtx) still fires.
        let feature = lower_one(ENUM_BINDING_SRC);
        let assignments = create_assignments(&feature, "capture_lead");
        assert!(matches!(assigned(&assignments, "owner"), ir::Expr::Path(_)));
    }
}
