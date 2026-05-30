impl DoctorPackage {
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
            SecurityProfile::Strict
            | SecurityProfile::Production
            | SecurityProfile::IronHand => lazuli_doctor::DoctorSeverity::Error,
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
            SecurityProfile::Strict | SecurityProfile::Production | SecurityProfile::IronHand => {
                CfgSeverity::Error
            }
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
            // iron-hand inherits production's coverage profile; the defaulted
            // tdd-iron-hand coverage preset layers the 90/95 thresholds on top.
            SecurityProfile::Production | SecurityProfile::IronHand => CoverageProfile::Production,
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
