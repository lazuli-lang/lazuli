//! Plugin manifest / README / catalog hygiene checks.
//!
//! - `PLUGIN-MANIFEST-MISSING` (error) — declared plugin missing its
//!   `manifest.toml`. Without it codegen falls back to silent
//!   defaults and the catalog loses self-description.
//! - `PLUGIN-MANIFEST-SCHEMA-LEGACY` (error, post-W§A4 hard cutover)
//!   — `manifest.toml` lacks the canonical `[plugin]` block (pre-v1
//!   flat schema).
//! - `PLUGIN-README-MISSING` (warning) — every plugin should ship a
//!   `README.md` so the catalog page derives from it.
//! - `PLUGIN-CATALOG-DRIFT` (warning) —
//!   `dist/plugin-catalog.json`'s mtime predates a plugin's
//!   `manifest.toml` or `README.md`.

use crate::doctor::{DoctorDiagnostic, DoctorPackage, DoctorSeverity};

pub(super) fn check_plugin_manifest_missing(
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
pub(super) fn check_plugin_manifest_schema_legacy(
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
pub(super) fn check_plugin_readme_missing(
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
pub(super) fn check_plugin_catalog_drift(
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
