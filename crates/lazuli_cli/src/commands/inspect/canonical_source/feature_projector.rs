//! Per-feature projector — builds one `InspectFeature` from the
//! source lines + the shared auth/tier3 lookups.
//!
//! Lifted out of the `canonical_source` god-file in the rails-style
//! R9 split. The orchestrator (`canonical_source::mod`) owns the
//! source-walking loop that calls `inspect_feature` for each lifted
//! feature block.

use std::collections::BTreeMap;

use super::super::expand_set::ExpandSet;
use super::super::projections::{
    inspect_agent_tools_projection, inspect_agents, inspect_built_in_trace_events,
    inspect_defaults, inspect_dependencies, inspect_events, inspect_expose_projection,
    inspect_external_calls, inspect_locators, inspect_notifications, inspect_policies,
    inspect_requirements, inspect_storage_projection, inspect_targets, inspect_tests, project_auth,
};
use super::super::projectors::{
    project_aggregate, project_event_group, project_job, project_webhook,
};
use super::super::security::inspect_security;
use super::super::{ContextStatus, InspectContext, InspectContextSection, InspectFeature};

use super::tier3_collect::Tier3FeatureSlice;

pub(super) fn inspect_feature(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &BTreeMap<String, Tier3FeatureSlice>,
) -> InspectFeature {
    let name = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown")
        .to_owned();
    let external_calls = inspect_external_calls(&name, lines);
    let agents = inspect_agents(lines);
    let tier3 = tier3_by_feature.get(&name);
    // Tier 4 follow-up — `policies` category lookup now reads typed
    // `Feature.policies.categories` from the Tier 3 slice. Falls back
    // to an empty map when the slice is absent (either because the
    // feature has no `policies` block, or because no expand flag
    // gated the slice collection).
    let policies: BTreeMap<String, Vec<String>> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .map(|c| (c.name.clone(), c.atoms.clone()))
                .collect()
        })
        .unwrap_or_default();
    let notifications = inspect_notifications(lines, tier3);

    let tools = expansions
        .tools
        .then(|| inspect_agent_tools_projection(&agents));

    let expose = expansions
        .expose
        .then(|| inspect_expose_projection(&name, &agents, lines));

    // Phase L — auth projection is only present when `--expand=auth`
    // is set AND the feature declared an `auth` block. Features
    // without auth omit the field entirely so consumers can distinguish
    // "no auth declared" from "auth declared but empty".
    let auth = expansions
        .auth
        .then(|| {
            auth_by_feature
                .get(&name)
                .map(|auth| project_auth(&name, auth))
        })
        .flatten();

    // Phase L Tier 2 — storage projection harvests every `@cap.File(...)`
    // site from the source text and runs each through the typed
    // analyzer pass. The projection is omitted when the feature
    // declares zero file capabilities; that distinguishes "no storage"
    // from "storage declared but empty" for downstream consumers.
    let storage = expansions
        .storage
        .then(|| inspect_storage_projection(lines))
        .filter(|s| !s.fields.is_empty() || !s.api_outputs.is_empty());

    // Phase L Tier 3 — jobs/webhooks/event_groups projections. Each is
    // present only when the matching expand flag is set AND the feature
    // actually declares the construct. Empty arrays still surface so
    // consumers can distinguish "flag not set" from "no constructs
    // declared". `tier3` is bound earlier in this function so the
    // notification projection can read typed `digest`/`throttle`.
    let jobs_projection = expansions.jobs.then(|| {
        tier3
            .map(|t| t.jobs.iter().map(project_job).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let webhooks_projection = expansions.webhooks.then(|| {
        tier3
            .map(|t| t.webhooks.iter().map(project_webhook).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let event_groups_projection = expansions.event_groups.then(|| {
        tier3
            .map(|t| {
                t.event_groups
                    .iter()
                    .map(project_event_group)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Migrations bucket cycle Route C — `--expand=migrations`. Surfaces
    // every lifted `ir::TenantMigration` on the feature.
    let tenant_migrations_projection = expansions.migrations.then(|| {
        tier3
            .map(|t| t.tenant_migrations.clone())
            .unwrap_or_default()
    });
    // CL.C.4 — `--expand=aggregates` projection.
    let aggregates_projection = expansions.aggregates.then(|| {
        tier3
            .map(|t| {
                t.aggregates
                    .iter()
                    .map(project_aggregate)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Cache bucket cycle (CL.C.3) — `--expand=caches`. Surfaces every
    // lifted feature-level `cache <name>` profile on the feature.
    // Empty arrays still surface so consumers can distinguish "flag
    // not set" from "no profiles declared".
    let caches_projection = expansions
        .caches
        .then(|| tier3.map(|t| t.caches.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=commands` projects every lifted
    // `ir::Command` on the feature. Empty arrays surface so consumers
    // distinguish "flag not set" from "no commands declared".
    let commands_projection = expansions
        .commands
        .then(|| tier3.map(|t| t.commands.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=apis` projects every lifted
    // `ir::Api` on the feature.
    let apis_projection = expansions
        .apis
        .then(|| tier3.map(|t| t.apis.clone()).unwrap_or_default());
    // Phase L Tier 4c — `--expand=resources` projects every lifted
    // `ir::Resource` on the feature.
    let resources_projection = expansions
        .resources
        .then(|| tier3.map(|t| t.resources.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=queries` projects every lifted
    // `ir::Query` on the feature.
    let queries_projection = expansions
        .queries
        .then(|| tier3.map(|t| t.queries.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=records` projects every lifted
    // `ir::Record` on the feature.
    let records_projection = expansions
        .records
        .then(|| tier3.map(|t| t.records.clone()).unwrap_or_default());
    // IR Error-Vocab (Cell PARSE-1) — `--expand=errors` projects the
    // lifted `ir::FeatureErrors` block (None when the feature has no
    // `errors` block authored). The outer `Option` is gated by the
    // expansion flag; the inner `Option` is gated by authoring.
    let errors_projection = expansions
        .errors
        .then(|| tier3.and_then(|t| t.errors.clone()))
        .flatten();

    // `knowledge <sector>` (iron-hand context) — `--expand=knowledge`
    // projects the feature intent triad (purpose + non_goals + knowledge
    // sector) from the lowered IR. Always `Some` when the flag is set so
    // consumers distinguish "flag not set" from "no intent declared".
    // Reading the on-disk `knowledge/<sector>/` vault is a later
    // concern. See `docs/proposals/knowledge-sector-field.md`.
    let knowledge_projection = expansions.knowledge.then(|| super::super::InspectKnowledge {
        purpose: tier3.and_then(|t| t.purpose.clone()),
        non_goals: tier3.map(|t| t.non_goals.clone()).unwrap_or_default(),
        sector: tier3.and_then(|t| t.knowledge.clone()),
    });

    // `cookie-sessions-child` — the security projection now reads the
    // lowered auth lookup (for the `auth.sessions.cookie` envelope), so
    // bind it before the struct literal moves `name`.
    let security_projection = expansions
        .security
        .then(|| inspect_security(lines, &name, auth_by_feature.get(&name)));

    // CUT 2 — the composite `--expand=context` section catalog. Built
    // here (before the struct literal moves `name`) by composing the
    // already-available projector outputs. Self-contained: it projects
    // the underlying data regardless of which individual sub-axes the
    // user set. See `build_context` below + `report_types/context.rs`.
    let context_projection = expansions.context.then(|| {
        build_context(
            lines,
            &name,
            tier3,
            &policies,
            auth_by_feature.get(&name),
        )
    });

    InspectFeature {
        name,
        requirements: inspect_requirements(lines),
        external_calls,
        agents,
        notifications,
        refs: expansions.refs.then(|| super::inspect_refs(lines)),
        summary: expansions.summary.then(|| super::inspect_summary(lines)),
        locators: expansions.locators.then(|| inspect_locators(lines)),
        dependencies: expansions.dependencies.then(|| inspect_dependencies(lines)),
        security: security_projection,
        defaults: expansions.defaults.then(|| inspect_defaults(lines, tier3)),
        events: expansions.events.then(|| inspect_events(lines)),
        built_in_trace_events: expansions.events.then(inspect_built_in_trace_events),
        targets: expansions.targets.then(|| inspect_targets(lines)),
        policies: expansions
            .policies
            .then(|| inspect_policies(lines, &policies, tier3)),
        tests: expansions.tests.then(|| inspect_tests(lines, &policies)),
        tools,
        expose,
        auth,
        storage,
        jobs: jobs_projection,
        webhooks: webhooks_projection,
        event_groups: event_groups_projection,
        tenant_migrations: tenant_migrations_projection,
        caches: caches_projection,
        aggregates: aggregates_projection,
        commands: commands_projection,
        apis: apis_projection,
        resources: resources_projection,
        queries: queries_projection,
        records: records_projection,
        errors: errors_projection,
        knowledge: knowledge_projection,
        context: context_projection,
    }
}

/// CUT 2 — compose the fixed "feature context" section catalog from the
/// already-available per-axis projector outputs. Zero IR change: each
/// section reuses an existing projected shape (boxed opaquely as a
/// `serde_json::Value` inside [`InspectContextSection`]) and carries a
/// [`ContextStatus`] provenance tag.
///
/// The three text-walk sections — `authorization`, `events`, `security`
/// — are tagged `derived-via-textwalk` because their projectors
/// (`policies.rs`, `events.rs`, `security.rs`) re-scan source lines
/// rather than cloning a verbatim typed-IR shape. Everything else that
/// the compiler derives is a clean `derived`; `invariants` is `derived`
/// only when a `soft_delete`/`append_only` resource is present, else
/// `absent`. `code_pointers`/`test_matrix` are `absent` (no projector),
/// `boundaries`/`performance`/`examples` are `prose`, and `decisions`
/// is `vault` (the on-disk `knowledge/decisions/` sector is not read
/// here). The catalog is fixed: every section surfaces so consumers see
/// the complete map of what the compiler can vs cannot derive.
fn build_context(
    lines: &[String],
    feature_name: &str,
    tier3: Option<&Tier3FeatureSlice>,
    policies: &BTreeMap<String, Vec<String>>,
    auth: Option<&lazuli_ir::Auth>,
) -> InspectContext {
    // --- `derived` sections: verbatim typed-IR clones from the slice. ---

    // purpose / non_goals — the feature intent fields (as in
    // `--expand=knowledge`). `derived` even when unset; the payload is
    // simply `None` if the feature declared none.
    let purpose = match tier3.and_then(|t| t.purpose.clone()) {
        Some(text) => InspectContextSection::derived_value(ContextStatus::Derived, &text),
        None => InspectContextSection::empty(ContextStatus::Derived),
    };
    let non_goals = {
        let goals = tier3.map(|t| t.non_goals.clone()).unwrap_or_default();
        if goals.is_empty() {
            InspectContextSection::empty(ContextStatus::Derived)
        } else {
            InspectContextSection::derived_value(ContextStatus::Derived, &goals)
        }
    };

    // data_model — resources + enums + records. Enums live on the
    // lowered `Resource`/`Record` graph and on the feature; the Tier 3
    // slice carries resources + records verbatim. Enums are surfaced via
    // the resources/records that reference them (the slice does not hold
    // a standalone enum vec), so the data_model payload boxes resources
    // + records together.
    let data_model = {
        let resources = tier3.map(|t| t.resources.clone()).unwrap_or_default();
        let records = tier3.map(|t| t.records.clone()).unwrap_or_default();
        InspectContextSection::derived_value(
            ContextStatus::Derived,
            &serde_json::json!({ "resources": resources, "records": records }),
        )
    };

    // operations — commands + queries + apis.
    let operations = {
        let commands = tier3.map(|t| t.commands.clone()).unwrap_or_default();
        let queries = tier3.map(|t| t.queries.clone()).unwrap_or_default();
        let apis = tier3.map(|t| t.apis.clone()).unwrap_or_default();
        InspectContextSection::derived_value(
            ContextStatus::Derived,
            &serde_json::json!({
                "commands": commands,
                "queries": queries,
                "apis": apis,
            }),
        )
    };

    // contracts — command inputs + query params + api outputs + records.
    // The full Command/Query/Api/Record shapes carry these slots; box
    // them verbatim (consumers read `.input` / `.params` / `.output`).
    let contracts = {
        let commands = tier3.map(|t| t.commands.clone()).unwrap_or_default();
        let queries = tier3.map(|t| t.queries.clone()).unwrap_or_default();
        let apis = tier3.map(|t| t.apis.clone()).unwrap_or_default();
        let records = tier3.map(|t| t.records.clone()).unwrap_or_default();
        InspectContextSection::derived_value(
            ContextStatus::Derived,
            &serde_json::json!({
                "command_inputs": commands,
                "query_params": queries,
                "api_outputs": apis,
                "records": records,
            }),
        )
    };

    // errors — `Feature.errors`. `None` when the feature declared no
    // `errors` block.
    let errors = match tier3.and_then(|t| t.errors.clone()) {
        Some(block) => InspectContextSection::derived_value(ContextStatus::Derived, &block),
        None => InspectContextSection::empty(ContextStatus::Derived),
    };

    // invariants — the resource-decorator subset (`soft_delete` /
    // `append_only` on resources) ONLY. `derived` when any present, else
    // `absent`.
    let invariants = {
        let flagged: Vec<serde_json::Value> = tier3
            .map(|t| {
                t.resources
                    .iter()
                    .filter(|r| r.soft_delete || r.append_only)
                    .map(|r| {
                        serde_json::json!({
                            "resource": r.name,
                            "soft_delete": r.soft_delete,
                            "append_only": r.append_only,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if flagged.is_empty() {
            InspectContextSection::empty(ContextStatus::Absent)
        } else {
            InspectContextSection::derived_value(
                ContextStatus::Derived,
                &serde_json::Value::Array(flagged),
            )
        }
    };

    // --- `derived-via-textwalk` sections: projectors re-scan source. ---

    // authorization — policies + auth. `inspect_policies` is a text
    // walker over command/query/transition lines (seeded by the typed
    // `policies` map); the `auth` block is the lowered IR envelope.
    let authorization = {
        let policy_projection = inspect_policies(lines, policies, tier3);
        InspectContextSection::derived_value(
            ContextStatus::DerivedViaTextWalk,
            &serde_json::json!({ "policies": policy_projection, "auth": auth }),
        )
    };

    // events — the event-decl payload projection (text walker over
    // `event`/`event_group` lines).
    let events = {
        let event_projection = inspect_events(lines);
        InspectContextSection::derived_value(ContextStatus::DerivedViaTextWalk, &event_projection)
    };

    // security — the per-feature security envelope (text walker over
    // field/op markers, with the lowered cookie envelope folded in).
    let security = {
        let security_projection = inspect_security(lines, feature_name, auth);
        InspectContextSection::derived_value(
            ContextStatus::DerivedViaTextWalk,
            &security_projection,
        )
    };

    // --- catalog slots the compiler cannot (or does not here) derive. ---

    // code_pointers / test_matrix — no feature-level file:line table and
    // no projected coverage layers exist, so enumerate them `absent`.
    let code_pointers = InspectContextSection::empty(ContextStatus::Absent);
    let test_matrix = InspectContextSection::empty(ContextStatus::Absent);

    // boundaries / performance / examples — human prose; lives in the
    // co-located `.ctx.md`, not derivable.
    let boundaries = InspectContextSection::empty(ContextStatus::Prose);
    let performance = InspectContextSection::empty(ContextStatus::Prose);
    let examples = InspectContextSection::empty(ContextStatus::Prose);

    // decisions — the `knowledge/decisions/` vault sector, not read in
    // this projection.
    let decisions = InspectContextSection::empty(ContextStatus::Vault);

    InspectContext {
        purpose,
        non_goals,
        data_model,
        operations,
        contracts,
        errors,
        authorization,
        events,
        security,
        invariants,
        code_pointers,
        test_matrix,
        boundaries,
        performance,
        examples,
        decisions,
    }
}
