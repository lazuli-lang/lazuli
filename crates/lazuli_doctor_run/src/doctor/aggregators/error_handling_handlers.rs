//! Go-handler error-handling aggregator — `HANDLER-*` rules over
//! user-app `features/<feature>/{handlers,domain,jobs,integrations}/`
//! Go sources.
//!
//! Mirrors the shape of [`super::correctness`]: walks once, runs every
//! `HANDLER-*` rule from `lazuli_doctor::error_handling`, maps each
//! native `Finding` into the canonical [`DoctorDiagnostic`] envelope,
//! and resolves severity through the manifest's
//! `[doctor.error_handling].preset`.
//!
//! ## Rules dispatched
//!
//! Error-handling family (severity default `Warning`; iron-hand → `Error`):
//! - [`HANDLER-NO-PANIC-001`] — literal `panic(...)` outside test files.
//! - [`HANDLER-NO-STRING-ERROR-001`] — `errors.New("...")` inside a
//!   function body, or pure-string `fmt.Errorf("literal")`.
//! - [`HANDLER-ERROR-WRAP-001`] — `fmt.Errorf(..., err)` with `%v` /
//!   `%s` / `%+v` instead of `%w`.
//!
//! Hand-written SQL drift (severity per [`SecurityProfile`]):
//! - [`HANDLER-SQL-COLUMN-DRIFT-001`] — INSERT statements omitting
//!   NOT NULL columns declared by the codegen resource struct.
//!   Prototype → `Warning`, Strict / Production → `Error`.
//!
//! Test-discipline rules that walk Go `_test.go` sources (share this
//! aggregator's walker so we don't double-walk the workspace):
//! - [`TEST-PINS-STUB-VOCAB-001`] — assertions on stub-state vocabulary
//!   inside `_test.go`. Prototype → `Info`, Strict → `Warning`,
//!   Production → `Error`; iron-hand preset escalates to `Error`.
//! - [`TEST-FAILURE-ONLY-COVERAGE-001`] — `_test.go` file with no
//!   success-path coverage. Default `Warning`; iron-hand → `Error`.
//!
//! ## Preset wiring
//!
//! Error-handling severity resolves through
//! [`super::super::helpers::resolve_error_handling_severity`]; the two
//! test-discipline rules resolve through
//! [`super::super::helpers::resolve_test_discipline_severity`] so
//! iron-hand semantics stay uniform across the framework and user-app
//! dispatch paths.
//!
//! [`HANDLER-NO-PANIC-001`]: lazuli_doctor::error_handling::handler_no_panic_001
//! [`HANDLER-NO-STRING-ERROR-001`]: lazuli_doctor::error_handling::handler_no_string_error_001
//! [`HANDLER-ERROR-WRAP-001`]: lazuli_doctor::error_handling::handler_error_wrap_001
//! [`HANDLER-SQL-COLUMN-DRIFT-001`]: lazuli_doctor::correctness::handler_sql_column_drift_001
//! [`TEST-PINS-STUB-VOCAB-001`]: lazuli_doctor::test_discipline::test_pins_stub_vocab_001
//! [`TEST-FAILURE-ONLY-COVERAGE-001`]: lazuli_doctor::test_discipline::test_failure_only_coverage_001

use std::path::Path;

use lazuli_doctor::correctness::handler_sql_column_drift_001;
use lazuli_doctor::error_handling::preset::ErrorHandlingPreset;
use lazuli_doctor::error_handling::walker::walk_workspace_go_handlers;
use lazuli_doctor::error_handling::{
    handler_error_wrap_001, handler_no_panic_001, handler_no_string_error_001,
};
use lazuli_doctor::test_discipline::preset::TestDisciplinePreset;
use lazuli_doctor::test_discipline::{test_failure_only_coverage_001, test_pins_stub_vocab_001};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use lazuli_manifest::lazurite_manifest::Manifest;

use crate::doctor::helpers::{resolve_error_handling_severity, resolve_test_discipline_severity};
use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

/// Walk the user-app workspace at `project_root` and run every
/// `HANDLER-*` rule. Returns the canonical
/// `Vec<DoctorDiagnostic>` the rest of the dispatch pipeline expects.
///
/// `manifest` is consulted for `[doctor.error_handling].preset` to
/// resolve severity. Pass `None` to fall back to per-rule defaults
/// (every rule fires at `Warning`).
///
/// ## Examples
///
/// ```no_run
/// // Direct invocation against a user-app workspace root.
/// // Wired into the main doctor flow in W7.
/// # use std::path::Path;
/// # // The aggregator is `pub(crate)` so this docstring is illustrative.
/// ```
pub(crate) fn diagnostics(
    project_root: &Path,
    manifest: Option<&Manifest>,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    diagnostics_with_preset(
        project_root,
        resolve_preset(manifest),
        resolve_test_discipline_preset(manifest),
        security_profile,
    )
}

/// Inner helper — same plumbing as [`diagnostics`] but takes the
/// already-resolved presets directly. Public to the parent module so
/// unit tests can validate severity escalation without faking a full
/// [`Manifest`].
pub(crate) fn diagnostics_with_preset(
    project_root: &Path,
    preset: Option<ErrorHandlingPreset>,
    test_discipline_preset: Option<TestDisciplinePreset>,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let files = walk_workspace_go_handlers(project_root);
    if files.is_empty() {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();

    for finding in handler_no_panic_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            handler_no_panic_001::Finding::CODE,
            preset,
        );
        diagnostics.push(DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: handler_no_panic_001::Finding::CODE.to_owned(),
            category: None,
            feature_name: Some(finding.feature),
            construct: None,
            fix: None,
            group: None,
        });
    }

    for finding in handler_no_string_error_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            handler_no_string_error_001::Finding::CODE,
            preset,
        );
        diagnostics.push(DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: handler_no_string_error_001::Finding::CODE.to_owned(),
            category: None,
            feature_name: Some(finding.feature),
            construct: None,
            fix: None,
            group: None,
        });
    }

    for finding in handler_error_wrap_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            handler_error_wrap_001::Finding::CODE,
            preset,
        );
        diagnostics.push(DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: handler_error_wrap_001::Finding::CODE.to_owned(),
            category: None,
            feature_name: Some(finding.feature),
            construct: None,
            fix: None,
            group: None,
        });
    }

    // HANDLER-SQL-COLUMN-DRIFT-001 — INSERT statement omits NOT NULL
    // columns declared by the codegen resource struct. Severity:
    // prototype = warning, strict / production = error (per the
    // proposal's history-blind drift policy).
    let sql_drift_default = match security_profile {
        SecurityProfile::Prototype => DoctorSeverity::Warning,
        SecurityProfile::Strict | SecurityProfile::Production => DoctorSeverity::Error,
    };
    for finding in handler_sql_column_drift_001::check(&files, project_root) {
        diagnostics.push(DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: finding.line,
            column: 1,
            severity: sql_drift_default,
            code: handler_sql_column_drift_001::Finding::CODE.to_owned(),
            category: None,
            feature_name: Some(finding.feature),
            construct: None,
            fix: None,
            group: None,
        });
    }

    // TEST-PINS-STUB-VOCAB-001 — `_test.go` assertion calls that pin
    // stub-state vocabulary (`assert.Contains(..., "not implemented")`,
    // etc). Severity policy: prototype = info, strict = warning,
    // production = error; iron-hand preset escalates to error.
    let stub_vocab_default = match security_profile {
        SecurityProfile::Prototype => DoctorSeverity::Info,
        SecurityProfile::Strict => DoctorSeverity::Warning,
        SecurityProfile::Production => DoctorSeverity::Error,
    };
    for file in &files {
        if !file.is_test {
            continue;
        }
        for finding in test_pins_stub_vocab_001::check(&file.source, &file.absolute_path) {
            let severity = resolve_test_discipline_severity(
                stub_vocab_default,
                test_pins_stub_vocab_001::Finding::CODE,
                test_discipline_preset,
            );
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: finding.line,
                column: finding.column,
                severity,
                code: test_pins_stub_vocab_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: Some(file.feature_name.clone()),
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // TEST-FAILURE-ONLY-COVERAGE-001 — `_test.go` file that exercises
    // only error-side assertions. Default severity is `Warning`; under
    // `tdd-iron-hand` the resolver escalates to `Error`.
    for finding in test_failure_only_coverage_001::check(&files) {
        let severity = resolve_test_discipline_severity(
            DoctorSeverity::Warning,
            test_failure_only_coverage_001::Finding::CODE,
            test_discipline_preset,
        );
        diagnostics.push(DoctorDiagnostic {
            message: finding.message(),
            path: finding.absolute_path,
            line: 1,
            column: 1,
            severity,
            code: test_failure_only_coverage_001::Finding::CODE.to_owned(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    diagnostics
}

/// Pull the `[doctor.error_handling].preset` value out of the manifest
/// and parse it. Returns `None` when:
///
/// - The manifest is absent.
/// - `[doctor]` is absent.
/// - `[doctor.error_handling]` is absent.
/// - `preset = "..."` is unset or unparseable.
///
/// Callers fall back to per-rule defaults in that case.
fn resolve_preset(manifest: Option<&Manifest>) -> Option<ErrorHandlingPreset> {
    manifest
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.error_handling.as_ref())
        .and_then(|eh| eh.preset.as_deref())
        .and_then(ErrorHandlingPreset::parse)
}

/// Pull `[doctor.test_discipline].preset` for the
/// `TEST-FAILURE-ONLY-COVERAGE-001` severity-resolver. Mirrors
/// [`resolve_preset`].
fn resolve_test_discipline_preset(manifest: Option<&Manifest>) -> Option<TestDisciplinePreset> {
    manifest
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.test_discipline.as_ref())
        .and_then(|td| td.preset.as_deref())
        .and_then(TestDisciplinePreset::parse)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn empty_workspace_yields_no_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let diags = diagnostics(tmp.path(), None, SecurityProfile::Strict);
        assert!(diags.is_empty());
    }

    #[test]
    fn panic_in_handler_fires_once() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/auth/handlers/login.go",
            "package handlers\n\nfunc Login() { panic(\"nope\") }\n",
        );
        let diags = diagnostics(tmp.path(), None, SecurityProfile::Strict);
        let panic_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "HANDLER-NO-PANIC-001")
            .collect();
        assert_eq!(panic_diags.len(), 1);
        assert_eq!(panic_diags[0].severity, DoctorSeverity::Warning);
        assert_eq!(panic_diags[0].feature_name.as_deref(), Some("auth"));
    }

    #[test]
    fn iron_hand_preset_escalates_severity() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/auth/handlers/login.go",
            "package handlers\n\nfunc Login() { panic(\"nope\") }\n",
        );
        let diags = diagnostics_with_preset(
            tmp.path(),
            Some(ErrorHandlingPreset::TddIronHand),
            None,
            SecurityProfile::Strict,
        );
        let panic_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "HANDLER-NO-PANIC-001")
            .collect();
        assert_eq!(panic_diags.len(), 1);
        assert_eq!(panic_diags[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn all_three_rules_dispatch_against_one_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/billing/jobs/charge.go",
            "package jobs\n\nfunc Run(err error) {\n  panic(\"x\")\n  return fmt.Errorf(\"y: %v\", err)\n}\n",
        );
        write(
            tmp.path(),
            "features/billing/domain/foo.go",
            "package domain\n\nfunc Foo() error { return errors.New(\"oops\") }\n",
        );
        let diags = diagnostics(tmp.path(), None, SecurityProfile::Strict);
        let codes: std::collections::BTreeSet<&str> =
            diags.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains("HANDLER-NO-PANIC-001"));
        assert!(codes.contains("HANDLER-NO-STRING-ERROR-001"));
        assert!(codes.contains("HANDLER-ERROR-WRAP-001"));
    }

    #[test]
    fn test_files_are_skipped_across_handler_error_rules() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/auth/handlers/login_test.go",
            "package handlers\n\nfunc TestX(t *T) {\n  panic(\"ok\")\n  return errors.New(\"ok\")\n  return fmt.Errorf(\"bad: %v\", err)\n}\n",
        );
        let diags = diagnostics(tmp.path(), None, SecurityProfile::Strict);
        // The HANDLER-* error-handling rules ignore `_test.go` files.
        // `TEST-FAILURE-ONLY-COVERAGE-001` does walk them, but the file
        // above has no `func Test*` body (the parens-fenced helper is
        // named `TestX` — accepted; but the body is unparseable for
        // the success/error categoriser, so a finding may fire).
        // Assert only that the HANDLER-* codes are silent.
        let handler_codes: Vec<_> = diags
            .iter()
            .filter(|d| d.code.starts_with("HANDLER-"))
            .collect();
        assert!(
            handler_codes.is_empty(),
            "_test.go files must be silent across all HANDLER-* rules"
        );
    }
}
