//! Plugin reference resolution + `@semantic.*` plugin alias checks.
//!
//! - `SEMANTIC-PLUGIN-001` — `@semantic.<X>` in `.lzi` resolves to
//!   neither the built-in closed catalog nor any active plugin
//!   alias map entry. Three failure modes (undeclared plugin,
//!   broken manifest, alias conflict) share the code.
//! - `SEMANTIC-PLUGIN-002` (B4) — `@semantic.<X>` resolves to a
//!   plugin alias whose entry declares no `validator`. The type
//!   is accepted but no runtime check enforces the value.
//! - `PLUGIN-NOT-DECLARED-001` — `.lzi` references an
//!   `@<namespace>.<name>` that no entry in `Lazurite.toml
//!   [plugins]` declares.
//! - `PLUGIN-UNUSED-001` — `[plugins]` declares an alias no `.lzi`
//!   references. Warning, not error — packages may declare in
//!   anticipation.
//! - `PLUGIN-NAMESPACE-MISMATCH-001` — plugin reference uses the
//!   wrong namespace (`@adapter.x` or `@<unknown>.x` where
//!   `Lazurite.toml` declared `@lazuli/plugin-x`).

use std::collections::BTreeSet;

use lazuli_manifest::lazurite_manifest::Manifest;

use super::plugin_resolution_view::authoritative_alias_map;
use crate::doctor::parsers::is_lzi_path;
use crate::doctor::{
    DoctorDiagnostic, DoctorPackage, DoctorSeverity, collect_at_references_in_source,
    collect_package_plugin_references, is_allowed_reference_namespace_for_doctor,
};

/// SEMANTIC-PLUGIN-002 (B4) — `@semantic.<Name>` references that
/// resolve to a plugin scalar with NO `validator` declared. The type
/// alias exists but no runtime check enforces it; the field accepts
/// any string at the wire boundary. Warn-level: some plugins ship
/// brand aliases intentionally without validation.
///
/// Source-of-truth: `docs/proposals/ir-semantic-auto-validate-2026-05-22.md`
/// (W2 §"Doctor B4").
pub(super) fn check_semantic_plugin_no_validator(
    _manifest: &lazuli_manifest::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    // 0020 — resolve through the AUTHORITATIVE alias map (upward-walked
    // root, same inputs codegen uses), not `package.project_root`.
    let alias_map = match authoritative_alias_map(package) {
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
pub(super) fn check_semantic_plugin_unresolved(
    _manifest: &lazuli_manifest::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    // 0020 — Build the alias map from the AUTHORITATIVE upward-walked
    // root (the same `find_project_root` + `build_alias_map` codegen
    // feeds), so `@semantic.<X>` fires SEMANTIC-PLUGIN-001 iff it would
    // leave a residual `UserDefined("@semantic.*")` on the generate
    // path. Map-construction errors (conflict, mismatch, unsupported
    // carrier) surface as SEMANTIC-PLUGIN-001 anchored at the project
    // root because they're project-wide.
    let alias_map = match authoritative_alias_map(package) {
        Ok(map) => map,
        Err(err) => {
            return vec![DoctorDiagnostic {
                path: super::plugin_resolution_view::authoritative_project_root(package)
                    .join("Lazurite.toml"),
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
        "Email",
        "Phone",
        "Url",
        "Uuid",
        "Currency",
        "GeoPoint",
        "Money",
        "HexColor",
        "Percentage",
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

pub(super) fn check_plugin_not_declared(
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

pub(super) fn check_plugin_unused(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut used: BTreeSet<String> = collect_package_plugin_references(package)
        .into_iter()
        .map(|reference| reference.reference)
        .collect();

    // 2026-05-27 — `@semantic.<X>` references that resolve via the
    // plugin alias_map count as usage of the providing plugin.
    // Without this, semantic-scalar-only plugins (scalars-br, viacep)
    // were always flagged PLUGIN-UNUSED even when every pilot form
    // used `@semantic.BrazilianCEP`, `@semantic.Money`, etc — the
    // plugin alias_map is the canonical source of truth for which
    // plugin provides which semantic alias.
    // 0020 — usage detection resolves through the AUTHORITATIVE alias
    // map so a `@semantic.<X>` from a features subdir still marks its
    // providing plugin as used (matching codegen's resolution).
    if let Ok(alias_map) = authoritative_alias_map(package) {
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
                if let Some(resolved) = alias_map.get(&alias) {
                    used.insert(resolved.plugin_namespace.clone());
                }
            }
        }
    }

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

pub(super) fn check_plugin_namespace_mismatch(
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
