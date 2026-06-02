//! W0-4 (META-THEME) — the **blocking doctor gate** wired into the
//! default build loop.
//!
//! The audits' meta-finding was: "the gates exist but aren't wired into
//! the default loop at a blocking severity." `lazuli doctor` ran the full
//! Correctness + Error rule set, but `lazuli generate` emitted artifacts
//! without ever consulting it — so a module with an `ERROR`-severity
//! finding (an empty-bindings `creates`, a handler-signature mismatch, an
//! undeclared enum variant) could still ship a half-broken Go tree at
//! exit 0.
//!
//! This module is that gate. [`run_generate_gate`] runs the same package
//! doctor engine `lazuli doctor` uses ([`crate::doctor::run_package_cli`]),
//! partitions the stream into errors / warnings, renders them
//! **error-first** with a `N errors, M warnings` banner, and — on any
//! `ERROR`-severity finding — **fails closed** (returns `Err`, non-zero
//! exit) *before* the emitter writes a single file. The chosen posture is
//! **refuse-emit**: a blocked `generate` leaves the existing tree
//! untouched rather than promoting a known-broken one.
//!
//! ## Escape hatch
//!
//! The gate is a guardrail, not a cage. [`GateBypass`] carries the
//! `--no-gate` flag (and the `LAZULI_NO_GATE=1` env honored equivalently);
//! when set, the gate still *runs and prints* every finding, then emits a
//! loud `BYPASSED N error(s)` notice and proceeds. The per-finding
//! `# doctor:allow <CODE>` waivers are untouched — they suppress
//! individual findings *upstream*, inside the rule bodies, exactly as
//! before; `--no-gate` is the coarse all-or-nothing override on top.
//!
//! ## `--allow-version-mismatch` parity
//!
//! `LAZULI-VERSION-*` findings are the diagnostic mirror of the
//! `Lazurite.toml [lazuli]` pin enforcement the command already performs
//! up front. When `--allow-version-mismatch` is set, that pin check is
//! skipped — so the gate drops the `LAZULI-VERSION-*` findings too, to
//! stay consistent (the flag means "I know the pin is off, proceed").

use std::path::Path;

use anyhow::{Result, bail};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use lazuli_doctor_run::{DoctorDiagnostic, DoctorSeverity};

/// The coarse all-or-nothing override on top of the per-finding
/// `# doctor:allow` waivers.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GateBypass {
    /// `--no-gate` was passed on the CLI.
    pub no_gate: bool,
    /// `--allow-version-mismatch` was passed; drop `LAZULI-VERSION-*`
    /// findings (the diagnostic mirror of the skipped pin check).
    pub allow_version_mismatch: bool,
}

impl GateBypass {
    /// Resolve the effective `--no-gate` posture: the CLI flag OR the
    /// `LAZULI_NO_GATE=1` env (so CI / scripts can opt out without
    /// threading the flag through every call site).
    fn no_gate_effective(self) -> bool {
        if self.no_gate {
            return true;
        }
        matches!(std::env::var("LAZULI_NO_GATE").ok().as_deref(), Some("1"))
    }
}

/// `LAZULI-VERSION-*` findings — dropped from the gate set when
/// `--allow-version-mismatch` is in effect (parity with the skipped pin
/// enforcement). Anything else is gate-relevant.
fn is_version_finding(d: &DoctorDiagnostic) -> bool {
    d.code.starts_with("LAZULI-VERSION-")
}

/// Run the blocking doctor gate ahead of a `generate` emit.
///
/// Loads the full package diagnostic stream (the same one `lazuli doctor`
/// renders), prints it **error-first** with a count banner, and returns
/// `Err` on any `ERROR`-severity finding unless `--no-gate` /
/// `LAZULI_NO_GATE=1` is set (in which case it prints a loud bypass notice
/// and returns `Ok`). Warnings are always shown but never block.
///
/// Called by `commands::generate::generate_command` for the artifact-emit
/// kinds (`go`, `ts`, `openapi`) before any file is written —
/// **refuse-emit** on a blocked run.
pub(crate) fn run_generate_gate(
    input: &Path,
    security_profile: SecurityProfile,
    bypass: GateBypass,
) -> Result<()> {
    let package = super::run_package_cli(input, security_profile)?;
    let diagnostics = package.diagnostics();
    render_and_gate(&diagnostics, input, bypass)
}

/// Shared render + gate decision over an already-loaded diagnostic
/// stream. Split out so tests can exercise the gate logic without a full
/// package load.
pub(crate) fn render_and_gate(
    diagnostics: &[DoctorDiagnostic],
    input: &Path,
    bypass: GateBypass,
) -> Result<()> {
    // Partition. Version findings drop out of the gate set when the
    // version-mismatch flag is in effect (mirrors the skipped pin check).
    let gate_relevant = |d: &&DoctorDiagnostic| {
        !(bypass.allow_version_mismatch && is_version_finding(d))
    };

    let mut errors: Vec<&DoctorDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == DoctorSeverity::Error)
        .filter(gate_relevant)
        .collect();
    let warnings: Vec<&DoctorDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == DoctorSeverity::Warning)
        .filter(gate_relevant)
        .collect();

    // Stable error-first ordering: errors (by path/line), then warnings.
    errors.sort_by(|a, b| {
        (a.path.as_path(), a.line, a.column).cmp(&(b.path.as_path(), b.line, b.column))
    });

    // Error-first banner + summary.
    if !errors.is_empty() || !warnings.is_empty() {
        eprintln!(
            "doctor gate: {} error(s), {} warning(s)",
            errors.len(),
            warnings.len()
        );
    }
    // Gate findings are part of the failure stream → stderr, so a caller
    // piping the emitter's stdout still sees why a run refused to emit.
    for d in &errors {
        d.eprint();
    }
    for d in &warnings {
        d.eprint();
    }

    if errors.is_empty() {
        return Ok(());
    }

    if bypass.no_gate_effective() {
        eprintln!(
            "doctor gate: BYPASSED {} error(s) via --no-gate / LAZULI_NO_GATE=1 — \
             emitting a tree that the doctor flagged as broken. Fix the errors above \
             or add a per-finding `# doctor:allow <CODE>` waiver instead.",
            errors.len()
        );
        return Ok(());
    }

    bail!(
        "doctor gate: {} blocking error(s) for {} — refusing to emit a broken tree. \
         Run `lazuli doctor {}` for detail, fix the errors, or bypass with --no-gate \
         (loud) / per-finding `# doctor:allow <CODE>`.",
        errors.len(),
        input.display(),
        input.display(),
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn diag(code: &str, severity: DoctorSeverity) -> DoctorDiagnostic {
        DoctorDiagnostic {
            path: PathBuf::from("app.lzi"),
            line: 1,
            column: 1,
            severity,
            code: code.to_owned(),
            message: "fixture".to_owned(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }
    }

    #[test]
    fn error_finding_blocks() {
        let diags = vec![diag("CREATES-EMPTY-BINDINGS-001", DoctorSeverity::Error)];
        let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
        assert!(res.is_err(), "an error finding must fail the gate");
    }

    #[test]
    fn warnings_only_pass() {
        let diags = vec![diag("VOCAB-SOMETHING-001", DoctorSeverity::Warning)];
        let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
        assert!(res.is_ok(), "warnings-only must pass the gate");
    }

    #[test]
    fn no_gate_bypasses_errors() {
        let diags = vec![diag("CREATES-EMPTY-BINDINGS-001", DoctorSeverity::Error)];
        let bypass = GateBypass {
            no_gate: true,
            allow_version_mismatch: false,
        };
        let res = render_and_gate(&diags, Path::new("app.lzi"), bypass);
        assert!(res.is_ok(), "--no-gate must bypass error findings");
    }

    #[test]
    fn version_findings_drop_under_allow_version_mismatch() {
        let diags = vec![diag("LAZULI-VERSION-001", DoctorSeverity::Error)];
        // Without the flag: blocks.
        assert!(
            render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default()).is_err(),
            "version finding blocks by default"
        );
        // With the flag: drops out of the gate set, passes.
        let bypass = GateBypass {
            no_gate: false,
            allow_version_mismatch: true,
        };
        assert!(
            render_and_gate(&diags, Path::new("app.lzi"), bypass).is_ok(),
            "--allow-version-mismatch drops LAZULI-VERSION-* from the gate"
        );
    }
}
