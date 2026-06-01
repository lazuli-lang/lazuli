impl DoctorPackage {
    fn diag_c(&self, diagnostics: &mut Vec<DoctorDiagnostic>) {
        let correctness_app_root = self
            .lazurite_manifest
            .as_ref()
            .map(|m| m.app_root(&self.project_root))
            .unwrap_or_else(|| self.project_root.clone());

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
                        if let Some(audit) = &command.audit
                            && let Some(m) = &audit.materialize {
                                sites.push(amt::AuditMaterializeSite {
                                    command: command.name.clone(),
                                    target_feature: m.feature.clone(),
                                    target_resource: m.resource.clone(),
                                });
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
            // v2 — `[doctor.error_handling] preset` + `[doctor.test_discipline]
            // preset` off the caller's severity config (CLI: disk; LSP:
            // unsaved buffer) instead of an on-disk manifest read.
            self.config.error_handling_preset,
            self.config.test_discipline_preset,
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
        //
        // 0020 — the alias map is now resolved through the AUTHORITATIVE
        // upward-walked root (`authoritative_alias_map`), the SAME inputs
        // codegen + the SEMANTIC-PLUGIN-001 check use. Running `lazuli
        // doctor app` from a features subdir, the plugin-provided BR
        // scalars (`@semantic.Brazilian*`, declared in the repo-root
        // manifest) now resolve and are correctly suppressed — instead of
        // false-flagging `semantic_type_unknown` because the pass-through
        // `self.project_root` (= `app/`) carried no manifest. This keeps
        // doctor in agreement with generate on EVERY surface, not just
        // SEMANTIC-PLUGIN-001.
        if let Ok(alias_map) = aggregators::lazurite_manifest::authoritative_alias_map(self)
            && !alias_map.is_empty()
        {
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
