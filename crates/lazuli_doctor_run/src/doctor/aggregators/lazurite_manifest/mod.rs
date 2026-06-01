//! `lazurite_manifest_diagnostics` aggregator.
//!
//! Owns every `DoctorDiagnostic` whose evidence ultimately comes from
//! the `Lazurite.toml` manifest (or `lazurite.toml` legacy casing).
//! The fan-out dispatcher (`lazurite_manifest_diagnostics`) walks each
//! `check_*` helper in turn and concatenates the results.
//!
//! ## Sub-modules
//!
//! - `doctor_config` — `DOCTOR-OVERRIDE-NEEDS-REASON-001`,
//!   `COVERAGE-PRESET-UNKNOWN-001`, `CONFIG-NOISE-001`.
//! - `plugins_manifest` — `PLUGIN-MANIFEST-MISSING`,
//!   `PLUGIN-MANIFEST-SCHEMA-LEGACY`, `PLUGIN-README-MISSING`,
//!   `PLUGIN-CATALOG-DRIFT`.
//! - `plugins_semantic` — `SEMANTIC-PLUGIN-001`,
//!   `SEMANTIC-PLUGIN-002`, `PLUGIN-NOT-DECLARED-001`,
//!   `PLUGIN-UNUSED-001`, `PLUGIN-NAMESPACE-MISMATCH-001`.
//! - `codegen` — `SUBMODULE-DRIFT-001`,
//!   `MIGRATION-STRATEGY-CONFLICT-001`.
//! - `frontend` — `FRONTEND-AUDIENCE-UNKNOWN-001`,
//!   `AUDIENCE-NO-FRONTEND-001`, `FRONTEND-OUT-COLLISION-001`.
//!
//! Visibility rule: every `check_*` helper is `pub(super)`. The only
//! `pub(crate)` symbol is `lazurite_manifest_diagnostics`, exported
//! through `crate::doctor` for the doctor dispatcher.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.
//! Fanned out into per-concern sub-modules in rails-style R7-4.

use crate::doctor::helpers::project_has_lazurite_manifest;
use crate::doctor::{DoctorDiagnostic, DoctorPackage};

mod codegen;
mod doctor_config;
mod frontend;
mod plugin_resolution_view;
mod plugins_manifest;
mod plugins_semantic;

// 0020 — the authoritative plugin alias map (upward-walked root, same
// inputs codegen uses). Re-exported so the `semantic_type_unknown`
// suppression in `dispatch_impl2` consults the IDENTICAL map the
// plugin-semantic checks do — otherwise a `@semantic.<X>` resolved by
// generate would still trip the legacy closed-catalog check when doctor
// runs from a features subdir.
pub(crate) use plugin_resolution_view::authoritative_alias_map;

pub(crate) fn lazurite_manifest_diagnostics(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // 0020 — the plugin-semantic family resolves through the
    // AUTHORITATIVE upward-walked root (the same `find_project_root`
    // codegen uses), loaded by `plugin_resolution_view`. Run `lazuli
    // doctor app` from a features subdir and these checks see the
    // repo-root `Lazurite.toml [plugins]` exactly as `generate go app`
    // does — they no longer short-circuit to silence because no manifest
    // sits at `app/`. The authoritative manifest is loaded ONCE here and
    // threaded to every plugin check so `SEMANTIC-PLUGIN-001` fires iff
    // the same `@semantic.<X>` would leave a residual on the generate
    // path (doctor↔generate agreement). The genuinely-no-`Lazurite.toml`
    // case (no ancestor manifest anywhere) yields no plugin findings.
    let authoritative_root = plugin_resolution_view::authoritative_project_root(package);
    if let Ok(Some(plugin_manifest)) =
        lazuli_manifest::lazurite_manifest::load(&authoritative_root)
    {
        let plugin_manifest = &plugin_manifest;
        diagnostics.extend(plugins_semantic::check_plugin_not_declared(
            plugin_manifest,
            package,
        ));
        diagnostics.extend(plugins_semantic::check_plugin_unused(plugin_manifest, package));
        diagnostics.extend(plugins_semantic::check_plugin_namespace_mismatch(
            plugin_manifest,
            package,
        ));
        diagnostics.extend(plugins_semantic::check_semantic_plugin_unresolved(
            plugin_manifest,
            package,
        ));
        diagnostics.extend(plugins_semantic::check_semantic_plugin_no_validator(
            plugin_manifest,
            package,
        ));
    }

    // The non-plugin manifest checks keep their original behavior: they
    // anchor at `package.project_root` and read the manifest the package
    // loaded from that same root, so they stay silent on a subdir
    // invocation (no manifest at the input root) exactly as before. Only
    // the plugin-semantic resolution view was the divergent surface 0020
    // unifies; widening these path-anchored checks to the repo root is
    // out of scope (it would re-anchor frontend/codegen/config findings).
    if !project_has_lazurite_manifest(&package.project_root) {
        return diagnostics;
    }

    let Some(manifest) = package.lazurite_manifest.as_ref() else {
        return diagnostics;
    };

    diagnostics.extend(plugins_manifest::check_plugin_manifest_missing(
        manifest, package,
    ));
    diagnostics.extend(plugins_manifest::check_plugin_manifest_schema_legacy(
        manifest, package,
    ));
    diagnostics.extend(plugins_manifest::check_plugin_readme_missing(
        manifest, package,
    ));
    diagnostics.extend(plugins_manifest::check_plugin_catalog_drift(
        manifest, package,
    ));
    diagnostics.extend(codegen::check_submodule_drift(manifest, package));
    diagnostics.extend(codegen::check_migration_strategy_conflict(
        manifest, package,
    ));
    diagnostics.extend(frontend::check_frontend_audience_unknown(manifest, package));
    diagnostics.extend(frontend::check_audience_no_frontend(manifest, package));
    diagnostics.extend(frontend::check_frontend_out_collision(manifest, package));
    // Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001`. Fires when any
    // `[doctor.<category>].severity_override.<RULE-CODE>` entry lacks a
    // non-blank `reason` justification.
    diagnostics.extend(doctor_config::check_doctor_override_needs_reason(
        manifest, package,
    ));
    // Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
    // `[doctor.coverage] preset = "<name>"` names a preset that does
    // not exist in `CoveragePreset::parse`. Surfacing this as an error
    // avoids silent "vacuous pass" behavior on a typo.
    diagnostics.extend(doctor_config::check_coverage_preset_unknown(
        manifest, package,
    ));
    // Frente 1 — `CONFIG-NOISE-001`. Warning when a config file's
    // comment ratio is dominated by commentary (more comment lines than
    // semantic lines). Anchors at `Lazurite.toml` and `Lazuli.toml`.
    diagnostics.extend(doctor_config::check_config_noise(package));
    diagnostics
}
