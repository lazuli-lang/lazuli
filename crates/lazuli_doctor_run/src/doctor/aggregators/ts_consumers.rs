//! TypeScript-consumer-side diagnostics (Wave §2/§3 sweeps).
//!
//! Two producers scan `app/clients/<frontend>/src/**/*.{ts,tsx}` for
//! patterns that should migrate to the generated SDK / parser surface:
//!
//! - `MANUAL-PARAM-COERCION` — hand-rolled `Number(params.id)` /
//!   `String(params.id)` / `as unknown as number` casts that should
//!   migrate to the generated `parse<Route>Params(rawParams)` factory
//!   (Wave §2, 2026-05-24 false-positive tightening).
//! - `IMPORT-DEPRECATED-ALIAS` — consumer imports of SDK exports
//!   marked `@deprecated` in `dist/ts-<surface>/<audience>/*.gen.ts`
//!   (Wave §3, 2026-05-23).
//!
//! Both producers gate on the project layout (`app/clients/<frontend>`
//! and `dist/` must exist); standalone fixture invocations short-circuit
//! with an empty diagnostic vec.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::doctor::scanners::{collect_deprecated_exports, matches_word, walk_frontend_ts_files};
use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

/// MANUAL-PARAM-COERCION — warn when a TS consumer hand-coerces a
/// route param to `Number`/`String` instead of using the generated
/// typed parser. The check only fires on variables literally named
/// `params` / `rawParams` (the canonical useParams() return-value
/// names); iteration vars (`p`, `item`, `entry`) that happen to
/// access `.id` are NOT route params and used to produce false
/// positives.
pub(crate) fn manual_param_coercion_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let clients_root = project_root.join("app").join("clients");
    if !clients_root.exists() {
        return diagnostics;
    }

    const ID_PARAMS: &[&str] = &[
        "params.id",
        "params.propertyId",
        "params.serviceId",
        "params.threadId",
        "params.chatId",
        "params.userId",
        "params.hostId",
        "params.travelerId",
        "rawParams.id",
        "rawParams.propertyId",
        "rawParams.serviceId",
        "rawParams.threadId",
        "rawParams.chatId",
        "rawParams.userId",
        "rawParams.hostId",
        "rawParams.travelerId",
    ];

    walk_frontend_ts_files(&clients_root, &mut |path, contents| {
        for (lineno, line) in contents.lines().enumerate() {
            // Cheap pre-filter: skip lines that can't contain any pattern.
            let has_number = line.contains("Number(");
            let has_cast = line.contains("as unknown as number");
            let has_string = line.contains("String(");
            if !has_number && !has_cast && !has_string {
                continue;
            }
            // Skip the lint's own comment lines and existing
            // codegen workaround banners — they MENTION the pattern
            // but aren't violations.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
                continue;
            }
            let matches_param = ID_PARAMS.iter().any(|p| line.contains(p));
            let kind = if matches_param && has_cast {
                Some("as unknown as number on params.X")
            } else if has_number && matches_param {
                Some("Number(params.X)")
            } else if has_string && matches_param {
                Some("String(params.X)")
            } else {
                None
            };
            let Some(kind) = kind else { continue };
            diagnostics.push(DoctorDiagnostic {
                path: path.to_path_buf(),
                line: lineno + 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "MANUAL-PARAM-COERCION".to_owned(),
                message: format!(
                    "manual route-param coercion ({kind}) — wave §2 typed param parsers should land here instead. Use the generated `parse<Route>Params(rawParams)` factory from `dist/ts-<surface>/<audience>/routes.gen.tsx`."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    });

    diagnostics
}

/// IMPORT-DEPRECATED-ALIAS (warning) — flags consumer imports of
/// SDK exports marked `@deprecated` in the generated TS code.
/// Codegen emits backward-compat aliases for every rename
/// (`listMinePropertiesCatalogs` → `listMineProperties`,
/// `listMinePropertiesUploadedAssets` → `listMineProperties`); the
/// alias lives for one cycle to give consumers time to migrate,
/// then gets removed. This lint catches the consumer half so the
/// removal lands without dangling references.
///
/// Wave §3 (2026-05-23). Severity is Warning — informational; the
/// alias still resolves at runtime. Escalation to Error happens
/// when each removal is planned (consumer fixes its import +
/// runtime drops the alias in the same release).
pub(crate) fn import_deprecated_alias_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let dist_root = project_root.join("dist");
    let clients_root = project_root.join("app").join("clients");
    if !dist_root.exists() || !clients_root.exists() {
        return diagnostics;
    }

    let mut deprecated_exports: BTreeMap<String, PathBuf> = BTreeMap::new();
    collect_deprecated_exports(&dist_root, &mut deprecated_exports);
    if deprecated_exports.is_empty() {
        return diagnostics;
    }

    walk_frontend_ts_files(&clients_root, &mut |path, contents| {
        // Cheap pre-filter: only inspect lines inside an import statement.
        let mut in_import = false;
        for (lineno, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
                in_import = true;
            }
            if !in_import {
                continue;
            }
            for name in deprecated_exports.keys() {
                // Word-boundary match — guard against substring false positives
                // (e.g. `listMinePropertiesV2` would otherwise fire on the
                // shorter `listMineProperties`).
                if !matches_word(line, name) {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: path.to_path_buf(),
                    line: lineno + 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "IMPORT-DEPRECATED-ALIAS".to_owned(),
                    message: format!(
                        "import of deprecated SDK alias `{name}`. The generated `.gen.ts` declares it `@deprecated`; switch to the canonical export before the alias is removed in the next codegen cycle."
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            if line.contains("from ") {
                in_import = false;
            }
        }
    });

    diagnostics
}
