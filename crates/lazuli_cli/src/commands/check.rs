//! `lazuli check <input>` — run the LSP diagnostic surface over a file
//! or directory of `.lzi`/`.lzx` sources.
//!
//! `check` is the CI gate that pulls every diagnostic
//! `lazuli_lsp::diagnostics_for_source_with_profile_cli` would emit for the
//! given `SecurityProfile`, prints them in a `path:line:col: severity:
//! message` shape, and exits non-zero when any are errors. It is the
//! same kernel `lazuli doctor` builds on — `doctor` adds cross-file
//! validations, coverage layers, and policy gates; `check` stays
//! diagnostic-only.
//!
//! When `allow_version_mismatch == false`, the handler first enforces
//! the `Lazurite.toml [lazuli]` version pin (mirrors what `generate`
//! does) so authors don't accidentally run a newer compiler against an
//! older pin without noticing.
//!
//! Cross-refs:
//! - `lazuli_lsp::diagnostics_for_source_with_profile_cli` — the kernel
//!   (CLI/batch variant: runs the parser/lower backstop over canonical
//!   `.lzi` sources, unlike the editor's per-keystroke pass).
//! - `crate::lazurite_manifest::load` / `crate::version::enforce_manifest_pin`
//!   — the pin gate.
//! - `commands/doctor.rs` (lives in main.rs today via `mod doctor`) —
//!   the super-set linter.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{lazurite_manifest, version};

/// Handler for the `Commands::Check` clap arm.
///
/// Enforces the manifest pin (unless `allow_version_mismatch` is set),
/// walks `input` for `.lzi`/`.lzx` files, runs the LSP diagnostic
/// kernel under `security_profile`, and prints findings in the canonical
/// `path:line:col: severity[code]: message` shape. Errors `bail!` non-zero.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor_config::DoctorProfile as SecurityProfile;
/// use lazuli_cli::commands::check::check_command;
///
/// // Run with `allow_version_mismatch = true` against a single file:
/// // check_command(Path::new("app.lzx"), SecurityProfile::Strict, true)?;
/// ```
pub fn check_command(
    input: &Path,
    security_profile: SecurityProfile,
    allow_version_mismatch: bool,
) -> Result<()> {
    check_command_with_gate(input, security_profile, allow_version_mismatch, false)
}

/// `check` with the W0-4 escape hatch threaded in.
///
/// `no_gate` (or `LAZULI_NO_GATE=1`) downgrades the ERROR→non-zero gate
/// to a loud bypass: every finding is still printed (error-first, with the
/// `N errors, M warnings` banner), but the command exits 0. Per-finding
/// `# doctor:allow <CODE>` waivers are honored upstream inside the rules
/// exactly as before — `no_gate` is the coarse all-or-nothing override.
pub fn check_command_with_gate(
    input: &Path,
    security_profile: SecurityProfile,
    allow_version_mismatch: bool,
    no_gate: bool,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = crate::project_root_for_input(input);
        let manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to read {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;
        version::enforce_manifest_pin(manifest.as_ref())?;
    }

    let inputs = check_inputs(input)?;

    // Collect every finding with its owning path so we can render
    // ERROR-first with a count banner (W3-4 DX: errors before warnings).
    let mut rows: Vec<(PathBuf, Diagnostic)> = Vec::new();
    for path in &inputs {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let diagnostics =
            lazuli_lsp::diagnostics_for_source_with_profile_cli(&source, security_profile);
        for diagnostic in diagnostics {
            rows.push((path.clone(), diagnostic));
        }
    }

    let error_count = rows
        .iter()
        .filter(|(_, d)| d.severity == Some(DiagnosticSeverity::ERROR))
        .count();
    let warning_count = rows
        .iter()
        .filter(|(_, d)| d.severity == Some(DiagnosticSeverity::WARNING))
        .count();

    // Stable error-first ordering: severity rank, then path/line/col.
    rows.sort_by(|(pa, da), (pb, db)| {
        severity_rank(da)
            .cmp(&severity_rank(db))
            .then_with(|| pa.cmp(pb))
            .then_with(|| da.range.start.line.cmp(&db.range.start.line))
            .then_with(|| da.range.start.character.cmp(&db.range.start.character))
    });

    if !rows.is_empty() {
        eprintln!("check: {error_count} error(s), {warning_count} warning(s)");
    }
    for (path, diagnostic) in &rows {
        print_diagnostic(path, diagnostic);
    }

    if error_count == 0 {
        println!("{} passed Lazuli checks", input.display());
        return Ok(());
    }

    let bypass = no_gate || matches!(std::env::var("LAZULI_NO_GATE").ok().as_deref(), Some("1"));
    if bypass {
        eprintln!(
            "check: BYPASSED {error_count} error(s) via --no-gate / LAZULI_NO_GATE=1 — \
             the findings above are real; fix them or add a per-finding \
             `# doctor:allow <CODE>` waiver instead."
        );
        return Ok(());
    }

    bail!(
        "{} failed Lazuli checks under {:?} security profile ({error_count} error(s))",
        input.display(),
        security_profile
    );
}

/// Severity ordering rank — lower sorts first. ERROR before WARNING
/// before INFO/HINT before the unset catch-all.
fn severity_rank(d: &Diagnostic) -> u8 {
    match d.severity {
        Some(DiagnosticSeverity::ERROR) => 0,
        Some(DiagnosticSeverity::WARNING) => 1,
        Some(DiagnosticSeverity::INFORMATION) => 2,
        Some(DiagnosticSeverity::HINT) => 3,
        _ => 4,
    }
}

/// Resolve `input` (a file or directory) to the concrete set of
/// `.lzi`/`.lzx` files the diagnostic pass should walk. Used by
/// `check_command` and by the `parse` subcommand's own scan.
fn check_inputs(input: &Path) -> Result<Vec<PathBuf>> {
    if !input.is_dir() {
        return Ok(vec![input.to_path_buf()]);
    }

    let mut paths = Vec::new();
    let mut stack = vec![input.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in
            fs::read_dir(&path).with_context(|| format!("failed to read {}", path.display()))?
        {
            let path = entry
                .with_context(|| format!("failed to read entry under {}", path.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("lzi" | "lzx")
            ) {
                paths.push(path);
            }
        }
    }

    paths.sort();
    if paths.is_empty() {
        bail!("no .lzi or .lzx files found under {}", input.display());
    }
    Ok(paths)
}

/// Format a single LSP `Diagnostic` in the canonical
/// `path:line:col: severity[code]: message` shape used by every
/// CI-facing CLI surface.
fn print_diagnostic(input: &Path, diagnostic: &Diagnostic) {
    let severity = match diagnostic.severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "diagnostic",
    };
    let code = diagnostic
        .code
        .as_ref()
        .map(|code| match code {
            tower_lsp::lsp_types::NumberOrString::String(value) => format!(" [{value}]"),
            tower_lsp::lsp_types::NumberOrString::Number(value) => format!(" [{value}]"),
        })
        .unwrap_or_default();
    println!(
        "{}:{}:{}: {severity}{code}: {}",
        input.display(),
        diagnostic.range.start.line + 1,
        diagnostic.range.start.character + 1,
        diagnostic.message
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_inputs_single_file_passes_through() {
        // A non-directory path returns itself as the sole input.
        let result = check_inputs(Path::new("Cargo.toml")).expect("non-dir path");
        assert_eq!(result, vec![PathBuf::from("Cargo.toml")]);
    }
}
