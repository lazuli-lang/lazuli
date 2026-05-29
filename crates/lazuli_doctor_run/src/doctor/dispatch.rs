//! `DoctorPackage::diagnostics` — the doctor dispatcher.
//!
//! Fans out every `*_diagnostics` aggregator on the loaded
//! `DoctorPackage`, then sorts + suppresses the result.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 3.
//! The aggregators themselves live in `doctor/aggregators/*` or as
//! `pub(super) fn *_diagnostics` items in `doctor/mod.rs`.

use std::collections::BTreeSet;

use super::aggregators::cross_feature::{
    app_urls_missing_diagnostics, collect_known_audiences, scope_owner_column_diagnostics,
};
use super::aggregators::env_manifest::{
    cap_file_policy_implicit_diagnostics, dedupe_env_contract_diagnostics,
    manifest_required_diagnostics, suppress_env_schema_when_declared,
};
use super::aggregators::field_health::{
    field_derived_from_unresolved_diagnostics, resource_unique_qualifier_unknown_diagnostics,
    resource_validates_path_unknown_diagnostics,
};
use super::aggregators::runtime_version::{
    lazuli_version_001_diagnostics, lazuli_version_002_diagnostics, schema_rich_gap_diagnostics,
};
use super::aggregators::ts_consumers::{
    import_deprecated_alias_diagnostics, manual_param_coercion_diagnostics,
};
use super::package::DoctorPackage;
use super::{
    DoctorDiagnostic, DoctorSeverity, LZIR_SCHEMA, aggregators, approval_diagnostics,
    approval_missing_children_diagnostics, auth_refresh, cap_file_storage_diagnostics,
    check_auth_session_callsite_001, check_codegen_wrap_001, check_pattern_draft_stale_001,
    collect_callable_bodies_for_eval_order, collect_known_roles,
    cross_feature_type_unresolved_diagnostics, doctor_rule_path, duplicate_query_name_diagnostics,
    feature_uses_missing_diagnostics, lazurite_manifest_diagnostics, lifecycle_gate,
    missing_policy_on_query_diagnostics, mutation_without_readback_diagnostics,
    operational_env_names, policy_reachability_diagnostics, query_view_sql_file_diagnostics,
    rbac_catalog_diagnostics, rbac_catalog_missing_diagnostics, rbac_missing_policy_diagnostics,
    rbac_role_undeclared_diagnostics, report_diagnostics, returns_list_001, returns_list_002,
    route_guard, route_id_effect_consistency_diagnostics, updates_missing_updated_at_diagnostics,
    vocab_grammar_form_diagnostics,
};

impl DoctorPackage {
    /// Fan out every aggregator on the loaded package, fold in the
    /// caller-injected package-level findings, then sort + suppress —
    /// returning the merged `lazuli doctor` stream. Public so the CLI
    /// (formatting / exit-code / report layer) and the LSP (in-editor
    /// Layer-2 run) both consume it after `run_package`.
    pub fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
        let mut diagnostics = Vec::new();

        diagnostics.extend(manifest_required_diagnostics(
            &self.project_root,
            self.single_file_input,
        ));
        diagnostics.extend(lazurite_manifest_diagnostics(self));

        // Iron-hand context-vocabulary lints (VOCAB-CONTEXT-PURPOSE-001,
        // VOCAB-CONTEXT-NONGOALS-001, VOCAB-CONTEXT-CTXMD-001). Severity
        // resolves through: manifest override > preset escalation
        // (iron-hand promotes to error) > category default.
        diagnostics.extend(self.context_vocab_diagnostics());

        // PG.B — plan-and-gate cross-feature checks.
        if let Some(facts) = &self.plan_gate_facts {
            let eval_order_inputs = collect_callable_bodies_for_eval_order(&self.files);
            for diag in lazuli_analyzer::diagnose_plan_gate_facts(facts, &eval_order_inputs) {
                let path = self
                    .app
                    .as_ref()
                    .map(|a| a.path.clone())
                    .or_else(|| self.files.first().map(|f| f.path.clone()))
                    .unwrap_or_else(|| self.project_root.clone());
                diagnostics.push(DoctorDiagnostic {
                    path,
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: diag.code.as_str().to_owned(),
                    message: diag.message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        let declared_env_names: BTreeSet<&str> = self
            .app
            .as_ref()
            .map(|app| operational_env_names(&app.manifest, self.registry.as_ref()))
            .unwrap_or_default();
        for file in &self.files {
            let after_dedupe = dedupe_env_contract_diagnostics(&file.local_diagnostics);
            diagnostics.extend(suppress_env_schema_when_declared(
                &after_dedupe,
                &declared_env_names,
            ));
        }
        diagnostics.extend(vocab_grammar_form_diagnostics(
            &self.files,
            self.security_profile,
        ));

        // `VOCAB-*` rule catalog dispatch — closes the deferred wiring
        // cell documented in `docs/proposals/doctor-vocabulary-lints.md`
        // §"Implementation status (post-wave)". Fifteen rules surface
        // from one Tier 3 walk plus a sidecar text-walker for
        // `VOCAB-CAP-MISSING-001` (the IR drops `@pii.*` decorators).
        // `Prototype` profile suppresses the whole family (vocabulary
        // refactors are opt-in at prototype); `Strict` -> Warning;
        // `Production` -> Error.
        diagnostics.extend(aggregators::vocab::diagnostics(
            &self.tier3_facts,
            self.security_profile,
        ));
        diagnostics.extend(aggregators::vocab::cap_missing_diagnostics(
            &self.files,
            self.security_profile,
        ));

        // LIFECYCLE-* rule catalog dispatch — closes the wiring gap
        // flagged by
        // `docs/proposals/lifecycle-vocab-architect-audit-2026-05-27.md`
        // §"Cell A". Ten resource-lifecycle structural checks ship in
        // `lazuli_doctor::lifecycle::*` with passing unit tests but no
        // dispatcher entry. `Prototype` profile suppresses the whole
        // family (lifecycle is opt-in vocabulary); `Strict` -> Warning;
        // `Production` -> Error.
        diagnostics.extend(aggregators::lifecycle::diagnostics(
            &self.tier3_facts,
            self.security_profile,
        ));

        diagnostics.extend(lazuli_version_001_diagnostics(
            self.app.as_ref(),
            LZIR_SCHEMA,
        ));
        diagnostics.extend(lazuli_version_002_diagnostics(
            self.app.as_ref(),
            LZIR_SCHEMA,
            &self.project_root,
        ));

        diagnostics.extend(policy_reachability_diagnostics(
            &self.files,
            &self.experiences,
            &self.commands,
        ));
        diagnostics.extend(cap_file_policy_implicit_diagnostics(&self.tier3_facts));
        diagnostics.extend(schema_rich_gap_diagnostics(&self.tier3_facts));
        diagnostics.extend(manual_param_coercion_diagnostics(&self.project_root));
        diagnostics.extend(import_deprecated_alias_diagnostics(&self.project_root));
        diagnostics.extend(duplicate_query_name_diagnostics(&self.tier3_facts));
        diagnostics.extend(missing_policy_on_query_diagnostics(&self.tier3_facts));
        diagnostics.extend(mutation_without_readback_diagnostics(&self.tier3_facts));
        diagnostics.extend(updates_missing_updated_at_diagnostics(&self.tier3_facts));
        diagnostics.extend(route_id_effect_consistency_diagnostics(&self.tier3_facts));
        // Cycle-2 cell DC1 — sweep the rest of `lazuli_doctor::correctness`
        // into the doctor dispatch so `lazuli doctor` reaches every
        // diagnostic the crate carries (the 11 sibling rules that until
        // now only fired in their `#[cfg(test)] mod tests`).
        // Resolve handler `app_root` from the manifest the same way
        // `load_lzi.rs` does for the per-feature test-discipline loop.
        // `HANDLER-SIGNATURE-MISMATCH-001` reads from
        // `<app_root>/features/<f>/handlers/`; defaults to
        // `project_root` when manifest absent or `[lazurite].app_dir`
        // unset (handler-walking rules gracefully return empty when
        // the resolved path doesn't exist).
        let correctness_app_root = self
            .lazurite_manifest
            .as_ref()
            .map(|m| m.app_root(&self.project_root))
            .unwrap_or_else(|| self.project_root.clone());
        diagnostics.extend(aggregators::correctness::diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
            &self.project_root,
            &correctness_app_root,
            self.security_profile,
            self.single_file_input,
        ));
        // JOB-* runtime-gap rules over the lifted `Feature.jobs` —
        // today `JOB-DECLARATIVE-BODY-UNSUPPORTED-001`, which fires when
        // a declarative job body lowers to a no-op (the runtime has no
        // `jobs.JobContract` slot to execute it). Severity resolves
        // through `[doctor.error_handling].preset` (warn under strict,
        // error under iron-hand) like the `HANDLER-*` family.
        diagnostics.extend(aggregators::job_runtime_gap::diagnostics(
            &self.tier3_facts,
            self.lazurite_manifest.as_ref(),
        ));
        // `.lzi` hygiene — file size, file/feature name alignment,
        // and multi-feature cohesion. Reads
        // `[doctor.lzi_hygiene].preset` from the workspace manifest.
        {
            let manifest = lazuli_manifest::lazurite_manifest::load(&self.project_root)
                .ok()
                .flatten();
            diagnostics.extend(aggregators::lzi_hygiene::lzi_hygiene_diagnostics(
                &self.project_root,
                manifest.as_ref(),
            ));
        }
        diagnostics.extend(returns_list_001::diagnostics(
            &self.tier3_facts,
            &self.project_root,
        ));
        diagnostics.extend(returns_list_002::diagnostics(&self.tier3_facts));
        diagnostics.extend(aggregators::app_manifest::app_contract_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref(),
            &self.profiles,
            &self.operational,
        ));
        diagnostics.extend(aggregators::app_manifest::workspace_contract_diagnostics(
            self.workspace.as_ref(),
        ));
        diagnostics.extend(aggregators::external::external_contract_diagnostics(
            &self.contracts,
            self.workspace.as_ref(),
        ));

        // Cut A — agent + tool + eval + discriminator cross-feature checks.
        diagnostics.extend(aggregators::agent::registry_tool_effect_diagnostics(
            &self.registry_tool_defects,
        ));
        diagnostics.extend(aggregators::agent::agent_tool_diagnostics(
            &self.agents,
            &self.feature_symbols,
            self.registry.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(aggregators::agent::agent_discriminator_diagnostics(
            &self.agents,
            &self.tier3_facts,
        ));
        diagnostics.extend(aggregators::agent::agent_eval_diagnostics(&self.agents));
        diagnostics.extend(cross_feature_type_unresolved_diagnostics(
            &self.files,
            &self.tier3_facts,
            &self.feature_resources,
        ));
        diagnostics.extend(feature_uses_missing_diagnostics(
            &self.files,
            &self.tier3_facts,
            &self.feature_resources,
            &self.feature_uses,
        ));

        // Cut A.7 — `expose http` cross-feature checks.
        let known_audiences = collect_known_audiences(&self.files);
        diagnostics.extend(aggregators::agent::agent_expose_diagnostics(
            &self.agents,
            &self.tier3_facts,
            &known_audiences,
        ));

        // Cut A.8 — built-in trace event reservation + subscriber
        // payload drift checks.
        diagnostics.extend(aggregators::agent::agent_run_trace_diagnostics(&self.files));

        // Observability bucket cycle row 37 — `audit emit_to`
        // resolution, `event.trace level` closed catalog, and health
        // probe path shape. Phase L Tier 4b — `audit emit_to` for
        // commands is now IR-driven via `tier3_facts`; the text walker
        // is narrowed to skip command bodies.
        diagnostics.extend(aggregators::audit::diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(aggregators::audit::resource_policy_hints(
            &self.tier3_facts,
            &self.feature_resources,
        ));

        // RB.B — RBAC catalog diagnostics.
        // Run BEFORE legacy `collect_known_roles`/approval checks so
        // `@role.*` resolution uses the catalog when present and falls
        // back to text-walk only when no catalog is declared.
        let (rbac_diags, rbac_catalog) = rbac_catalog_diagnostics(&self.files);
        diagnostics.extend(rbac_diags);
        if let Some(catalog) = &rbac_catalog {
            diagnostics.extend(rbac_role_undeclared_diagnostics(&self.files, catalog));
        }
        diagnostics.extend(rbac_catalog_missing_diagnostics(
            &self.files,
            rbac_catalog.is_some(),
        ));
        diagnostics.extend(rbac_missing_policy_diagnostics(&self.files));

        // Cut A.9 — `approval` primitive contract + role resolution.
        // When the RBAC catalog is present, prefer its role set; fall
        // back to the legacy `collect_known_roles` text walk when no
        // catalog is declared (back-compat per
        // `docs/proposals/rbac-catalog-vocab.md` §Backwards compatibility).
        let known_roles = if let Some(catalog) = &rbac_catalog {
            catalog.roles.iter().map(|r| r.name.clone()).collect()
        } else {
            collect_known_roles(&self.files)
        };
        diagnostics.extend(approval_diagnostics(&self.tier3_facts, &known_roles));
        diagnostics.extend(scope_owner_column_diagnostics(&self.tier3_facts));
        diagnostics.extend(field_derived_from_unresolved_diagnostics(&self.tier3_facts));
        diagnostics.extend(resource_unique_qualifier_unknown_diagnostics(
            &self.tier3_facts,
        ));
        diagnostics.extend(resource_validates_path_unknown_diagnostics(
            &self.tier3_facts,
        ));
        diagnostics.extend(approval_missing_children_diagnostics(
            &self.approval_presences,
        ));

        diagnostics.extend(app_urls_missing_diagnostics(self.app.as_ref()));

        // Cut A.11 — `cors` block cross-checks against the app's
        // declared environments + urls.
        diagnostics.extend(aggregators::cors::diagnostics(self.app.as_ref()));

        // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy /
        // limits. Each block's typed lift is doctor-validated against
        // the closed catalog (same_site, parseable CIDR/size/duration).
        diagnostics.extend(aggregators::http_hygiene::cookie_diagnostics(
            self.app.as_ref(),
        ));
        diagnostics.extend(aggregators::http_hygiene::proxy_diagnostics(
            self.app.as_ref(),
        ));
        diagnostics.extend(aggregators::http_hygiene::limits_diagnostics(
            self.app.as_ref(),
        ));

        // Roadmap §1.10 — `app.headers` production-completeness +
        // closed-catalog gate.
        diagnostics.extend(aggregators::headers_secrets::headers_diagnostics(
            self.app.as_ref(),
            self.security_profile,
        ));
        // Roadmap §1.10 — `secret_rotation` overlap + binding
        // cross-check.
        diagnostics.extend(aggregators::headers_secrets::secret_rotation_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Observability bucket cycle row 36 — `app.logging` and
        // `app.tracing` closed-catalog + range + exporter binding
        // checks.
        diagnostics.extend(aggregators::observability::logging_tracing_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(aggregators::observability::app_diagnostics(
            self.app.as_ref(),
        ));

        // Phase L — auth block cross-feature diagnostics.
        diagnostics.extend(aggregators::auth::diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
        ));
        // AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 — warn when the app's
        // `actor_query` resolves the authenticated actor to a non-`User`
        // resource while an owner/scope check (ctx.user / @scope.owner /
        // @scope.same_org) gates a User-typed owner; both identities
        // collapse into the single ctx.User runtime slot.
        diagnostics.extend(aggregators::auth_actor_subject::diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
            &self.feature_uses,
        ));
        diagnostics.extend(auth_refresh::diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_uses,
            self.app.as_ref(),
            &self.files,
        ));
        diagnostics.extend(check_auth_session_callsite_001(
            &self.auth_facts,
            &self.project_root,
        ));
        // SESSION-QUERY-TEMPORAL-VALIDITY-001 — IR-driven session-query
        // security invariant. Promotes the warn-only LSP
        // `active-session-temporal-scope` text-scan to a blocking,
        // name-agnostic IR rule over the resource bound by `auth sessions
        // resource <X>`. Reads `Query.filters` from the re-parsed typed
        // `Feature` (the fact-only auth slices the aggregator above uses
        // do not carry queries).
        diagnostics.extend(self.session_query_temporal_validity_diagnostics());

        // Row 30 — Storage bucket cycle: 5 typed `@cap.File`
        // diagnostics. See `docs/proposals/bucket-storage-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(cap_file_storage_diagnostics(&self.operational));

        // Row 33 — Jobs bucket cycle: six IR-driven diagnostics on the
        // Tier 3 lift (`JOB-TIMEOUT-001`, `JOB-FANOUT-001`,
        // `JOB-FANOUT-002`, `WEBHOOK-SCOPE-001`, `NOTIF-CHANNEL-001`,
        // `EVENTGROUP-NESTING-001`). See
        // `docs/proposals/bucket-jobs-cycle.md` §Doctor/LSP.
        //
        // Rows 38–39 — Webhooks expanded cycle: eight additional
        // IR-driven diagnostics (`WEBHOOK-PAYLOAD-001/002`,
        // `WEBHOOK-REPLAY-001/002`, `WEBHOOK-DLQ-001/002/003`,
        // `WEBHOOK-EVENT-001`). Threaded through the same
        // `tier3_diagnostics` entry-point so the iteration over
        // feature webhooks stays single-pathed.
        diagnostics.extend(aggregators::tier3::tier3_diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(aggregators::webhook_event_registry::diagnostics(
            self.registry.as_ref(),
        ));

        // Row 34 — `event_group` pattern-prefix rule promoted from LSP
        // to doctor now that `EventGroup` IR exists.
        diagnostics.extend(aggregators::event_group::diagnostics(&self.tier3_facts));

        // Rows 41-43 — Migrations bucket cycle Route C: eight new
        // IR-driven diagnostics covering rename hints, the
        // `tenant_migration` kind, and the deploy block expansion. See
        // `docs/proposals/bucket-migrations-cycle.md` §Doctor.
        diagnostics.extend(aggregators::migrations::diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
        ));

        // Row 48 — OpenAPI bucket cycle: five `deprecated_*` + text-pattern
        // api detection. See `docs/proposals/bucket-openapi-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(aggregators::deprecated::diagnostics(&self.tier3_facts));

        // Row 51 — Cache bucket cycle: five `cache_*` diagnostics. See
        // `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP.
        diagnostics.extend(aggregators::cache::diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(query_view_sql_file_diagnostics(
            &self.tier3_facts,
            &self.project_root,
        ));

        // Row 54 — i18n bucket cycle: 15 locale/translation diagnostics.
        // See `docs/proposals/bucket-i18n-cycle.md` §Doctor/LSP.
        diagnostics.extend(aggregators::i18n::diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
            &self.files,
        ));
        diagnostics.extend(check_codegen_wrap_001(&self.project_root));
        // D3 — `SCHEMA-RICH-001` is computed by the caller (the CLI,
        // which owns the TS-codegen module loader this crate deliberately
        // does not depend on) and handed in via `run_package`'s `injected`
        // argument. Folded in here, BEFORE the final sort, so it lands in
        // exactly the same byte-identical position as the inline call did.
        // The LSP injects nothing (no `dist/` to walk), so this is empty
        // in-editor.
        diagnostics.extend(self.injected.iter().cloned());
        diagnostics.extend(check_pattern_draft_stale_001(&self.project_root));

        // Report vocab — 10 doctor codes per
        // `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP. The
        // capability-aware rules (`REPORT-SIGNED-NO-STORAGE-001`,
        // `REPORT-STORAGE-AMBIGUOUS-001`) read object_storage caps from
        // the package's app manifest + registry.
        diagnostics.extend(report_diagnostics(
            &self.tier3_facts,
            self.app.as_ref().map(|a| &a.manifest),
            self.registry.as_ref(),
        ));

        // CL.C.4 — domain-model diagnostics (roadmap §1.7). Four codes:
        // `AGGREGATE-ROOT-UNKNOWN`, `AGGREGATE-CONTAINS-UNKNOWN`,
        // `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`.
        diagnostics.extend(aggregators::domain::diagnostics(&self.tier3_facts));

        // GAP-12 — REF-CROSS-FEATURE-UNKNOWN-001. Field-level `target
        // @feature.<feature>.<Resource>` FK references must name a feature
        // in the declaring feature's `uses` (Dependencies) and a resource
        // that exists in it. Module-level: build the per-feature views from
        // the Tier-3 fact bundle (which carries `uses` + full resources).
        {
            use lazuli_doctor::cross_feature::ref_unknown_001;
            let views: Vec<ref_unknown_001::FeatureCrossRefView> = self
                .tier3_facts
                .iter()
                .map(|fact| {
                    let mut targets = Vec::new();
                    for resource in &fact.resources {
                        for field in &resource.fields {
                            if let Some(t) = &field.cross_feature_target {
                                targets.push(ref_unknown_001::CrossFeatureTargetRef {
                                    resource: resource.name.clone(),
                                    field: field.name.clone(),
                                    target_feature: t.feature.clone(),
                                    target_resource: t.resource.clone(),
                                    origin: ref_unknown_001::Origin::Resource,
                                });
                            }
                        }
                    }
                    // GAP-R5 — `target @feature.<f>.<R>` may also sit on a
                    // `record <Name>` field (logical-only ID nested in JSONB,
                    // no migration index). Same resolution as resources.
                    for record in &fact.records {
                        for field in &record.fields {
                            if let Some(t) = &field.cross_feature_target {
                                targets.push(ref_unknown_001::CrossFeatureTargetRef {
                                    resource: record.name.clone(),
                                    field: field.name.clone(),
                                    target_feature: t.feature.clone(),
                                    target_resource: t.resource.clone(),
                                    origin: ref_unknown_001::Origin::Record,
                                });
                            }
                        }
                    }
                    ref_unknown_001::FeatureCrossRefView {
                        feature: fact.feature.clone(),
                        path: fact.path.clone(),
                        uses: fact.uses.clone(),
                        resources: fact.resources.iter().map(|r| r.name.clone()).collect(),
                        cross_feature_targets: targets,
                    }
                })
                .collect();
            for finding in ref_unknown_001::check(&views) {
                let line = self
                    .tier3_facts
                    .iter()
                    .find(|f| f.feature == finding.feature)
                    .map(|f| f.feature_line)
                    .unwrap_or(1);
                diagnostics.push(DoctorDiagnostic {
                    path: doctor_rule_path(&self.project_root, finding.path.clone()),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: ref_unknown_001::Finding::CODE.to_owned(),
                    message: finding.message(),
                    category: None,
                    feature_name: Some(finding.feature.clone()),
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // GAP-13 — REF-POLYMORPHIC-TARGET-001. Every `polymorphic_ref`
        // target must resolve to a resource in the declaring feature or a
        // feature it `uses` (reuses the GAP-12 resolution model).
        {
            use lazuli_doctor::cross_feature::polymorphic_target_001;
            let views: Vec<polymorphic_target_001::FeaturePolymorphicView> = self
                .tier3_facts
                .iter()
                .map(|fact| {
                    let mut sites = Vec::new();
                    for resource in &fact.resources {
                        for pref in &resource.polymorphic_refs {
                            sites.push(polymorphic_target_001::PolymorphicRefSite {
                                resource: resource.name.clone(),
                                type_field: pref.type_field.clone(),
                                targets: pref.targets.clone(),
                            });
                        }
                    }
                    polymorphic_target_001::FeaturePolymorphicView {
                        feature: fact.feature.clone(),
                        path: fact.path.clone(),
                        uses: fact.uses.clone(),
                        resources: fact.resources.iter().map(|r| r.name.clone()).collect(),
                        polymorphic_refs: sites,
                    }
                })
                .collect();
            for finding in polymorphic_target_001::check(&views) {
                let line = self
                    .tier3_facts
                    .iter()
                    .find(|f| f.feature == finding.feature)
                    .map(|f| f.feature_line)
                    .unwrap_or(1);
                diagnostics.push(DoctorDiagnostic {
                    path: doctor_rule_path(&self.project_root, finding.path.clone()),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: polymorphic_target_001::Finding::CODE.to_owned(),
                    message: finding.message(),
                    category: None,
                    feature_name: Some(finding.feature.clone()),
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // GAP-AUDIT-01 — AUDIT-MATERIALIZE-TARGET-001. Every command
        // `audit materialize @feature.<f>.<R>` target must resolve to a
        // reachable resource (same feature or a `uses` dependency) AND
        // that resource must be `append_only` (W4 modifier). Reuses the
        // GAP-12 `uses`-as-Dependencies resolution model; anchors at the
        // command header.
        {
            use lazuli_doctor::cross_feature::audit_materialize_target_001 as amt;
            let views: Vec<amt::FeatureAuditMaterializeView> = self
                .tier3_facts
                .iter()
                .map(|fact| {
                    let mut sites = Vec::new();
                    for command in &fact.commands {
                        if let Some(audit) = &command.audit {
                            if let Some(m) = &audit.materialize {
                                sites.push(amt::AuditMaterializeSite {
                                    command: command.name.clone(),
                                    target_feature: m.feature.clone(),
                                    target_resource: m.resource.clone(),
                                });
                            }
                        }
                    }
                    amt::FeatureAuditMaterializeView {
                        feature: fact.feature.clone(),
                        path: fact.path.clone(),
                        uses: fact.uses.clone(),
                        resources: fact
                            .resources
                            .iter()
                            .map(|r| amt::ResourceAppendOnly {
                                name: r.name.clone(),
                                append_only: r.append_only,
                            })
                            .collect(),
                        materialize_sites: sites,
                    }
                })
                .collect();
            for finding in amt::check(&views) {
                let fact = self
                    .tier3_facts
                    .iter()
                    .find(|f| f.feature == finding.feature);
                let line = fact
                    .and_then(|f| f.command_lines.get(&finding.command).copied())
                    .or_else(|| fact.map(|f| f.feature_line))
                    .unwrap_or(1);
                diagnostics.push(DoctorDiagnostic {
                    path: doctor_rule_path(&self.project_root, finding.path.clone()),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: amt::Finding::CODE.to_owned(),
                    message: finding.message(),
                    category: None,
                    feature_name: Some(finding.feature.clone()),
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // IR Error-Vocab (Cell ANALYZE-1) — 7 typed `ERR-VOCAB-*` codes
        // per `docs/proposals/ir-error-messages-vocab.md` §6. Operates
        // on the lowered IR carried in `tier3_facts`; `files` is passed
        // for `SpanRef -> line` resolution so each diagnostic anchors at
        // the offending construct.
        diagnostics.extend(aggregators::error_vocab::diagnostics(
            &self.tier3_facts,
            &self.files,
        ));
        diagnostics.extend(route_guard::diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(lifecycle_gate::diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));

        // Workspace-wide Go-handler rules: HANDLER-* error-handling
        // family + HANDLER-SQL-COLUMN-DRIFT-001 +
        // TEST-FAILURE-ONLY-COVERAGE-001. All three share the same
        // `walk_workspace_go_handlers` walker so they live in one
        // aggregator.
        diagnostics.extend(aggregators::error_handling_handlers::diagnostics(
            &correctness_app_root,
            self.lazurite_manifest.as_ref(),
            self.security_profile,
        ));

        diagnostics.extend(aggregators::folder::diagnostics(
            &self.project_root,
            self.security_profile,
        ));
        diagnostics.extend(aggregators::design::diagnostics(
            &self.project_root,
            self.security_profile,
        ));

        diagnostics.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then(left.path.cmp(&right.path))
                .then(left.line.cmp(&right.line))
                .then(left.column.cmp(&right.column))
        });

        // B3 — suppress legacy `semantic_type_unknown` errors for any
        // `@semantic.<Name>` that resolves through a plugin manifest
        // alias map. The legacy diagnostic was authored against the
        // closed catalog; the plugin-locales proposal augments the
        // catalog without touching the diagnostic site. Filter here
        // rather than threading the alias set down to every emission
        // call site so the existing walker code stays untouched.
        // See `docs/proposals/semantic-types-plugin-locales.md`
        // §New diagnostics.
        if let Some(manifest) = self.lazurite_manifest.as_ref() {
            if let Ok(alias_map) = lazuli_manifest::plugin_manifest::build_alias_map(
                Some(manifest),
                &self.project_root,
            ) {
                if !alias_map.is_empty() {
                    diagnostics.retain(|d| {
                        if d.code != "semantic_type_unknown" {
                            return true;
                        }
                        // Match the alias name out of the diagnostic
                        // message (`unknown @semantic type "@semantic.X"; ...`).
                        // The message format is fixed; if it changes,
                        // this filter has a single update site.
                        let alias = d
                            .message
                            .split_once('"')
                            .and_then(|(_, rest)| rest.split_once('"').map(|(a, _)| a))
                            .unwrap_or("");
                        !alias_map.contains_key(alias)
                    });
                }
            }
        }

        diagnostics
    }
}
