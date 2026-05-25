//! `DoctorPackage::diagnostics` — the doctor dispatcher.
//!
//! Fans out every `*_diagnostics` aggregator on the loaded
//! `DoctorPackage`, then sorts + suppresses the result.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 3.
//! The aggregators themselves live in `doctor/aggregators/*` or as
//! `pub(super) fn *_diagnostics` items in `doctor/mod.rs`.

use lazuli_lsp::SecurityProfile;

use super::aggregators;
use super::auth_refresh;
use super::lifecycle_gate;
use super::package::DoctorPackage;
use super::returns_list_001;
use super::returns_list_002;
use super::route_guard;
use super::schema_rich_001;
use super::{
    DoctorDiagnostic, DoctorSeverity, LZIR_SCHEMA, adapter_provenance_diagnostics,
    agent_discriminator_diagnostics, agent_eval_diagnostics, agent_expose_diagnostics,
    agent_run_trace_diagnostics, agent_tool_diagnostics, app_binding_contract_diagnostics,
    app_contract_diagnostics, app_pack_contract_diagnostics, app_route_redirect_diagnostics,
    app_service_contract_diagnostics, app_urls_missing_diagnostics, approval_diagnostics,
    approval_missing_children_diagnostics, audit_event_health_diagnostics, auth_diagnostics,
    cap_file_policy_implicit_diagnostics, cap_file_storage_diagnostics,
    check_auth_session_callsite_001, check_codegen_wrap_001, check_pattern_draft_stale_001,
    collect_callable_bodies_for_eval_order, collect_known_audiences, collect_known_roles,
    command_route_binding_diagnostics, cross_feature_type_unresolved_diagnostics,
    dedupe_env_contract_diagnostics, doctor_rule_path, duplicate_query_name_diagnostics,
    error_page_contract_diagnostics, external_call_contract_diagnostics,
    external_contract_diagnostics, feature_uses_missing_diagnostics,
    field_derived_from_unresolved_diagnostics, import_deprecated_alias_diagnostics,
    lazuli_version_001_diagnostics, lazuli_version_002_diagnostics,
    lazurite_manifest_diagnostics, manifest_required_diagnostics,
    manual_param_coercion_diagnostics, missing_policy_on_query_diagnostics,
    mutation_without_readback_diagnostics, operational_env_names, policy_reachability_diagnostics,
    profile_contract_diagnostics, query_view_sql_file_diagnostics, rbac_catalog_diagnostics,
    rbac_catalog_missing_diagnostics, rbac_missing_policy_diagnostics,
    rbac_role_undeclared_diagnostics, registry_tool_effect_diagnostics, report_diagnostics,
    resource_policy_and_command_audit_hints, resource_unique_qualifier_unknown_diagnostics,
    resource_validates_path_unknown_diagnostics, route_id_effect_consistency_diagnostics,
    scope_owner_column_diagnostics, schema_rich_gap_diagnostics, suppress_env_schema_when_declared,
    tier3_diagnostics, updates_missing_updated_at_diagnostics, vocab_grammar_form_diagnostics,
    workspace_contract_diagnostics,
};

impl DoctorPackage {
    pub(super) fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
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
        diagnostics.extend(aggregators::correctness::diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
            &self.project_root,
            self.security_profile,
            self.single_file_input,
        ));
        diagnostics.extend(returns_list_001::diagnostics(
            &self.tier3_facts,
            &self.project_root,
        ));
        diagnostics.extend(returns_list_002::diagnostics(&self.tier3_facts));
        diagnostics.extend(app_contract_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref(),
            &self.profiles,
            &self.operational,
        ));
        diagnostics.extend(workspace_contract_diagnostics(self.workspace.as_ref()));
        diagnostics.extend(external_contract_diagnostics(
            &self.contracts,
            self.workspace.as_ref(),
        ));

        // Cut A — agent + tool + eval + discriminator cross-feature checks.
        diagnostics.extend(registry_tool_effect_diagnostics(
            &self.registry_tool_defects,
        ));
        diagnostics.extend(agent_tool_diagnostics(
            &self.agents,
            &self.feature_symbols,
            self.registry.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(agent_discriminator_diagnostics(
            &self.agents,
            &self.tier3_facts,
        ));
        diagnostics.extend(agent_eval_diagnostics(&self.agents));
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
        diagnostics.extend(agent_expose_diagnostics(
            &self.agents,
            &self.tier3_facts,
            &known_audiences,
        ));

        // Cut A.8 — built-in trace event reservation + subscriber
        // payload drift checks.
        diagnostics.extend(agent_run_trace_diagnostics(&self.files));

        // Observability bucket cycle row 37 — `audit emit_to`
        // resolution, `event.trace level` closed catalog, and health
        // probe path shape. Phase L Tier 4b — `audit emit_to` for
        // commands is now IR-driven via `tier3_facts`; the text walker
        // is narrowed to skip command bodies.
        diagnostics.extend(audit_event_health_diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(resource_policy_and_command_audit_hints(
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
        diagnostics.extend(auth_diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
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
        diagnostics.extend(tier3_diagnostics(
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
        diagnostics.extend(
            schema_rich_001::check(&self.project_root)
                .into_iter()
                .map(|finding| DoctorDiagnostic {
                    path: doctor_rule_path(&self.project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: schema_rich_001::Finding::CODE.to_owned(),
                    message: finding.message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }),
        );
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
            if let Ok(alias_map) =
                crate::plugin_manifest::build_alias_map(Some(manifest), &self.project_root)
            {
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
