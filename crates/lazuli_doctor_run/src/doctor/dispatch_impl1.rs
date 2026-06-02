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
        // VOCAB-CONTEXT-NONGOALS-001, VOCAB-CONTEXT-CTXMD-001, and CUT 1b's
        // VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 — the drift-killer that fires
        // when a `<feature>.ctx.md` markdown table shadows a resource's
        // fields). Severity resolves through: manifest override > preset
        // escalation (iron-hand promotes to error) > category default.
        diagnostics.extend(self.context_vocab_diagnostics());

        // Knowledge-sector vocabulary — the five `VOCAB-KNOWLEDGE-*` rules
        // (SECTOR-UNKNOWN, DANGLING-CITE, UNGATED-WRITE, STALE, DUP-TOPIC).
        // Sibling of the VOCAB-CONTEXT family above: same Vocabulary
        // category + severity precedence, but it additionally iterates
        // features carrying a `knowledge <sector>` field (SECTOR-UNKNOWN /
        // DANGLING-CITE) and scans the on-disk `knowledge/<sector>/` gold-doc
        // vault for the write-gate + decay + dedup rules. Closes the dormant
        // wiring gap: the rules compiled + were registry-claimed but no
        // dispatcher invoked them. See
        // `docs/proposals/knowledge-sector-field.md` §Doctor.
        diagnostics.extend(self.knowledge_vocab_diagnostics());

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

        self.diag_env(&mut diagnostics);
        self.diag_b(&mut diagnostics);
        self.diag_c(&mut diagnostics);
        diagnostics
    }

    fn diag_env(&self, diagnostics: &mut Vec<DoctorDiagnostic>) {
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

        // G-A2 — command-trigger lifecycle-transition pass
        // (`LIFECYCLE-TRANSITION-001..006`). The analyzer
        // `run_lifecycle_transition_checks` pass shipped with passing unit
        // tests but was never invoked from the CLI / doctor / LSP — the six
        // codes were dormant (declared-but-inert, same class as F4). This
        // dispatches it over the synthesized feature set so it fires on real
        // `command … triggers transition <x>` usage. Severity defers to
        // `RuleCategory::Lifecycle` (Strict → Warning, Production → Error;
        // Prototype suppresses).
        diagnostics.extend(aggregators::lifecycle::transition_command_diagnostics(
            &self.tier3_facts,
            self.security_profile,
        ));

        // G-A2 — §7a surface-UX rules (`LZX-WIZARD-STEPS-EXPR-001`,
        // `LZX-TAB-GROUP-CASE-001`, `LZX-TAB-VIEW-REF-001`,
        // `LZX-VIEW-MODE-001`, `LZX-BOARD-LANES-001`,
        // `LZX-REPEATABLE-SUM-001`, `LZX-DATE-RANGE-001`). The
        // `doctor::lzx::ux_rules` + `date_range_filter` checks shipped with
        // passing unit tests but were reached only from those tests — never
        // from the package-doctor `.lzx` pass, so they never fired on real
        // surface usage. This synthesizes the rich `ir::Surface` shape from the
        // parsed `.lzx` documents + Tier-3 facts and runs them. Severity:
        // `RuleCategory::Correctness` (Strict → Warning, Production → Error;
        // Prototype suppresses).
        diagnostics.extend(aggregators::lzx_ux::diagnostics(
            &self.files,
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
        diagnostics.extend(policy_ref_unresolved_diagnostics(&self.tier3_facts));
        diagnostics.extend(mutation_without_readback_diagnostics(&self.tier3_facts));
        diagnostics.extend(updates_missing_updated_at_diagnostics(&self.tier3_facts));
        diagnostics.extend(creates_empty_bindings_diagnostics(&self.tier3_facts));
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
    }

    fn diag_b(&self, diagnostics: &mut Vec<DoctorDiagnostic>) {
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
            // v2 — `[doctor.error_handling] preset` off the caller's
            // severity config (CLI: disk; LSP: unsaved buffer).
            self.config.error_handling_preset,
        ));
        // `.lzi` hygiene — file size, file/feature name alignment,
        // and multi-feature cohesion. v2 — `[doctor.lzi_hygiene] preset`
        // rides the caller's severity config instead of an independent
        // on-disk manifest reload here.
        diagnostics.extend(aggregators::lzi_hygiene::lzi_hygiene_diagnostics(
            &self.project_root,
            self.config.lzi_hygiene_preset,
        ));
        // Escape-hatch visibility (spec 0010) — the three `ESC-*` rules
        // (`ESC-RAWSQL-IN-HANDLER-001`, `ESC-SQL-TENANCY-CONTRACT-001`,
        // `ESC-SCOPE-OVERRIDE-UNGUARDED-001`) close the holes where a read's
        // effect/tenancy is invisible to a cold `.lzi` audit. Built + unit
        // tested in `lazuli_doctor::escape_hatch` but never dispatched until
        // now; this is the wiring that makes them fire on real apps. The
        // `[doctor.escape_hatch]` preset rides its own standalone enum (not
        // the shared `ResolvedDoctorConfig`), so `None` here keeps the
        // per-rule defaults (all Warning) until that config surface lands.
        diagnostics.extend(aggregators::escape_hatch::escape_hatch_diagnostics(
            &self.project_root,
            None,
        ));
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
        // spec 0004 — `defaults rate_limit` / `defaults audit` hoist hints.
        diagnostics.extend(aggregators::audit::defaults_hoist_hints(&self.tier3_facts));

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

        // AUTH-SESSION-CANONICAL-COLUMNS-001 — IR-driven preventability
        // guard: the resource bound by `auth sessions resource <X>` must
        // declare every column the runtime session resolver reads
        // (`expires_at` + a FK to the auth identity resource), else auth
        // silently 403s every authenticated request. Re-parses the typed
        // `Feature` IR for the session resource's fields + the identity
        // binding the fact-only auth slices above do not carry.
        diagnostics.extend(self.auth_session_canonical_columns_diagnostics());

        // SESSION-COOKIE-* — the five IR-driven session-cookie transport
        // diagnostics over `auth.sessions.cookie` (insecure-in-prod,
        // samesite-none-insecure, missing, profile-conflict,
        // host-prefix-violation). Re-parses the typed `Feature` IR for the
        // cookie sub-block the fact-only auth slices above do not carry.
        // See `docs/proposals/cookie-sessions-child.md` §Doctor.
        diagnostics.extend(self.session_cookie_diagnostics());

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
    }

}
