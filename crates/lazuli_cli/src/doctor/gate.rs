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
use lazuli_doctor::RuleCategory;
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

/// Resolve a diagnostic's rule category — the explicit `category` field
/// when a rule set it, else derived from the code prefix (the same
/// fallback the rest of the doctor pipeline uses).
fn effective_category(d: &DoctorDiagnostic) -> RuleCategory {
    d.category
        .unwrap_or_else(|| RuleCategory::from_code_prefix(&d.code))
}

/// Whether an `Error`-severity finding in this category should **block**
/// the generate/check gate (refuse-emit) vs merely be reported.
///
/// The gate's job is to refuse to emit a tree the doctor proved is
/// **broken or unsafe** — the "this emitted app does not work / is
/// insecure" dimensions. Style / hygiene / advisory dimensions are
/// reported (error-first, in the banner) but must NOT fail-close code
/// generation: e.g. `design-token-undefined` (a frontend Tailwind
/// hygiene lint, `Design`) is error-severity and fires ~1504× on a real
/// frontend-bearing app — letting it block `lazuli generate go` is a
/// footgun that fail-closes every pilot. A CSS/design lint cannot make
/// the GENERATED GO TREE broken.
///
/// BLOCKING (an error here means the emitted artifact is broken/unsafe):
/// - `Correctness`  — wiring/shape bugs (unwired handler, empty bindings,
///   signature mismatch, undeclared enum variant). The app breaks.
/// - `Security`     — auth/cookie/CORS/CSRF/webhook holes. Unsafe.
/// - `ErrorHandling`— panic / string-error / swallowed handler errors.
///   The contract the runtime depends on is violated.
/// - `Lifecycle`    — resource/state lifecycle correctness.
/// - `CrossFeature` — cross-feature wiring contracts.
/// - `ErrorVocab`   — declared-error catalog integrity the runtime routes on.
///
/// NON-BLOCKING (style / hygiene / advisory — reported, never refuse-emit):
/// - `Design`       — frontend/CSS/design-token hygiene. Does not touch Go.
/// - `Vocabulary`   — naming/vocab advice + the catch-all fallback bucket.
/// - `LziHygiene`   — `.lzi` source-shape / file-size hygiene.
/// - `InternalHygiene` — framework dogfooding (`--self`), not user artifacts.
/// - `TestDiscipline`  — TDD/BDD coverage advice.
/// - `EscapeHatch`  — sanctioned-escape *visibility* notes, not bugs.
/// - `Poller` / `Report` — advisory surface notes.
/// - `Encryption`   — advisory here; the *enforcing* encryption holes
///   surface under `Security` (ENCRYPT-* is hygiene/advice today).
/// - `Domain`       — domain-modelling advice, not a wiring break.
fn is_blocking_category(cat: RuleCategory) -> bool {
    matches!(
        cat,
        RuleCategory::Correctness
            | RuleCategory::Security
            | RuleCategory::ErrorHandling
            | RuleCategory::Lifecycle
            | RuleCategory::CrossFeature
            | RuleCategory::ErrorVocab
    )
}

/// Whether an error-severity diagnostic should block the gate.
///
/// Blocks when its [`effective_category`] is a concrete-bug category, OR
/// when it is a `LAZULI-VERSION-*` pin-mismatch finding. The latter has
/// no dedicated `RuleCategory` (it derives to `Vocabulary`), but a
/// runtime/contract version mismatch is a genuine "this tree won't work
/// against the pinned runtime" bug, not style — so it stays blocking by
/// code (and is already dropped from the gate set entirely when
/// `--allow-version-mismatch` is in effect, handled upstream).
fn is_blocking_error(d: &DoctorDiagnostic) -> bool {
    is_version_finding(d) || is_blocking_category(effective_category(d))
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

    // Scope the gate's BLOCKING decision to concrete-bug categories. An
    // error in a style/hygiene/advisory dimension (Design tailwind
    // hygiene, Vocabulary advice, LZI source-shape, …) is STILL PRINTED
    // error-first and STILL counted, but it must NOT refuse-emit a Go
    // tree — a CSS lint can't make the emitted app broken. Only errors
    // in the "this code is broken / unsafe" set fail the gate.
    let blocking_count = errors
        .iter()
        .filter(|d| is_blocking_error(d))
        .count();

    // Error-first banner + summary. Distinguish blocking errors (which
    // refuse-emit) from the total error count (which includes non-blocking
    // style/hygiene errors that are reported but never block).
    if !errors.is_empty() || !warnings.is_empty() {
        eprintln!(
            "doctor gate: {} blocking error(s), {} error(s) total, {} warning(s)",
            blocking_count,
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

    if blocking_count == 0 {
        return Ok(());
    }

    if bypass.no_gate_effective() {
        eprintln!(
            "doctor gate: BYPASSED {} blocking error(s) via --no-gate / LAZULI_NO_GATE=1 — \
             emitting a tree that the doctor flagged as broken. Fix the errors above \
             or add a per-finding `# doctor:allow <CODE>` waiver instead.",
            blocking_count
        );
        return Ok(());
    }

    bail!(
        "doctor gate: {} blocking error(s) for {} — refusing to emit a broken tree \
         (concrete-bug categories: correctness, security, error_handling, lifecycle, \
         cross_feature, error_vocab; style/hygiene errors are reported but do not block). \
         Run `lazuli doctor {}` for detail, fix the errors, or bypass with --no-gate \
         (loud) / per-finding `# doctor:allow <CODE>`.",
        blocking_count,
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
        assert!(res.is_err(), "a Correctness error finding must fail the gate");
    }

    /// FIX 1 — a Design-category error (frontend Tailwind hygiene, e.g.
    /// `design-token-undefined`) must NOT block the gate even at
    /// error-severity: a CSS/design lint can't make the emitted Go tree
    /// broken. It is still printed/counted but never refuse-emit.
    #[test]
    fn design_error_does_not_block() {
        let diags = vec![diag("DESIGN-TOKEN-UNDEFINED-001", DoctorSeverity::Error)];
        // Sanity: this code derives to the Design category.
        assert_eq!(
            effective_category(&diags[0]),
            RuleCategory::Design,
            "fixture must be a Design-category finding"
        );
        let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
        assert!(
            res.is_ok(),
            "a Design-category error must NOT block code generation"
        );
    }

    /// FIX 1 — the complement: a Correctness-category error in the SAME
    /// stream still blocks (the Design error doesn't mask it), and the
    /// gate's `blocking_count` reflects only the concrete-bug error.
    #[test]
    fn correctness_error_blocks_even_alongside_design_error() {
        let diags = vec![
            diag("DESIGN-TOKEN-UNDEFINED-001", DoctorSeverity::Error),
            diag("CREATES-EMPTY-BINDINGS-001", DoctorSeverity::Error),
        ];
        let blocking = diags.iter().filter(|d| is_blocking_error(d)).count();
        assert_eq!(blocking, 1, "only the Correctness error is blocking");
        let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
        assert!(
            res.is_err(),
            "a Correctness error must block even when a Design error is present"
        );
    }

    /// FIX 1 — Vocabulary / LziHygiene / TestDiscipline errors are
    /// style/advisory and do not block.
    #[test]
    fn style_and_hygiene_errors_do_not_block() {
        for code in [
            "VOCAB-NAMING-001",
            "LZI-FILE-SIZE-001",
            "TEST-MISSING-AUTHORED-001",
            "ESC-RAWSQL-IN-HANDLER-001",
        ] {
            let diags = vec![diag(code, DoctorSeverity::Error)];
            let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
            assert!(
                res.is_ok(),
                "{code} (style/hygiene/advisory) must not block the gate"
            );
        }
    }

    /// FIX 1 — concrete-bug categories all block.
    #[test]
    fn concrete_bug_errors_block() {
        for code in [
            "CREATES-EMPTY-BINDINGS-001", // Correctness
            "AUTH-COOKIE-INSECURE-001",   // Security
            "HANDLER-NO-PANIC-001",       // ErrorHandling
            "LIFECYCLE-FOO-001",          // Lifecycle
            "CROSS-FEATURE-FOO-001",      // CrossFeature
            "ERR-VOCAB-001",              // ErrorVocab
        ] {
            let diags = vec![diag(code, DoctorSeverity::Error)];
            let res = render_and_gate(&diags, Path::new("app.lzi"), GateBypass::default());
            assert!(res.is_err(), "{code} (concrete bug) must block the gate");
        }
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
