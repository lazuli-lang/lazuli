//! Runtime / IR ABI version diagnostics + schema-richness lints.
//!
//! Two clusters of producers live here:
//!
//! - `lazuli_version_001` / `lazuli_version_002` — pin / migration-recipe
//!   discipline for the `lazuli_version` field in `app.lzi`. Closes the
//!   gap where a project pins an old ABI without a recipe in
//!   `migrations/recipes/<from>-to-<to>/` to migrate forward.
//! - `SCHEMA-RICH-GAP` — informational hint that a `JSON`-typed field
//!   with a name ending in `_photos`/`_files`/`_attachments`/`_images`/
//!   `_documents`/`_assets` should probably lift to `@cap.File[]` for a
//!   typed TS/Zod codegen surface.
//!
//! Both clusters operate on already-loaded `DoctorAppManifest` /
//! `Tier3FeatureFacts` refs; no filesystem walking happens here.

use std::path::Path;

use crate::doctor::parsers::{is_one_dot_zero_plus, major_minor};
use crate::doctor::scanners::lazuli_version_line;
use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// SCHEMA-RICH-GAP — informational hint that a `JSON`-typed field with
/// a file-bucket-suggesting name should probably lift to `@cap.File[]`.
/// The check is deliberately conservative (name suffix heuristic) so
/// false positives are easy to triage.
pub(crate) fn schema_rich_gap_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    const AVOIDABLE_SUFFIXES: &[&str] = &[
        "_photos",
        "_files",
        "_attachments",
        "_images",
        "_documents",
        "_assets",
    ];
    let mut diagnostics = Vec::new();
    for feature in facts {
        for resource in &feature.resources {
            for field in &resource.fields {
                let is_json = matches!(
                    field.type_ref,
                    lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Json)
                );
                if !is_json {
                    continue;
                }
                let suggests_files = AVOIDABLE_SUFFIXES
                    .iter()
                    .any(|suffix| field.name.ends_with(suffix));
                if !suggests_files {
                    continue;
                }
                // 2026-05-27 — honor `# doctor:allow SCHEMA-RICH-GAP`
                // in the feature .lzi (per the rule message's own
                // promise of "future @opaque annotation"). Until the
                // @opaque sigil lands, the doctor:allow comment is
                // the canonical opt-out.
                if lazuli_doctor::allow_comment::file_contains_doctor_allow(
                    &feature.path,
                    "SCHEMA-RICH-GAP",
                ) {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Hint,
                    code: "SCHEMA-RICH-GAP".to_owned(),
                    message: format!(
                        "resource `{}.{}` field `{}` is declared as opaque `JSON` but its name suggests a typed array of files/attachments. Consider lifting to `@cap.File[]` (or `@cap.AttachmentRef[]`) so the codegen emits a specific TS type + Zod schema instead of `unknown`/`z.any()`. If the field is genuinely opaque JSON, this hint can be ignored (the future `@opaque` annotation will silence it).",
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

/// LAZULI-VERSION-001 — every `app.lzi` must pin `lazuli_version`, and
/// the pin must match the current IR schema's major.minor.
pub(crate) fn lazuli_version_001_diagnostics(
    app: Option<&DoctorAppManifest>,
    schema: &str,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else { return Vec::new() };
    let current_major_minor = major_minor(schema);

    match app.manifest.lazuli_version.as_deref() {
        None => {
            let severity = if is_one_dot_zero_plus(schema) {
                DoctorSeverity::Error
            } else {
                DoctorSeverity::Warning
            };
            vec![DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity,
                code: "LAZULI-VERSION-001".to_owned(),
                message: format!(
                    "lazuli_version pin missing. Expected: lazuli_version \"{}\". Add this to app.lzi to lock the runtime/IR ABI version.",
                    current_major_minor
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }]
        }
        Some(pinned) => {
            let pinned_major_minor = major_minor(pinned);
            if pinned_major_minor == current_major_minor {
                Vec::new()
            } else {
                vec![DoctorDiagnostic {
                    path: app.path.clone(),
                    line: lazuli_version_line(&app.source).unwrap_or(1),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "LAZULI-VERSION-001".to_owned(),
                    message: format!(
                        "lazuli_version pin \"{}\" does not match current LZIR_SCHEMA \"{}\". Run: lazuli upgrade --from {} --to {} <project>. See migrations/recipes/{}-to-{}/.",
                        pinned,
                        schema,
                        pinned_major_minor,
                        current_major_minor,
                        pinned_major_minor,
                        current_major_minor
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }]
            }
        }
    }
}

/// LAZULI-VERSION-002 — a `lazuli_version` pin that differs from the
/// current IR schema must have a migration recipe directory under
/// `migrations/recipes/<from>-to-<to>/`. A missing recipe surfaces as
/// an Error.
pub(crate) fn lazuli_version_002_diagnostics(
    app: Option<&DoctorAppManifest>,
    schema: &str,
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else { return Vec::new() };
    let Some(pinned) = app.manifest.lazuli_version.as_deref() else {
        return Vec::new();
    };
    let pinned_major_minor = major_minor(pinned);
    let current_major_minor = major_minor(schema);
    if pinned_major_minor == current_major_minor {
        return Vec::new();
    }

    let recipe_dir = project_root
        .join("migrations/recipes")
        .join(format!("{}-to-{}", pinned_major_minor, current_major_minor));
    if recipe_dir.exists() {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "LAZULI-VERSION-002".to_owned(),
        message: format!(
            "lazuli_version pin \"{}\" has no migration recipe to current \"{}\". No recipe directory at {}. This may indicate a stale pin or a release that shipped without a recipe - file an issue.",
            pinned,
            schema,
            recipe_dir.display()
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}
