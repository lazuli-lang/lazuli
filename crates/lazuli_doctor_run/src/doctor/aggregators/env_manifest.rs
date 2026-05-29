//! Env declaration discipline + manifest / cap-file policy diagnostics.
//!
//! Four producers live here, all gated on project-level facts loaded
//! once by the package walker:
//!
//! - `dedupe_env_contract_diagnostics` — drops the broader
//!   `app-env-contract` diagnostic when `env-schema-contract` already
//!   flagged the same `(path, line)`.
//! - `suppress_env_schema_when_declared` — suppresses LSP-emitted
//!   `env-schema-reference` warnings for envs that ARE declared in the
//!   loaded registry / app manifest.
//! - `manifest_required_diagnostics` (MANIFEST-REQUIRED-001) — error
//!   when a project uses `@lazuli/plugin-*` references but ships no
//!   `Lazurite.toml`.
//! - `cap_file_policy_implicit_diagnostics` (CAP-FILE-POLICY-IMPLICIT)
//!   — warn when a `@cap.File(...)` site does not declare an explicit
//!   `auto_photo_policy: @policy.<name>`.
//!
//! The local helper `project_uses_plugin_refs` walks every `.lzi` under
//! the project root and tests for any `@lazuli/plugin-*` reference; it's
//! private to this aggregator since no other producer needs it today.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::doctor::helpers::project_has_lazurite_manifest;
use crate::doctor::parsers::is_lzi_path;
use crate::doctor::refs::collect_plugin_references_in_source;
use crate::doctor::scanners::collect_lazuli_paths_recursive;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// LSP emits both `app-env-contract` and `env-schema-contract` on the
/// same line of a `registry.env` block when the env declaration shape is
/// invalid — the `app` and `registry` indent-6 branches both call
/// `validate_app_env_line`, then the dedicated `env-schema-contract`
/// validator runs over the registry pass. Audit ref: R1.C real-world
/// sweep produced 9 duplicates (gamma 7×, delta 2×).
///
/// `env-schema-contract` is the more specific registry-scoped rule and
/// owns the registry env shape; drop the broader `app-env-contract`
/// diagnostic when the same `(path, line)` already carries it.
pub(crate) fn dedupe_env_contract_diagnostics(
    diagnostics: &[DoctorDiagnostic],
) -> Vec<DoctorDiagnostic> {
    let env_schema_lines: BTreeSet<(PathBuf, usize)> = diagnostics
        .iter()
        .filter(|d| d.code == "env-schema-contract")
        .map(|d| (d.path.clone(), d.line))
        .collect();

    diagnostics
        .iter()
        .filter(|d| {
            !(d.code == "app-env-contract" && env_schema_lines.contains(&(d.path.clone(), d.line)))
        })
        .cloned()
        .collect()
}

/// LSP emits `env-schema-reference` per file because the per-file rule can't
/// see the registry. Doctor has cross-package visibility (it loads
/// `registry.lzi` and `app.lzi`), so it can suppress those warnings for envs
/// that ARE declared. Closes the false-positive surfaced by the the canonical pilot
/// pilot port (2026-05-16): `env.MERCADOPAGO_WEBHOOK_SECRET` was correctly
/// declared in `registry.env` but the LSP warning was inherited verbatim.
///
/// Message shape: ``"environment reference `env.<NAME>` should be declared..."``
pub(crate) fn suppress_env_schema_when_declared(
    diagnostics: &[DoctorDiagnostic],
    declared_env_names: &BTreeSet<&str>,
) -> Vec<DoctorDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            if d.code != "env-schema-reference" {
                return true;
            }
            // Extract env name from `env.X` in the message.
            let Some(start) = d.message.find("env.") else {
                return true;
            };
            let rest = &d.message[start + "env.".len()..];
            let end = rest.find('`').unwrap_or(rest.len());
            let env_name = &rest[..end];
            !declared_env_names.contains(env_name)
        })
        .cloned()
        .collect()
}

/// MANIFEST-REQUIRED-001 — error when the project uses
/// `@lazuli/plugin-*` references but ships no `Lazurite.toml`.
/// Single-file invocations are excluded from the check (no project
/// context — historically produced 12 false positives across standalone
/// fixtures).
pub(crate) fn manifest_required_diagnostics(
    project_root: &Path,
    single_file_input: bool,
) -> Vec<DoctorDiagnostic> {
    if single_file_input {
        return Vec::new();
    }

    if !project_uses_plugin_refs(project_root) || project_has_lazurite_manifest(project_root) {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: project_root.join("Lazurite.toml"),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "MANIFEST-REQUIRED-001".to_owned(),
        message: "project uses @lazuli/plugin-* references but is missing Lazurite.toml."
            .to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// CAP-FILE-POLICY-IMPLICIT (warning) — every `@cap.File` field on
/// a per-user resource should declare an explicit
/// `auto_photo_policy: @policy.<name>`. The analyzer's heuristic
/// fallback (resource-singular + `_only`) produces silent surprises
/// when a feature has multiple matching policies — e.g. both
/// `host_only` and `host_and_operator` — and the wrong one wins.
///
/// Wave §6 (2026-05-23). Severity is Warning today so existing
/// pilots can migrate field-by-field; escalating to Error is
/// gated on every pilot's `@cap.File` sites having explicit
/// policy declarations.
pub(crate) fn cap_file_policy_implicit_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in facts {
        for resource in &feature.resources {
            for field in &resource.fields {
                let cap = match &field.type_ref {
                    lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(spec)) => spec,
                    _ => continue,
                };
                if cap.auto_photo_policy.is_some() {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CAP-FILE-POLICY-IMPLICIT".to_owned(),
                    message: format!(
                        "resource `{}.{}` field `{}` is a `@cap.File(...)` site without an explicit `auto_photo_policy: @policy.<name>`. The analyzer falls back to the resource-singular heuristic (e.g. `host_only`), which silently picks the wrong policy when the feature has multiple matching candidates. Add `auto_photo_policy: @policy.<your_policy>` to the `@cap.File(...)` arglist.",
                        feature.feature, resource.name, field.name,
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

fn project_uses_plugin_refs(project_root: &Path) -> bool {
    let mut paths = Vec::new();
    if collect_lazuli_paths_recursive(project_root, &mut paths).is_err() {
        return false;
    }

    paths
        .into_iter()
        .filter(|path| is_lzi_path(path))
        .any(|path| {
            fs::read_to_string(&path)
                .map(|source| !collect_plugin_references_in_source(&path, &source).is_empty())
                .unwrap_or(false)
        })
}
