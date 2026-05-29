//! Additional `impl DoctorPackage` blocks for the layered-coverage and
//! `VOCAB-CONTEXT-*` dispatches that don't fit inside the core
//! `package.rs` (which already owns the load + standard `diagnostics()`
//! path). Kept in a sibling file so each concern stays under the
//! per-file LOC budget; both blocks see the same private fields via
//! `super`.

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor_config::{
    DoctorProfile as SecurityProfile, ResolvedDoctorConfig, SeverityOverride, effective_severity,
    effective_severity_over_base,
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
            vocab_context_ctxmd_001, vocab_context_nongoals_001, vocab_context_purpose_001,
        };

        let preset = self.coverage_preset();
        // `off` preset opts out entirely — mirrors how the coverage
        // layers all zero out under `off`. (The shared resolver also
        // returns `None` for these codes under `Off`; this early return
        // is the equivalent loop-level short-circuit.)
        if matches!(preset, Some(CoveragePreset::Off)) {
            return Vec::new();
        }

        // W1 — build the resolved config once and route every severity
        // decision through `lazuli_doctor_config::effective_severity`.
        // For the VOCAB-CONTEXT family (category Vocabulary, no category
        // preset) this exercises precedence levels 1 (manifest override),
        // 2 (coverage-preset escalation), and 4 (profile default) — the
        // exact union the old hand-rolled `resolve` closure implemented.
        let overrides = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.test_discipline.as_ref())
            .map(|td| {
                td.severity_override
                    .iter()
                    .map(|(code, ov)| {
                        (
                            code.clone(),
                            SeverityOverride {
                                severity: ov.severity.clone(),
                                reason: ov.reason.clone(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let config = ResolvedDoctorConfig {
            profile: self.security_profile.into(),
            coverage_preset: preset,
            overrides,
            ..ResolvedDoctorConfig::default()
        };

        // The VOCAB-CONTEXT codes always resolve to a concrete severity
        // here (the `Off` preset is already short-circuited above and the
        // category default always has an opinion), so the resolver never
        // returns `None`; `Warning` is the unreachable fallback.
        let resolve = |code: &str| -> DoctorSeverity {
            effective_severity(
                code,
                lazuli_doctor::DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &config,
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

                // VOCAB-CONTEXT-CTXMD-001 — passes project_root so
                // sidecar paths resolve relative to the feature `.lzi`
                // first, then to the project root as a fallback.
                let sev = resolve(vocab_context_ctxmd_001::Finding::CODE);
                for finding in
                    vocab_context_ctxmd_001::check(&feature, &file.path, Some(&self.project_root))
                {
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
        let config = ResolvedDoctorConfig {
            profile: self.security_profile.into(),
            coverage_preset: self.coverage_preset(),
            ..ResolvedDoctorConfig::default()
        };
        let resolve = |code: &str| -> Option<DoctorSeverity> {
            effective_severity_over_base(code, base_severity, RuleCategory::Security, &config)
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
            CoveragePreset, CoverageProfile, LayerThreshold, build_coverage_report_with_e2e_root,
            resolve_coverage_thresholds,
        };

        let (features, lzx_views) = self.coverage_inputs();
        let profile = match self.security_profile {
            SecurityProfile::Prototype => CoverageProfile::Prototype,
            SecurityProfile::Strict => CoverageProfile::Strict,
            SecurityProfile::Production => CoverageProfile::Production,
        };

        // Lift `[doctor.coverage]` from the manifest into the resolver
        // inputs. Absent manifest / absent section → empty maps, which
        // makes resolution fall back to the profile defaults verbatim
        // (backwards compatible).
        let (preset, per_layer_overrides, aggregate_method) = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.coverage.as_ref())
            .map(|cov| {
                let preset = cov.preset.as_deref().and_then(CoveragePreset::parse);
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
                (preset, per_layer, cov.aggregate_method.clone())
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
    }
}
