//! `lazurite_manifest_diagnostics` aggregator.
//!
//! Owns every `DoctorDiagnostic` whose evidence ultimately comes from
//! the `Lazurite.toml` manifest (or `lazurite.toml` legacy casing).
//! The fan-out dispatcher (`lazurite_manifest_diagnostics`) walks each
//! `check_*` helper in turn and concatenates the results.
//!
//! Diagnostic families covered, by topic:
//!
//! * Plugin declaration hygiene — `PLUGIN-NOT-DECLARED-001`,
//!   `PLUGIN-UNUSED-001`, `PLUGIN-NAMESPACE-MISMATCH-001`,
//!   `SEMANTIC-PLUGIN-001` / `SEMANTIC-PLUGIN-002`,
//!   `PLUGIN-MANIFEST-MISSING`, `PLUGIN-MANIFEST-SCHEMA-LEGACY`,
//!   `PLUGIN-README-MISSING`, `PLUGIN-CATALOG-DRIFT`.
//! * Project-level codegen hygiene — `SUBMODULE-DRIFT`,
//!   `MIGRATION-STRATEGY-CONFLICT-001`.
//! * Frontend / audience contracts — `FRONTEND-AUDIENCE-UNKNOWN-001`,
//!   `AUDIENCE-NO-FRONTEND-001`, `FRONTEND-OUT-COLLISION-001`.
//! * Doctor configuration hygiene — `DOCTOR-OVERRIDE-NEEDS-REASON-001`,
//!   `COVERAGE-PRESET-UNKNOWN-001`, `CONFIG-NOISE-001`.
//!
//! Visibility rule: every helper is `pub(crate)` so the aggregator
//! dispatcher in mod.rs can reach the dispatcher entry point
//! `lazurite_manifest_diagnostics`. Per-`check_*` helpers are
//! `pub(crate)` only for the regression-test harness in
//! `doctor/tests.rs`; nothing outside the doctor module should call
//! them directly.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::doctor::aggregators::cross_feature::collect_known_audiences;
use crate::doctor::helpers::project_has_lazurite_manifest;
use crate::doctor::parsers::is_lzi_path;
use crate::doctor::{
    DoctorDiagnostic, DoctorPackage, DoctorSeverity, RuleCategory, collect_at_references_in_source,
    collect_package_plugin_references, doctor_severity_for, go_mod_lazuli_runtime_version,
    is_allowed_reference_namespace_for_doctor,
};
use crate::lazurite_manifest::{Manifest, MigrationStrategy};

pub(crate) fn lazurite_manifest_diagnostics(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    if !project_has_lazurite_manifest(&package.project_root) {
        return Vec::new();
    }

    let Some(manifest) = package.lazurite_manifest.as_ref() else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    diagnostics.extend(check_plugin_not_declared(manifest, package));
    diagnostics.extend(check_plugin_unused(manifest, package));
    diagnostics.extend(check_plugin_namespace_mismatch(manifest, package));
    diagnostics.extend(check_semantic_plugin_unresolved(manifest, package));
    diagnostics.extend(check_semantic_plugin_no_validator(manifest, package));
    diagnostics.extend(check_plugin_manifest_missing(manifest, package));
    diagnostics.extend(check_plugin_manifest_schema_legacy(manifest, package));
    diagnostics.extend(check_plugin_readme_missing(manifest, package));
    diagnostics.extend(check_plugin_catalog_drift(manifest, package));
    diagnostics.extend(check_submodule_drift(manifest, package));
    diagnostics.extend(check_migration_strategy_conflict(manifest, package));
    diagnostics.extend(check_frontend_audience_unknown(manifest, package));
    diagnostics.extend(check_audience_no_frontend(manifest, package));
    diagnostics.extend(check_frontend_out_collision(manifest, package));
    // Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001`. Fires when any
    // `[doctor.<category>].severity_override.<RULE-CODE>` entry lacks a
    // non-blank `reason` justification.
    diagnostics.extend(check_doctor_override_needs_reason(manifest, package));
    // Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
    // `[doctor.coverage] preset = "<name>"` names a preset that does
    // not exist in `CoveragePreset::parse`. Surfacing this as an error
    // avoids silent "vacuous pass" behavior on a typo.
    diagnostics.extend(check_coverage_preset_unknown(manifest, package));
    // Frente 1 — `CONFIG-NOISE-001`. Warning when a config file's
    // comment ratio is dominated by commentary (more comment lines than
    // semantic lines). Anchors at `Lazurite.toml` and `Lazuli.toml`.
    diagnostics.extend(check_config_noise(package));
    diagnostics
}

/// Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001` dispatcher.
///
/// Lifts the `[doctor.test_discipline].severity_override` table from
/// the parsed manifest into the rule's portable `OverrideEntry` shape,
/// invokes the rule, and maps findings to `DoctorDiagnostic`. Anchors
/// the diagnostic at `Lazurite.toml` line 1 (the rule's structural
/// payload doesn't yet carry exact TOML line spans; that refinement
/// lands post-Wave-0.5 once the toml crate exposes spans cleanly).
pub(crate) fn check_doctor_override_needs_reason(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use crate::lazurite_manifest as lzr;
    use lazuli_doctor::test_discipline::override_needs_reason_001::{self, OverrideEntry};

    let Some(doctor) = manifest.doctor.as_ref() else {
        return Vec::new();
    };
    let mut entries: Vec<OverrideEntry> = Vec::new();
    if let Some(td) = doctor.test_discipline.as_ref() {
        for (code, ov) in td.severity_override.iter() {
            let _: &lzr::SeverityOverride = ov; // keep the type pinned
            entries.push(OverrideEntry {
                category: RuleCategory::TestDiscipline.as_str().to_owned(),
                rule_code: code.clone(),
                severity: ov.severity.clone(),
                reason: ov.reason.clone(),
            });
        }
    }

    let manifest_path = package.project_root.join(lzr::MANIFEST_FILENAME);
    let findings = override_needs_reason_001::check(&entries, &manifest_path);
    findings
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            let severity = doctor_severity_for(
                override_needs_reason_001::Finding::CODE,
                RuleCategory::TestDiscipline,
                package.security_profile,
                &std::collections::BTreeMap::new(),
            );
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity,
                code: override_needs_reason_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::TestDiscipline),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
/// `[doctor.coverage] preset = "<name>"` names a preset that
/// `CoveragePreset::parse` does not recognize. Listing the recognized
/// preset names in the message keeps the diagnostic self-explanatory
/// to an LLM authoring `Lazurite.toml` cold.
pub(crate) fn check_coverage_preset_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::coverage::CoveragePreset;

    let Some(preset_name) = manifest
        .doctor
        .as_ref()
        .and_then(|d| d.coverage.as_ref())
        .and_then(|c| c.preset.as_deref())
    else {
        return Vec::new();
    };
    if CoveragePreset::parse(preset_name).is_some() {
        return Vec::new();
    }
    vec![DoctorDiagnostic {
        path: package
            .project_root
            .join(crate::lazurite_manifest::MANIFEST_FILENAME),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "COVERAGE-PRESET-UNKNOWN-001".to_owned(),
        message: format!(
            "[doctor.coverage] preset = \"{preset_name}\" is not a recognized preset. \
             Allowed values: tdd-strict, tdd-mature, off. \
             Omit the field to fall back to the security-profile defaults."
        ),
        category: Some(RuleCategory::Vocabulary),
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// Frente 1 — `CONFIG-NOISE-001`. Warns when the comment ratio of a
/// top-level config file exceeds 1:1 (more comment lines than semantic
/// lines). The signal: when the user's config file is mostly inline
/// commentary, the framework is hiding intent behind explanation. The
/// fix: push the explanation into framework defaults / canonical docs.
///
/// Severity is Warning by design — the rule is informational and
/// never gates. Scope: `Lazurite.toml` (and the legacy lowercase
/// `lazurite.toml`). `.lzi` / `.lzx` follow in a future cycle once
/// the comment-vs-statement counter understands Lazuli syntax.
///
/// Heuristic logic + 6 unit tests live in
/// `lazuli_doctor::config_noise`; this function only stitches the
/// metrics to a `DoctorDiagnostic`.
pub(crate) fn check_config_noise(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::config_noise::config_noise_metrics;
    let mut diagnostics = Vec::new();
    // Prefer the canonical capitalized name; fall back to legacy only
    // if canonical is absent (mirrors `lazurite_manifest::load`). On
    // case-insensitive filesystems both `exists()` calls would
    // otherwise report the same file twice and double-fire.
    let canonical = package
        .project_root
        .join(crate::lazurite_manifest::MANIFEST_FILENAME);
    let legacy = package
        .project_root
        .join(crate::lazurite_manifest::LEGACY_MANIFEST_FILENAME);
    let (path, filename) = if canonical.exists() {
        (canonical, crate::lazurite_manifest::MANIFEST_FILENAME)
    } else if legacy.exists() {
        (legacy, crate::lazurite_manifest::LEGACY_MANIFEST_FILENAME)
    } else {
        return diagnostics;
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return diagnostics;
    };
    let metrics = config_noise_metrics(&contents);
    if metrics.fires() {
        diagnostics.push(DoctorDiagnostic {
            path,
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "CONFIG-NOISE-001".to_owned(),
            message: format!(
                "{filename} has {} comment line(s) vs {} semantic line(s) (ratio {:.2}:1). \
                 When commentary exceeds config, the framework is leaking \
                 intent into the user's file — push the knowledge into framework \
                 defaults or canonical docs. See docs/canonical-semantics.md#config-hygiene.",
                metrics.comment_lines,
                metrics.semantic_lines,
                metrics.ratio()
            ),
            category: Some(RuleCategory::Vocabulary),
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}

/// PLUGIN-MANIFEST-MISSING (error) — every plugin declared in
/// `Lazurite.toml [plugins]` with a resolvable local path must ship a
/// `manifest.toml` at its root. Today the framework silently skips
/// plugins without a manifest (the alias-builder pass returns
/// `Ok(None)`); doctor escalates that to an error so the plugin
/// catalog stays self-describing.
///
/// Remote plugins without a `dev.plugin_paths` override skip the
/// check (the manifest isn't on the local filesystem at all — a
/// different diagnostic class).
pub(crate) fn check_plugin_manifest_missing(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        if manifest_path.exists() {
            continue;
        }
        diagnostics.push(DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-MANIFEST-MISSING".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` is missing `manifest.toml`. Every plugin must declare a `[plugin]` block (name + namespace + go_module + ts_package) so the catalog stays self-describing. Add `manifest.toml` to the plugin root or remove the plugin from Lazurite.toml [plugins].",
                plugin_root.display(),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}

/// PLUGIN-MANIFEST-SCHEMA-LEGACY (error) — every plugin
/// `manifest.toml` MUST declare a `[plugin]` block carrying
/// `name`/`namespace`/`go_module` (per
/// `crate::plugin_manifest::PluginManifest`). Some older plugins
/// (pre-2026-05-12, before the lazuli-ops 85ff076 cutover) used a
/// flat top-level `name`/`version`/`implements` shape that the
/// loader accepts silently — codegen falls back to v1 conventions
/// and the LSP catalog shows the plugin under its DSL ref, but
/// every downstream feature is degraded.
///
/// Wave §A4 hard-cutover (2026-05-23): all 10 known legacy plugins
/// have been migrated; any remaining legacy manifest is a bug, so
/// this lint runs at Error severity from day one.
pub(crate) fn check_plugin_manifest_schema_legacy(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            // PLUGIN-MANIFEST-MISSING already covers absence.
            continue;
        };
        // Parse as raw TOML so we can inspect for a `[plugin]` table.
        // The PluginManifest deserializer treats `plugin` as optional,
        // so a `PluginManifest::default()` round-trips a legacy
        // manifest cleanly — that's exactly the silent path we close
        // here.
        let Ok(value) = text.parse::<toml::Value>() else {
            // TOML syntax error is a separate concern; let the loader
            // surface it through its own error path.
            continue;
        };
        let table = match value.as_table() {
            Some(t) => t,
            None => continue,
        };
        if table.contains_key("plugin") {
            continue; // canonical v1 shape — no diagnostic
        }
        diagnostics.push(DoctorDiagnostic {
            path: manifest_path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-MANIFEST-SCHEMA-LEGACY".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` uses the legacy flat manifest schema (top-level `name`/`version`/`implements`). Migrate to the v1 schema with a `[plugin]` block declaring `namespace`, `name`, `go_module`, and `ts_package` (optional). See `docs/proposals/plugin-manifest-v1-hard-cutover-2026-05-23.md` (wave §A4).",
                manifest_path.display(),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}

/// PLUGIN-README-MISSING (warning) — every plugin with a resolvable
/// local path should ship a `README.md`. Authors of new pilots (and
/// new plugin contributors) rely on the README to understand the
/// surface; missing READMEs silently degrade the catalog quality.
pub(crate) fn check_plugin_readme_missing(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        // Skip when the manifest itself is missing — the manifest lint
        // anchors that failure mode, no need to double-flag.
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        if !manifest_path.exists() {
            continue;
        }
        let readme_path = plugin_root.join("README.md");
        if readme_path.exists() {
            continue;
        }
        diagnostics.push(DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "PLUGIN-README-MISSING".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` is missing `README.md`. Plugins should ship a README documenting their surface (Go fns, TS fns, manifest scalars). The catalog page derives from it.",
                plugin_root.display(),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}

/// PLUGIN-CATALOG-DRIFT (warning) — `dist/plugin-catalog.json` is
/// expected to be regenerated whenever a plugin's manifest or README
/// changes. When the catalog's mtime predates any plugin's
/// `manifest.toml` or `README.md`, the catalog is stale and the LSP /
/// docs site / `lazuli plugins` CLI will show outdated info.
///
/// Quietly skips when the catalog file doesn't exist yet (the next
/// `lazuli generate ts` will produce it) and when no plugins are
/// declared. Spec: `docs/proposals/plugin-catalog-file-2026-05-23.md`.
pub(crate) fn check_plugin_catalog_drift(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if manifest.plugins.is_empty() {
        return Vec::new();
    }
    let catalog_path = package
        .project_root
        .join("dist")
        .join("plugin-catalog.json");
    let Ok(catalog_meta) = std::fs::metadata(&catalog_path) else {
        return Vec::new();
    };
    let Ok(catalog_mtime) = catalog_meta.modified() else {
        return Vec::new();
    };

    let mut stale_sources: Vec<String> = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        for relpath in [
            crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME,
            "README.md",
        ] {
            let p = plugin_root.join(relpath);
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else { continue };
            if mtime > catalog_mtime {
                stale_sources.push(format!("{plugin_ref} ({relpath})"));
                break;
            }
        }
    }

    if stale_sources.is_empty() {
        return Vec::new();
    }
    stale_sources.sort();

    vec![DoctorDiagnostic {
        path: catalog_path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "PLUGIN-CATALOG-DRIFT".to_owned(),
        message: format!(
            "`dist/plugin-catalog.json` is older than {} plugin source(s) ({}). Run `lazuli generate ts` to refresh the catalog so the LSP / docs site / `lazuli plugins` CLI see current plugin info.",
            stale_sources.len(),
            stale_sources.join(", "),
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// SEMANTIC-PLUGIN-002 (B4) — `@semantic.<Name>` references that
/// resolve to a plugin scalar with NO `validator` declared. The type
/// alias exists but no runtime check enforces it; the field accepts
/// any string at the wire boundary. Warn-level: some plugins ship
/// brand aliases intentionally without validation.
///
/// Source-of-truth: `docs/proposals/ir-semantic-auto-validate-2026-05-22.md`
/// (W2 §"Doctor B4").
pub(crate) fn check_semantic_plugin_no_validator(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let alias_map =
        match crate::plugin_manifest::build_alias_map(Some(manifest), &package.project_root) {
            Ok(map) => map,
            Err(_) => return Vec::new(), // SEMANTIC-PLUGIN-001 already covers this
        };
    let mut diagnostics = Vec::new();
    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            let Some(rest) = reference.reference.strip_prefix("@semantic.") else {
                continue;
            };
            let head = rest.split('(').next().unwrap_or(rest);
            let alias = format!("@semantic.{}", head);
            let Some(resolved) = alias_map.get(&alias) else {
                continue;
            };
            if !resolved.validator.is_empty() {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: reference.path.clone(),
                line: reference.line,
                column: reference.column,
                severity: DoctorSeverity::Warning,
                code: "SEMANTIC-PLUGIN-002".to_owned(),
                message: format!(
                    "plugin semantic type `{alias}` from `{}` does not declare a `validator` in its manifest. The type alias is accepted, but no runtime check enforces the value — invalid input is silently stored. Add a `validator` to the plugin's `[[semantic_types]]` entry, or annotate the field with `@validate.skip` to acknowledge the bypass.",
                    resolved.plugin_namespace
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    diagnostics
}

/// SEMANTIC-PLUGIN-001 — `@semantic.<Name>` references in `.lzi` files
/// that resolve neither against the built-in closed catalog nor any
/// plugin's `manifest.toml`. Per
/// `docs/proposals/semantic-types-plugin-locales.md` §New diagnostics.
///
/// Three failure modes share the diagnostic code:
/// 1. Plugin not declared in `Lazurite.toml [plugins]` (the source of
///    truth for alias activation).
/// 2. Plugin manifest missing or malformed.
/// 3. Two or more active plugins declare the same alias (conflict).
///
/// The shared error code is intentional — every failure has the same
/// resolution path (declare the right plugin, fix the manifest).
pub(crate) fn check_semantic_plugin_unresolved(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    // Build the alias map. Map-construction errors (conflict, mismatch,
    // unsupported carrier) surface as SEMANTIC-PLUGIN-001 anchored at
    // the project root because they're project-wide.
    let alias_map = match crate::plugin_manifest::build_alias_map(
        Some(manifest),
        &package.project_root,
    ) {
        Ok(map) => map,
        Err(err) => {
            return vec![DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "SEMANTIC-PLUGIN-001".to_owned(),
                message: format!(
                    "plugin semantic alias map: {}. Fix the affected plugin manifest under [plugins] in Lazurite.toml.",
                    err
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }];
        }
    };

    // Closed catalog of built-in `@semantic.<X>` names; matches the
    // analyzer's `type_ref_from_syntax` match arm. Authors writing one
    // of these never hit the plugin path.
    const BUILT_IN_SEMANTIC: &[&str] = &[
        "Email", "Phone", "Url", "Uuid", "Currency", "GeoPoint", "Money",
    ];

    let mut diagnostics = Vec::new();
    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        // Walk every `@semantic.<Name>` reference. The shared
        // `collect_at_references_in_source` picks up the full set of
        // `@namespace.name` references; we filter to `semantic` here.
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            // `reference.reference` is the raw `@semantic.<Name>` text.
            let Some(rest) = reference.reference.strip_prefix("@semantic.") else {
                continue;
            };
            // Built-ins resolve syntactically — never SEMANTIC-PLUGIN-001.
            if BUILT_IN_SEMANTIC.contains(&rest) {
                continue;
            }
            // `@semantic.Money(currency:USD)` lifts via parens — pick
            // the head token before `(` so we don't false-flag a typed
            // money reference.
            let head = rest.split('(').next().unwrap_or(rest);
            if BUILT_IN_SEMANTIC.contains(&head) {
                continue;
            }
            // Strip any trailing non-name punctuation (whitespace lifts
            // already drop everything after the first non-ident char,
            // but defensive normalisation here helps when the reference
            // came from a typed-block line ending in `@validator.x`).
            let alias = format!("@semantic.{}", head);
            if alias_map.contains_key(&alias) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: reference.path.clone(),
                line: reference.line,
                column: reference.column,
                severity: DoctorSeverity::Error,
                code: "SEMANTIC-PLUGIN-001".to_owned(),
                message: format!(
                    "unknown plugin semantic type `{alias}`. No plugin in Lazurite.toml [plugins] declares this alias. Add the appropriate `@lazuli/plugin-<name>` to [plugins] or replace the field with a built-in `@semantic.*` type.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    diagnostics
}

pub(crate) fn check_plugin_not_declared(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let declared: BTreeSet<&str> = manifest.plugins.keys().map(|key| key.as_str()).collect();
    collect_package_plugin_references(package)
        .into_iter()
        .filter(|reference| !declared.contains(reference.reference.as_str()))
        .map(|reference| DoctorDiagnostic {
            path: reference.path,
            line: reference.line,
            column: reference.column,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-NOT-DECLARED-001".to_owned(),
            message: format!(
                "`.lzi` references `{}`, but Lazurite.toml does not declare it in `[plugins]`.",
                reference.reference
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(crate) fn check_plugin_unused(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let used: BTreeSet<String> = collect_package_plugin_references(package)
        .into_iter()
        .map(|reference| reference.reference)
        .collect();

    manifest
        .plugins
        .keys()
        .filter(|plugin_ref| !used.contains(*plugin_ref))
        .map(|plugin_ref| DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "PLUGIN-UNUSED-001".to_owned(),
            message: format!(
                "Lazurite.toml declares `{plugin_ref}`, but no `.lzi` file references it."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(crate) fn check_plugin_namespace_mismatch(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_short: BTreeSet<String> = manifest
        .plugins
        .keys()
        .filter_map(|key| {
            key.strip_prefix("@lazuli/plugin-")
                .map(|name| name.to_owned())
        })
        .collect();

    for key in manifest.plugins.keys() {
        if !key.starts_with("@lazuli/plugin-") {
            diagnostics.push(DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                message: format!(
                    "manifest plugin key `{key}` does not use the canonical plugin namespace; plugins must be declared as `@lazuli/plugin-<name>`."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            // A canonical plugin reference is `@lazuli/plugin-<name>` —
            // the @-reference parser yields namespace=`lazuli`,
            // name=`plugin-<name>` for that shape. Skip it before the
            // mismatch detector runs.
            if reference.namespace == "lazuli" && reference.name.starts_with("plugin-") {
                continue;
            }
            if reference.namespace == "adapter" && declared_short.contains(&reference.name) {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses the local adapter namespace, but Lazurite.toml declares `@lazuli/plugin-{}`; use the plugin reference.",
                        reference.reference, reference.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            } else if !is_allowed_reference_namespace_for_doctor(&reference.namespace)
                && declared_short.contains(&reference.name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses unknown namespace `@{}`, but Lazurite.toml declares `@lazuli/plugin-{}`; use the plugin reference.",
                        reference.reference, reference.namespace, reference.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn check_submodule_drift(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if !manifest
        .generate
        .go
        .as_ref()
        .map(|go| go.submodule)
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let root_go_mod = package.project_root.join("go.mod");
    let dist_go_mod = package.project_root.join("dist/go/go.mod");
    if !dist_go_mod.is_file() {
        return Vec::new();
    }

    let Ok(root_source) = fs::read_to_string(&root_go_mod) else {
        return Vec::new();
    };
    let Ok(dist_source) = fs::read_to_string(&dist_go_mod) else {
        return Vec::new();
    };
    let Some(root_version) = go_mod_lazuli_runtime_version(&root_source) else {
        return Vec::new();
    };
    let Some(dist_version) = go_mod_lazuli_runtime_version(&dist_source) else {
        return Vec::new();
    };

    if root_version == dist_version {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: dist_go_mod,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "SUBMODULE-DRIFT-001".to_owned(),
        message: format!(
            "`dist/go/go.mod` requires lazuli.dev/runtime {dist_version}, but root go.mod requires {root_version}."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(crate) fn check_migration_strategy_conflict(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if !matches!(
        manifest
            .migrations
            .as_ref()
            .map(|migrations| &migrations.strategy),
        Some(MigrationStrategy::Manual)
    ) {
        return Vec::new();
    }

    let Some(app) = package.app.as_ref() else {
        return Vec::new();
    };
    if app
        .manifest
        .deploy
        .as_ref()
        .and_then(|deploy| deploy.migrations.as_deref())
        != Some("before_deploy")
    {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "MIGRATION-STRATEGY-CONFLICT-001".to_owned(),
        message: "`[migrations].strategy = \"manual\"` conflicts with `app.lzi deploy migrations before_deploy`."
            .to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(crate) fn check_frontend_audience_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let known = collect_known_audiences(&package.files);
    let mut diagnostics = Vec::new();
    for (frontend_name, frontend) in &manifest.frontends {
        for audience in &frontend.audiences {
            if !known.contains(audience) {
                diagnostics.push(DoctorDiagnostic {
                    path: package.project_root.join("Lazurite.toml"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "FRONTEND-AUDIENCE-UNKNOWN-001".to_owned(),
                    message: format!(
                        "`[frontends.{frontend_name}]` ships audience `{audience}`, but no `.lzx` surface declares it."
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
    diagnostics
}

pub(crate) fn check_audience_no_frontend(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let shipped: BTreeSet<&str> = manifest
        .frontends
        .values()
        .flat_map(|frontend| frontend.audiences.iter().map(|audience| audience.as_str()))
        .collect();

    collect_known_audiences(&package.files)
        .into_iter()
        .filter(|audience| !shipped.contains(audience.as_str()))
        .map(|audience| DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "AUDIENCE-NO-FRONTEND-001".to_owned(),
            message: format!(
                "`.lzx` declares audience `{audience}`, but no `[frontends.*]` block ships it."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(crate) fn check_frontend_out_collision(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut first_by_out: BTreeMap<&str, &str> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (name, frontend) in &manifest.frontends {
        if let Some(first) = first_by_out.insert(frontend.out.as_str(), name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "FRONTEND-OUT-COLLISION-001".to_owned(),
                message: format!(
                    "`[frontends.{name}]` and `[frontends.{first}]` both declare output path `{}`.",
                    frontend.out
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    diagnostics
}
