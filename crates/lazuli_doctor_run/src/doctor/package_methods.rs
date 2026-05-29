//! Additional `impl DoctorPackage` blocks for the layered-coverage and
//! `VOCAB-CONTEXT-*` dispatches that don't fit inside the core
//! `package.rs` (which already owns the load + standard `diagnostics()`
//! path). Kept in a sibling file so each concern stays under the
//! per-file LOC budget; both blocks see the same private fields via
//! `super`.

use std::collections::BTreeSet;

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor_config::{
    DoctorProfile as SecurityProfile, effective_severity, effective_severity_over_base,
};
use lazuli_syntax::parse_feature_skeletons;

use super::parsers::is_lzi_path;
use super::{DoctorDiagnostic, DoctorPackage, DoctorSeverity, RuleCategory};

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

    /// SESSION-QUERY-TEMPORAL-VALIDITY-001 — IR-driven auth/session
    /// security invariant. Dispatches
    /// [`auth::session_query_temporal_validity_001::check`] across every
    /// `.lzi` feature, re-parsing the typed `Feature` IR so the rule can
    /// read `Query.filters` + the session binding (neither survives on
    /// the fact-only `AuthFacts`/`ResourceFact` slices the
    /// `aggregators::auth` dispatcher uses).
    ///
    /// Severity follows the session-family enforcement posture (parity
    /// with the LSP's [`is_security_enforcement_code`] peers
    /// `auth-session-ttl` / `auth_sessions_resource_unknown`): **WARNING**
    /// under the prototype profile, **ERROR** under strict/production —
    /// so under the scaffolded `[doctor] profile = "strict"` it blocks.
    /// A manifest `severity_override.<code>` (under either the kebab or
    /// snake code) still wins via the shared resolver.
    ///
    /// [`is_security_enforcement_code`]: crate::security_profile
    pub(super) fn session_query_temporal_validity_diagnostics(&self) -> Vec<DoctorDiagnostic> {
        use super::auth::session_query_temporal_validity_001 as rule;
        use super::helpers::line_col_for_offset;

        // Enforcement-code posture: prototype warns, strict/production
        // block. Mirrors `security_profile::apply_security_profile` so the
        // engine severity matches the in-editor severity for this code.
        // Computed in the config-side `lazuli_doctor::DoctorSeverity` so it
        // feeds `effective_severity_over_base` directly; the result is
        // mapped back to the local `DoctorSeverity` via `From`.
        let base_severity = match self.security_profile {
            SecurityProfile::Prototype => lazuli_doctor::DoctorSeverity::Warning,
            SecurityProfile::Strict | SecurityProfile::Production => {
                lazuli_doctor::DoctorSeverity::Error
            }
        };

        // A manifest override (either spelling) still wins, and `off`
        // coverage suppression is honored, via the shared resolver flooring
        // on the enforcement base. Security category keeps levels 1-3.
        //
        // v2 — the config (profile + coverage preset + per-rule overrides)
        // is the caller-supplied `self.config` (CLI: disk; LSP: unsaved
        // buffer), so a buffered `severity_override` for either code
        // spelling takes effect in-editor.
        let config = &self.config;
        let resolve = |code: &str| -> Option<DoctorSeverity> {
            effective_severity_over_base(code, base_severity, RuleCategory::Security, config)
                .map(DoctorSeverity::from)
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
                for finding in rule::check(&feature, &file.path) {
                    // Resolve severity under both spellings so an override
                    // keyed to either the kebab or snake code applies; the
                    // kebab spelling (LSP/profile family) takes precedence.
                    let severity = match resolve(rule::Finding::KEBAB_CODE)
                        .or_else(|| resolve(rule::Finding::CODE))
                    {
                        Some(sev) => sev,
                        // Suppressed under the active config (coverage-off).
                        None => continue,
                    };
                    let (line, column) = finding
                        .offset
                        .map(|offset| line_col_for_offset(&file.source, offset))
                        .unwrap_or((1, 1));
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line,
                        column,
                        severity,
                        code: rule::Finding::KEBAB_CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Security),
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

    /// `SESSION-COOKIE-*` — the five IR-driven session-cookie transport
    /// diagnostics over `auth.sessions.cookie`. Dispatches each rule's
    /// `check` across every `.lzi` feature, re-parsing the typed `Feature`
    /// IR (the fact-only `AuthFacts` slice the `aggregators::auth`
    /// dispatcher uses does not carry the cookie sub-block, exactly as for
    /// `session_query_temporal_validity_diagnostics`).
    ///
    /// Two severity postures, all under the Security category:
    ///   - **Blocking** (`SESSION-COOKIE-INSECURE-IN-PROD-001`,
    ///     `SESSION-COOKIE-SAMESITE-NONE-INSECURE-001`): WARNING under
    ///     prototype, ERROR under strict/production — they encode
    ///     browser-reject / production-replay shapes that break the wire.
    ///   - **Hygiene** (`SESSION-COOKIE-MISSING-001`,
    ///     `SESSION-COOKIE-PROFILE-CONFLICT-001`,
    ///     `SESSION-COOKIE-HOST-PREFIX-VIOLATION-001`): WARNING on every
    ///     profile — missing-declaration / resolvable-precedence /
    ///     naming-convention smells, not wire-breaking, so they inform
    ///     rather than block.
    ///
    /// A manifest `severity_override.<CODE>` still wins and `off` coverage
    /// suppression is honored, via the shared `effective_severity_over_base`
    /// resolver flooring on each rule's base.
    ///
    /// `INSECURE-IN-PROD` is profile-scoped: it only emits when the
    /// resolved deploy profile is `production` (the `is_production` flag).
    /// `MISSING` / `PROFILE-CONFLICT` additionally read the app-wide
    /// `app.cookie` block off the package's loaded `AppManifest`.
    pub(super) fn session_cookie_diagnostics(&self) -> Vec<DoctorDiagnostic> {
        use lazuli_doctor::DoctorSeverity as CfgSeverity;

        use super::auth::{
            session_cookie_host_prefix_violation_001 as host_prefix,
            session_cookie_insecure_in_prod_001 as insecure_prod,
            session_cookie_missing_001 as missing, session_cookie_profile_conflict_001 as conflict,
            session_cookie_samesite_none_insecure_001 as samesite_none,
        };
        use super::helpers::line_col_for_offset;

        let is_production = matches!(self.security_profile, SecurityProfile::Production);

        // Blocking base: prototype warns, strict/production block (parity
        // with the session-family enforcement posture).
        let blocking_base = match self.security_profile {
            SecurityProfile::Prototype => CfgSeverity::Warning,
            SecurityProfile::Strict | SecurityProfile::Production => CfgSeverity::Error,
        };
        // Hygiene base: WARNING on every profile. Used by the two rules
        // that only ever fire when the author HAS declared a `cookie` block
        // (PROFILE-CONFLICT, HOST-PREFIX) — they never touch a feature that
        // declares no cookie child, so they cannot regress back-compat
        // fixtures.
        let hygiene_base = CfgSeverity::Warning;
        // Advisory base: HINT on every profile. MISSING is the one rule
        // that fires on cookie *absence* (rotation/refresh with no pinned
        // transport at any anchor). It is a nudge to declare the transport
        // envelope, not a wire-breaking defect, and the runtime already
        // stamps safe session-cookie defaults — so it rides the softest,
        // never-blocking tier. This keeps existing rotation apps that lean
        // on runtime defaults (no `cookie` child yet) free of any
        // warning/error regression while still surfacing the guidance.
        let advisory_base = CfgSeverity::Hint;

        // v2 — caller-supplied severity config (CLI: disk; LSP: unsaved
        // buffer). Carries the profile, coverage preset, and per-rule
        // overrides the SESSION-COOKIE family resolves through.
        let config = &self.config;
        let resolve = |code: &str, base: CfgSeverity| -> Option<DoctorSeverity> {
            effective_severity_over_base(code, base, RuleCategory::Security, config)
                .map(DoctorSeverity::from)
        };

        // App-wide `app.cookie` block (shared across all features in the
        // package). Read once off the loaded manifest IR.
        let app_cookie = self.app.as_ref().and_then(|a| a.manifest.cookie.as_ref());

        // Second app anchor for session-cookie coverage: the refresh-cookie
        // capability declaration (`refresh_token_storage cookie` or a
        // `cookie_domain` capability). The proposal pins these edge
        // capabilities to the session cookie's transport, so an app that
        // declares them has expressed where the refresh cookie lives — the
        // `MISSING` rule treats that as coverage. Mirrors the capability
        // scan in `auth_refresh::rules::ProjectMarkers::from_app`.
        let app_declares_cookie = self
            .app
            .as_ref()
            .map(|a| {
                a.manifest.capabilities.iter().any(|cap| {
                    (cap.name == "refresh_token_storage" && cap.value == "cookie")
                        || cap.name == "cookie_domain"
                })
            })
            .unwrap_or(false);

        // Helper that pushes one diagnostic from a (path, feature, offset,
        // message) tuple under the given code/severity.
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

                // SESSION-COOKIE-INSECURE-IN-PROD-001 (blocking, prod-scoped)
                if let Some(sev) = resolve(insecure_prod::Finding::CODE, blocking_base) {
                    for f in insecure_prod::check(&feature, &file.path, is_production) {
                        let (line, column) = f
                            .offset
                            .map(|o| line_col_for_offset(&file.source, o))
                            .unwrap_or((1, 1));
                        let message = f.message();
                        out.push(DoctorDiagnostic {
                            path: f.path,
                            line,
                            column,
                            severity: sev,
                            code: insecure_prod::Finding::CODE.to_owned(),
                            message,
                            category: Some(RuleCategory::Security),
                            feature_name: Some(f.feature),
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }

                // SESSION-COOKIE-SAMESITE-NONE-INSECURE-001 (blocking)
                if let Some(sev) = resolve(samesite_none::Finding::CODE, blocking_base) {
                    for f in samesite_none::check(&feature, &file.path) {
                        let (line, column) = f
                            .offset
                            .map(|o| line_col_for_offset(&file.source, o))
                            .unwrap_or((1, 1));
                        let message = f.message();
                        out.push(DoctorDiagnostic {
                            path: f.path,
                            line,
                            column,
                            severity: sev,
                            code: samesite_none::Finding::CODE.to_owned(),
                            message,
                            category: Some(RuleCategory::Security),
                            feature_name: Some(f.feature),
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }

                // SESSION-COOKIE-MISSING-001 (advisory; reads both app anchors)
                if let Some(sev) = resolve(missing::Finding::CODE, advisory_base) {
                    for f in missing::check(&feature, &file.path, app_cookie, app_declares_cookie) {
                        let message = f.message();
                        out.push(DoctorDiagnostic {
                            path: f.path,
                            line: 1,
                            column: 1,
                            severity: sev,
                            code: missing::Finding::CODE.to_owned(),
                            message,
                            category: Some(RuleCategory::Security),
                            feature_name: Some(f.feature),
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }

                // SESSION-COOKIE-PROFILE-CONFLICT-001 (hygiene; reads app.cookie)
                if let Some(sev) = resolve(conflict::Finding::CODE, hygiene_base) {
                    for f in conflict::check(&feature, &file.path, app_cookie) {
                        let message = f.message();
                        out.push(DoctorDiagnostic {
                            path: f.path,
                            line: 1,
                            column: 1,
                            severity: sev,
                            code: conflict::Finding::CODE.to_owned(),
                            message,
                            category: Some(RuleCategory::Security),
                            feature_name: Some(f.feature),
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }

                // SESSION-COOKIE-HOST-PREFIX-VIOLATION-001 (hygiene)
                if let Some(sev) = resolve(host_prefix::Finding::CODE, hygiene_base) {
                    for f in host_prefix::check(&feature, &file.path) {
                        let message = f.message();
                        out.push(DoctorDiagnostic {
                            path: f.path,
                            line: 1,
                            column: 1,
                            severity: sev,
                            code: host_prefix::Finding::CODE.to_owned(),
                            message,
                            category: Some(RuleCategory::Security),
                            feature_name: Some(f.feature),
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
            }
        }
        out
    }

    /// Wave 6 — `lazuli doctor --coverage` data path. Builds the per-layer
    /// coverage report using the active `SecurityProfile`, the optional
    /// `[doctor.coverage] preset = "<name>"` opt-in (Frente 1), and any
    /// per-layer `[doctor.coverage.<layer>]` overrides authored in
    /// `Lazurite.toml`.
    ///
    /// Resolution precedence (highest wins):
    ///   1. per-layer `[doctor.coverage.<layer>]` block
    ///   2. `[doctor.coverage] preset = "<name>"` (Frente 1)
    ///   3. profile-default thresholds (`profile_default_thresholds`)
    ///
    /// Unknown preset names are silently ignored at this layer (a doctor
    /// diagnostic flags them via `check_coverage_preset_unknown`).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let report = doctor_package.coverage_report();
    /// ```
    pub fn coverage_report(&self) -> lazuli_doctor::coverage::CoverageReport {
        use std::collections::BTreeMap;
        use std::path::PathBuf;

        use lazuli_doctor::coverage::{
            CoverageProfile, LayerThreshold, build_coverage_report_with_e2e_root,
            resolve_coverage_thresholds,
        };

        let (features, lzx_views) = self.coverage_inputs();
        let profile = match self.security_profile {
            SecurityProfile::Prototype => CoverageProfile::Prototype,
            SecurityProfile::Strict => CoverageProfile::Strict,
            SecurityProfile::Production => CoverageProfile::Production,
        };

        // v2 — the coverage PRESET (a severity/threshold-escalation input)
        // rides the caller-supplied `self.config` (CLI: disk; LSP: unsaved
        // buffer), so an unsaved `[doctor.coverage] preset` edit drives the
        // in-editor coverage rollup. The per-layer thresholds + aggregate
        // method are non-preset coverage-gating knobs that stay sourced
        // from the on-disk manifest. Absent manifest / absent section →
        // empty maps, falling back to the profile defaults (back-compat).
        let preset = self.config.coverage_preset;
        let (per_layer_overrides, aggregate_method) = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.coverage.as_ref())
            .map(|cov| {
                let per_layer: BTreeMap<String, LayerThreshold> = cov
                    .per_layer
                    .iter()
                    .map(|(name, cfg)| {
                        (
                            name.clone(),
                            LayerThreshold {
                                block_under: cfg.block_under,
                                warn_under: cfg.warn_under,
                            },
                        )
                    })
                    .collect();
                (per_layer, cov.aggregate_method.clone())
            })
            .unwrap_or_default();

        let thresholds =
            resolve_coverage_thresholds(profile, preset, per_layer_overrides, aggregate_method);

        let e2e_discovery_root: Option<PathBuf> = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.testing.as_ref())
            .and_then(|t| t.playwright.as_ref())
            .and_then(|pw| pw.discovery_root.as_deref())
            .map(PathBuf::from);

        build_coverage_report_with_e2e_root(
            &features,
            &lzx_views,
            profile,
            &thresholds,
            Some(&self.project_root),
            e2e_discovery_root.as_deref(),
        )
    }
}

/// Today's date as an ISO `YYYY-MM-DD` string, for the `VOCAB-KNOWLEDGE-STALE-001`
/// `revalidate_by` comparison.
///
/// Derived from the system clock with a stdlib-only civil-date conversion
/// (Howard Hinnant's `days -> y/m/d` algorithm) — no `chrono`/`time`
/// dependency, keeping the helper wire-thin per the founding principle. The
/// rule does the lexical compare; this just supplies the reference date the
/// same way the doctor walker would pass `ctx.now`'s date. Falls back to the
/// canonical dev pivot if the clock is before the Unix epoch (unreachable in
/// practice).
fn current_iso_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if secs == 0 {
        // Pre-epoch / clock unreadable: anchor at the Lazuli dev pivot so the
        // rule stays deterministic rather than firing on a bogus date.
        return "2026-05-29".to_string();
    }
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert a count of days since the Unix epoch (1970-01-01) into a
/// `(year, month, day)` Gregorian date. Hinnant's branch-free algorithm;
/// valid for the full proleptic Gregorian range.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    // Smoke pairing — the methods in this file dispatch into rich
    // analyzer state built by the parent `DoctorPackage`, which the
    // unit tests under `crates/lazuli_cli/tests` already cover end-to-
    // end. We just guard against the public surface disappearing.
    use super::*;

    #[test]
    fn impl_block_compiles() {
        let _ = DoctorPackage::coverage_report;
        let _ = DoctorPackage::knowledge_vocab_diagnostics;
    }

    #[test]
    fn civil_from_days_known_anchors() {
        // Epoch and a few well-known dates.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18_993), (2022, 1, 1));
        assert_eq!(civil_from_days(20_602), (2026, 5, 29));
        assert_eq!(civil_from_days(20_605), (2026, 6, 1));
    }

    #[test]
    fn current_iso_date_is_well_formed() {
        let s = current_iso_date();
        assert_eq!(s.len(), 10, "YYYY-MM-DD is 10 chars: {s}");
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
        // Sanity: year is in the plausible modern range.
        let year: i64 = s[0..4].parse().expect("year parses");
        assert!((2020..2100).contains(&year), "year out of range: {year}");
    }
}
