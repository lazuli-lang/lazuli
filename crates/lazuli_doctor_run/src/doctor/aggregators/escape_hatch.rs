//! `[doctor.escape_hatch]` aggregator (spec 0010) — runs the three
//! `ESC-*` escape-hatch visibility rules over a user app.
//!
//! Lazuli's promise is that a cold read of a feature's `.lzi` is the
//! source of truth for its effects, reads, policies, and tenancy. The
//! three rules close the accidental holes:
//!
//! - `ESC-RAWSQL-IN-HANDLER-001` — raw SQL hidden in a `@fn` Go handler
//!   for a read the `.lzi` declares only as an opaque `fn ...: Function`.
//! - `ESC-SQL-TENANCY-CONTRACT-001` — a `query.sql` mixing `:named` and
//!   `$N` binding, or referencing a param the `.lzi` doesn't declare.
//! - `ESC-SCOPE-OVERRIDE-UNGUARDED-001` — a `query.sql` with no tenant
//!   predicate AND no `@actor.<privileged>` guard.
//!
//! Mirrors [`super::lzi_hygiene`]: walk `<project_root>` for `.lzi`
//! feature files (via [`walk_lzi_sources`]), lower each feature once here
//! (the `lazuli_doctor` crate carries no runtime `lazuli_analyzer`
//! dependency, so lowering lives in this aggregator), then run each rule
//! over the lowered [`Feature`] + its on-disk feature directory. Each rule
//! resolves its own on-disk inputs relative to that directory:
//! `ESC-RAWSQL-IN-HANDLER-001` walks `<feature_dir>/handlers/*.go`; the
//! two `query.sql` rules read each block's `sql_path`. Findings become
//! [`DoctorDiagnostic`]s with severity resolved through
//! [`resolve_escape_hatch_severity`], which honors the
//! `[doctor.escape_hatch].preset`.

use std::path::Path;

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor::escape_hatch::preset::EscapeHatchPreset;
use lazuli_doctor::escape_hatch::{
    rawsql_in_handler_001, scope_override_unguarded_001, sql_tenancy_contract_001,
};
use lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources;
use lazuli_syntax::parse_feature_skeletons;

use crate::doctor::helpers::resolve_escape_hatch_severity;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

/// Run every `ESC-*` escape-hatch rule against `project_root` and return
/// the collected diagnostics. `preset` is the active
/// `[doctor.escape_hatch]` preset, resolved by the caller off the severity
/// `ResolvedDoctorConfig` (mirrors how `lzi_hygiene_diagnostics` receives
/// its preset pre-resolved).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// let diagnostics = escape_hatch_diagnostics(
///     Path::new("/path/to/user-app"),
///     None,
/// );
/// for d in &diagnostics {
///     println!("{}", d.code);
/// }
/// ```
pub(crate) fn escape_hatch_diagnostics(
    project_root: &Path,
    preset: Option<EscapeHatchPreset>,
) -> Vec<DoctorDiagnostic> {
    let files = walk_lzi_sources(project_root);
    if files.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    for file in &files {
        // The feature directory is the directory holding the `.lzi`; each
        // rule resolves its on-disk inputs (`handlers/*.go`, `sql_path`)
        // relative to it.
        let Some(feature_dir) = file.absolute_path.parent() else {
            continue;
        };

        // Lower the file's first feature once; files that don't parse /
        // lower are skipped (other rule families report those).
        let Ok(skeletons) = parse_feature_skeletons(&file.source) else {
            continue;
        };
        let Some(first) = skeletons.first() else {
            continue;
        };
        let Ok(feature) = lower_feature_skeleton(first) else {
            continue;
        };

        // ESC-RAWSQL-IN-HANDLER-001 — default Warning. Waivable-to-convert
        // (NOT waivable-to-silence): the rule body honors `# doctor:allow`
        // mechanically but still fires, marking the silence as debt.
        for finding in rawsql_in_handler_001::check(&feature, feature_dir) {
            let severity = resolve_escape_hatch_severity(
                DoctorSeverity::Warning,
                rawsql_in_handler_001::Finding::CODE,
                preset,
            );
            let message = finding.message();
            diagnostics.push(DoctorDiagnostic {
                path: finding.handler_path,
                line: 1,
                column: 1,
                severity,
                code: rawsql_in_handler_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ESC-SQL-TENANCY-CONTRACT-001 — default Warning. Mis-declared
        // binding contract is an ordinary waivable warning.
        for finding in sql_tenancy_contract_001::check(&feature, feature_dir) {
            let severity = resolve_escape_hatch_severity(
                DoctorSeverity::Warning,
                sql_tenancy_contract_001::Finding::CODE,
                preset,
            );
            let message = finding.message();
            diagnostics.push(DoctorDiagnostic {
                path: file.absolute_path.clone(),
                line: 1,
                column: 1,
                severity,
                code: sql_tenancy_contract_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ESC-SCOPE-OVERRIDE-UNGUARDED-001 — default Warning. Unguarded
        // scope override is an ordinary waivable warning.
        for finding in scope_override_unguarded_001::check(&feature, feature_dir) {
            let severity = resolve_escape_hatch_severity(
                DoctorSeverity::Warning,
                scope_override_unguarded_001::Finding::CODE,
                preset,
            );
            let message = finding.message();
            diagnostics.push(DoctorDiagnostic {
                path: file.absolute_path.clone(),
                line: 1,
                column: 1,
                severity,
                code: scope_override_unguarded_001::Finding::CODE.to_owned(),
                message,
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::File::create(&p)
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
    }

    #[test]
    fn aggregator_returns_empty_when_root_has_no_lzi() {
        let tmp = tempfile::tempdir().unwrap();
        let diagnostics = escape_hatch_diagnostics(tmp.path(), None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn aggregator_fires_rawsql_in_handler() {
        // Feature declares the read only as an opaque @fn; the sibling Go
        // handler runs a multi-line raw SQL read whose backtick literal is
        // the first call argument.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/trust/trust.lzi",
            "feature trust\n  resource Review\n    rating: Integer required\n",
        );
        write(
            tmp.path(),
            "features/trust/handlers/list_property_reviews.go",
            "package handlers\n\nfunc ListPropertyReviews(ctx lazuli.Ctx) ([]R, error) {\n    \
             rows, err := lazuli.DB().Query(`\n        SELECT r.id, r.rating\n        \
             FROM property_reviews r\n        WHERE r.org_id = $1\n    `, ctx.OrgID())\n    \
             return nil, err\n}\n",
        );
        let diagnostics = escape_hatch_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "ESC-RAWSQL-IN-HANDLER-001"),
            "expected ESC-RAWSQL-IN-HANDLER-001, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn aggregator_fires_sql_tenancy_contract() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/notifications/queries/unread_count.sql",
            "SELECT count(*) FROM n WHERE org_id = :org_id AND status = $2",
        );
        write(
            tmp.path(),
            "features/notifications/notifications.lzi",
            "feature notifications\n  resource Note\n    body: Text required\n  \
             query.sql unread_count\n    returns Note\n    sql \"./queries/unread_count.sql\"\n    \
             policy @policy.public\n",
        );
        let diagnostics = escape_hatch_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "ESC-SQL-TENANCY-CONTRACT-001"),
            "got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn aggregator_fires_scope_override_unguarded() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/admin_panel/list_everything.sql",
            "SELECT id, name FROM agencies ORDER BY created_at DESC",
        );
        write(
            tmp.path(),
            "features/admin_panel/admin_panel.lzi",
            "feature admin_panel\n  resource Agency\n    name: Text required\n  \
             query.sql list_everything\n    returns Agency\n    sql \"./list_everything.sql\"\n    \
             policy none\n",
        );
        let diagnostics = escape_hatch_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "ESC-SCOPE-OVERRIDE-UNGUARDED-001"),
            "got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
}
