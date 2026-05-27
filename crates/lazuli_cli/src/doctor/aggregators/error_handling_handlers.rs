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
//! - [`HANDLER-NO-PANIC-001`] — literal `panic(...)` outside test files.
//! - [`HANDLER-NO-STRING-ERROR-001`] — `errors.New("...")` inside a
//!   function body, or pure-string `fmt.Errorf("literal")`.
//! - [`HANDLER-ERROR-WRAP-001`] — `fmt.Errorf(..., err)` with `%v` /
//!   `%s` / `%+v` instead of `%w`.
//!
//! ## Preset wiring
//!
//! Severity defaults are `Warning` for all three rules. Under
//! `[doctor.error_handling].preset = "tdd-iron-hand"` each escalates
//! to `Error`. The resolution uses
//! [`super::super::helpers::resolve_error_handling_severity`] — same
//! helper the `--self` audit uses, so iron-hand semantics stay
//! uniform across the framework and user-app dispatch paths.
//!
//! ## Dispatch
//!
//! TODO(W7): the aggregator function is ready; wire from the main
//! `doctor_command_with_options` flow (alongside the existing
//! aggregators in [`super::super::dispatch`]) once the
//! `DoctorPackage` carries either a workspace-root handle or a
//! pre-walked `Vec<GoHandlerSourceFile>`. Until then this aggregator
//! is reachable only via direct invocation in unit tests, which is
//! sufficient to validate the rule-to-diagnostic plumbing.
//!
//! [`HANDLER-NO-PANIC-001`]: lazuli_doctor::error_handling::handler_no_panic_001
//! [`HANDLER-NO-STRING-ERROR-001`]: lazuli_doctor::error_handling::handler_no_string_error_001
//! [`HANDLER-ERROR-WRAP-001`]: lazuli_doctor::error_handling::handler_error_wrap_001

use std::path::Path;

use lazuli_doctor::error_handling::{
    handler_error_wrap_001, handler_no_panic_001, handler_no_string_error_001,
    preset::ErrorHandlingPreset, walker::walk_workspace_go_handlers,
};

use crate::doctor::helpers::resolve_error_handling_severity;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity};
use crate::lazurite_manifest::Manifest;

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
#[allow(dead_code)] // TODO(W7): wire from doctor_command_with_options
pub(crate) fn diagnostics(
    project_root: &Path,
    manifest: Option<&Manifest>,
) -> Vec<DoctorDiagnostic> {
    diagnostics_with_preset(project_root, resolve_preset(manifest))
}

/// Inner helper — same plumbing as [`diagnostics`] but takes the
/// already-resolved preset directly. Public to the parent module so
/// unit tests can validate severity escalation without faking a full
/// [`Manifest`].
#[allow(dead_code)] // TODO(W7): wire from doctor_command_with_options
pub(crate) fn diagnostics_with_preset(
    project_root: &Path,
    preset: Option<ErrorHandlingPreset>,
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
#[allow(dead_code)] // TODO(W7): wire from doctor_command_with_options
fn resolve_preset(manifest: Option<&Manifest>) -> Option<ErrorHandlingPreset> {
    manifest
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.error_handling.as_ref())
        .and_then(|eh| eh.preset.as_deref())
        .and_then(ErrorHandlingPreset::parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

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
        let diags = diagnostics(tmp.path(), None);
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
        let diags = diagnostics(tmp.path(), None);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "HANDLER-NO-PANIC-001");
        assert_eq!(diags[0].severity, DoctorSeverity::Warning);
        assert_eq!(diags[0].feature_name.as_deref(), Some("auth"));
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
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DoctorSeverity::Error);
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
        let diags = diagnostics(tmp.path(), None);
        let codes: std::collections::BTreeSet<&str> =
            diags.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains("HANDLER-NO-PANIC-001"));
        assert!(codes.contains("HANDLER-NO-STRING-ERROR-001"));
        assert!(codes.contains("HANDLER-ERROR-WRAP-001"));
    }

    #[test]
    fn test_files_are_skipped_across_all_rules() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/auth/handlers/login_test.go",
            "package handlers\n\nfunc TestX(t *T) {\n  panic(\"ok\")\n  return errors.New(\"ok\")\n  return fmt.Errorf(\"bad: %v\", err)\n}\n",
        );
        let diags = diagnostics(tmp.path(), None);
        assert!(diags.is_empty(), "_test.go files must be silent across all HANDLER-* rules");
    }
}
