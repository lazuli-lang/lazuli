impl DoctorPackage {
    /// Iron-hand meta-bundle — dispatch the three `VOCAB-CONTEXT-*`
    /// rules across every `.lzi` feature in the package and resolve
    /// each finding's severity through the layered precedence:
    ///
    ///   1. Manifest user override
    ///      (`[doctor.test_discipline.severity_override."<CODE>"]`)
    ///      wins absolutely. Authors can downgrade an iron-hand error
    ///      back to a warning with a documented `reason`.
    ///   2. Active coverage preset escalation
    ///      (`preset_severity_overrides`): under `tdd-iron-hand` the
    ///      three rules become `error`.
    ///   3. Category default (`doctor_severity_for` →
    ///      `RuleCategory::Vocabulary` → warning at strict, error at
    ///      production).
    ///
    /// The `off` preset suppresses the rules entirely (consistent with
    /// the coverage layers it zeroes out).
    pub(super) fn context_vocab_diagnostics(&self) -> Vec<DoctorDiagnostic> {
        use lazuli_doctor::coverage::CoveragePreset;
        use lazuli_doctor::vocab::{
            vocab_context_ctxmd_001, vocab_context_nongoals_001,
            vocab_context_prose_shadows_ir_001, vocab_context_purpose_001,
        };

        let preset = self.coverage_preset();
        // `off` preset opts out entirely — mirrors how the coverage
        // layers all zero out under `off`. (The shared resolver also
        // returns `None` for these codes under `Off`; this early return
        // is the equivalent loop-level short-circuit.)
        if matches!(preset, Some(CoveragePreset::Off)) {
            return Vec::new();
        }

        // W1 — route every severity decision through
        // `lazuli_doctor_config::effective_severity`. For the VOCAB-CONTEXT
        // family (category Vocabulary, no category preset) this exercises
        // precedence levels 1 (manifest override), 2 (coverage-preset
        // escalation), and 4 (profile default).
        //
        // v2 — the severity config (profile + coverage preset + per-rule
        // overrides) is the caller-supplied `self.config`, NOT a fresh one
        // built from the on-disk manifest. `coverage_preset()` already
        // reads `self.config`, so the local `preset` matches; the overrides
        // ride the same config. Byte-identical for the CLI (its config is
        // built from the same on-disk `[doctor]`).
        let config = &self.config;

        // The VOCAB-CONTEXT codes always resolve to a concrete severity
        // here (the `Off` preset is already short-circuited above and the
        // category default always has an opinion), so the resolver never
        // returns `None`; `Warning` is the unreachable fallback.
        let resolve = |code: &str| -> DoctorSeverity {
            effective_severity(
                code,
                lazuli_doctor::DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                config,
            )
            .map(DoctorSeverity::from)
            .unwrap_or(DoctorSeverity::Warning)
        };

        let mut out: Vec<DoctorDiagnostic> = Vec::new();
        for file in &self.files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            let Ok(skeletons) = parse_feature_skeletons(&file.source) else {
                continue;
            };
            for skeleton in &skeletons {
                let Ok(feature) = lower_feature_skeleton(skeleton) else {
                    continue;
                };

                // VOCAB-CONTEXT-PURPOSE-001
                let sev = resolve(vocab_context_purpose_001::Finding::CODE);
                for finding in vocab_context_purpose_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_purpose_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                // VOCAB-CONTEXT-NONGOALS-001
                let sev = resolve(vocab_context_nongoals_001::Finding::CODE);
                for finding in vocab_context_nongoals_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_nongoals_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                // VOCAB-CONTEXT-CTXMD-001 — resolves the `<feature>.ctx.md`
                // convention sidecar at the SINGLE base of the feature
                // `.lzi` directory (no project-root fallback).
                let sev = resolve(vocab_context_ctxmd_001::Finding::CODE);
                for finding in vocab_context_ctxmd_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_ctxmd_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                // VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 (CUT 1b) — the
                // drift-killer. Reads the same `<feature>.ctx.md` convention
                // sidecar and fires when a markdown table's header columns
                // duplicate >=3 of a resource's field names (the prose
                // shadows the IR). The finding's `line` anchors at the
                // offending table row inside the sidecar.
                let sev = resolve(vocab_context_prose_shadows_ir_001::Finding::CODE);
                for finding in vocab_context_prose_shadows_ir_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: finding.table_line,
                        column: 1,
                        severity: sev,
                        code: vocab_context_prose_shadows_ir_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
        out
    }

    /// Knowledge-sector vocabulary — dispatch the five `VOCAB-KNOWLEDGE-*`
    /// rules across the package. Parallel to [`Self::context_vocab_diagnostics`]
    /// (same `Vocabulary` category, same `effective_severity` precedence,
    /// same `Off`-preset short-circuit) but with a richer input contract,
    /// because the five rules split across two data sources:
    ///
    ///   * **IR-driven** (`SECTOR-UNKNOWN`, `DANGLING-CITE`) — read the
    ///     feature's `knowledge <sector>` field and resolve `cites:` against
    ///     the lifted IR symbol table.
    ///   * **Vault-scanning** (`UNGATED-WRITE`, `STALE`, `DUP-TOPIC`) — walk
    ///     the on-disk `knowledge/<sector>/` gold docs via the shared
    ///     [`knowledge_vault`] scanner, anchored at the project root (the same
    ///     `self.project_root` the vault-scanner + git probe resolve against).
    ///
    /// Wiring contract (mirrors the rules' own input split):
    ///   (a) iterate every feature carrying a non-empty `knowledge` field —
    ///       `SECTOR-UNKNOWN` is per-feature (it names the declaring feature);
    ///   (b) collect the *distinct* sectors and scan each one once for the
    ///       three doc-level rules + `DANGLING-CITE`, so a sector referenced by
    ///       two features is not double-reported.
    ///
    /// The `DANGLING-CITE` symbol index is built once from **every** feature
    /// in the package (a doc may legitimately cite a sibling feature's
    /// symbol), and the rule self-skips when that index is empty — it never
    /// false-fires when no IR is loaded.
    ///
    /// [`knowledge_vault`]: lazuli_doctor::vocab::knowledge_vault
    pub(super) fn knowledge_vocab_diagnostics(&self) -> Vec<DoctorDiagnostic> {
        use lazuli_doctor::coverage::CoveragePreset;
        use lazuli_doctor::vocab::{
            vocab_knowledge_dangling_cite_001 as dangling,
            vocab_knowledge_dup_topic_001 as dup_topic,
            vocab_knowledge_sector_unknown_001 as sector_unknown,
            vocab_knowledge_single_feature_001 as single_feature,
            vocab_knowledge_stale_001 as stale, vocab_knowledge_ungated_write_001 as ungated,
        };

        let preset = self.coverage_preset();
        // `off` preset opts out of the whole vocabulary family (parity with
        // `context_vocab_diagnostics`).
        if matches!(preset, Some(CoveragePreset::Off)) {
            return Vec::new();
        }

        // Severity resolver — identical precedence to the VOCAB-CONTEXT
        // family: manifest override > coverage-preset escalation > category
        // default (Vocabulary: warning at strict, error at production).
        //
        // v2 — the override / preset / profile inputs ride the
        // caller-supplied `self.config` (CLI: disk; LSP: unsaved buffer),
        // not a fresh config built from the on-disk manifest.
        let config = &self.config;
        let resolve = |code: &str| -> DoctorSeverity {
            effective_severity(
                code,
                lazuli_doctor::DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                config,
            )
            .map(DoctorSeverity::from)
            .unwrap_or(DoctorSeverity::Warning)
        };

        // Lift every feature in the package once — reused both for the
        // per-feature `SECTOR-UNKNOWN` walk and the package-wide
        // `DANGLING-CITE` symbol index. Each entry pairs the source `.lzi`
        // path (for the SECTOR-UNKNOWN anchor) with the lowered `Feature`.
        let mut features: Vec<(std::path::PathBuf, lazuli_ir::Feature)> = Vec::new();
        for file in &self.files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            let Ok(skeletons) = parse_feature_skeletons(&file.source) else {
                continue;
            };
            for skeleton in &skeletons {
                if let Ok(feature) = lower_feature_skeleton(skeleton) {
                    features.push((file.path.clone(), feature));
                }
            }
        }

        // Build the DANGLING-CITE known-symbol index from EVERY feature
        // (cites may cross feature boundaries); the rule itself skips on an
        // empty index, so this is the only place we must guarantee the index
        // sees the whole package.
        let symbol_index = dangling::SymbolIndex::from_features(
            &features.iter().map(|(_, f)| f.clone()).collect::<Vec<_>>(),
        );

        let today = current_iso_date();
        let root = &self.project_root;

        // Custom sectors declared in `Lazurite.toml [knowledge.sectors]`.
        // Combined with the closed core catalog + the on-disk folder leg,
        // these form the three ways a `knowledge <sector>` is KNOWN to
        // SECTOR-UNKNOWN. Empty when no `[knowledge]` block is authored.
        let declared_sectors: Vec<String> = self
            .lazurite_manifest
            .as_ref()
            .map(|m| m.declared_knowledge_sectors())
            .unwrap_or_default();

        let mut out: Vec<DoctorDiagnostic> = Vec::new();

        // (a) Per-feature IR rule: SECTOR-UNKNOWN. Distinct sectors are
        //     collected here for the (b) vault scan below, in declaration
        //     order, de-duplicated.
        let mut sectors_seen: BTreeSet<String> = BTreeSet::new();
        let mut sectors_to_scan: Vec<String> = Vec::new();
        for (path, feature) in &features {
            // SECTOR-UNKNOWN-001 — names the declaring feature, so it runs
            // once per feature even if two features share a sector.
            let sev = resolve(sector_unknown::Finding::CODE);
            for finding in sector_unknown::check(feature, path, Some(root), &declared_sectors) {
                let message = finding.message();
                out.push(DoctorDiagnostic {
                    path: finding.path,
                    line: 1,
                    column: 1,
                    severity: sev,
                    code: sector_unknown::Finding::CODE.to_owned(),
                    message,
                    category: Some(RuleCategory::Vocabulary),
                    feature_name: Some(finding.feature),
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            if let Some(sector) = feature.knowledge.as_deref() {
                let sector = sector.trim();
                if !sector.is_empty() && sectors_seen.insert(sector.to_string()) {
                    sectors_to_scan.push(sector.to_string());
                }
            }
        }

        // (a2) Package-level cross-feature rule: SINGLE-FEATURE. Counts how
        //      many distinct features declare each `knowledge <sector>` slug
        //      across the WHOLE package and fires once per sector declared by
        //      exactly one feature — anchored at that single declarer's `.lzi`.
        //      Can only be decided at the package layer (a lone feature can
        //      never satisfy the shared 1:N invariant), so it runs here over
        //      the already-lifted `features` set rather than per-file.
        let sev = resolve(single_feature::Finding::CODE);
        let entries: Vec<(&std::path::Path, &lazuli_ir::Feature)> = features
            .iter()
            .map(|(path, feature)| (path.as_path(), feature))
            .collect();
        for finding in single_feature::check(&entries) {
            let message = finding.message();
            out.push(DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity: sev,
                code: single_feature::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: Some(finding.feature),
                construct: None,
                fix: None,
                group: None,
            });
        }

        // (b) Vault-scanning rules: DANGLING-CITE, STALE, UNGATED-WRITE,
        //     DUP-TOPIC — once per distinct sector. The `knowledge/<sector>/`
        //     walk + git probe both anchor at `self.project_root`.
        for sector in &sectors_to_scan {
            // DANGLING-CITE-001 — file (vault doc cites) ↔ IR symbol.
            let sev = resolve(dangling::Finding::CODE);
            for finding in dangling::check(root, sector, &symbol_index) {
                let message = finding.message();
                out.push(DoctorDiagnostic {
                    path: finding.path,
                    line: 1,
                    column: 1,
                    severity: sev,
                    code: dangling::Finding::CODE.to_owned(),
                    message,
                    category: Some(RuleCategory::Vocabulary),
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // STALE-001 — gold doc past its `revalidate_by` (today injected).
            let sev = resolve(stale::Finding::CODE);
            for finding in stale::check(root, sector, &today) {
                let message = finding.message();
                out.push(DoctorDiagnostic {
                    path: finding.path,
                    line: 1,
                    column: 1,
                    severity: sev,
                    code: stale::Finding::CODE.to_owned(),
                    message,
                    category: Some(RuleCategory::Vocabulary),
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // UNGATED-WRITE-001 — gold doc born-gold in git history. Skips
            // silently when git is unavailable / the path is untracked.
            let sev = resolve(ungated::Finding::CODE);
            for finding in ungated::check(root, sector) {
                let message = finding.message();
                out.push(DoctorDiagnostic {
                    path: finding.path,
                    line: 1,
                    column: 1,
                    severity: sev,
                    code: ungated::Finding::CODE.to_owned(),
                    message,
                    category: Some(RuleCategory::Vocabulary),
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // DUP-TOPIC-001 — two unsuperseded gold docs on one topic.
            let sev = resolve(dup_topic::Finding::CODE);
            for finding in dup_topic::check(root, sector) {
                let message = finding.message();
                // Anchor at the first colliding doc for a stable path.
                let path = finding
                    .docs
                    .first()
                    .cloned()
                    .unwrap_or_else(|| root.clone());
                out.push(DoctorDiagnostic {
                    path,
                    line: 1,
                    column: 1,
                    severity: sev,
                    code: dup_topic::Finding::CODE.to_owned(),
                    message,
                    category: Some(RuleCategory::Vocabulary),
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        out
    }

}
