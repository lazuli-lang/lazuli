//! `lazuli check <input>` — run the LSP diagnostic surface over a file
//! or directory of `.lzi`/`.lzx` sources.
//!
//! `check` is the CI gate that pulls every diagnostic
//! `lazuli_lsp::diagnostics_for_source_with_profile` would emit for the
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
//! - `lazuli_lsp::diagnostics_for_source_with_profile` — the kernel.
//! - `crate::lazurite_manifest::load` / `crate::version::enforce_manifest_pin`
//!   — the pin gate.
//! - `commands/doctor.rs` (lives in main.rs today via `mod doctor`) —
//!   the super-set linter.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_lsp::SecurityProfile;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{lazurite_manifest, version};

/// Handler for the `Commands::Check` clap arm.
pub fn check_command(
    input: &Path,
    security_profile: SecurityProfile,
    allow_version_mismatch: bool,
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
    let mut has_error = false;

    for path in &inputs {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let diagnostics =
            lazuli_lsp::diagnostics_for_source_with_profile(&source, security_profile);
        has_error |= diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR));

        for diagnostic in &diagnostics {
            print_diagnostic(path, diagnostic);
        }
    }

    if has_error {
        bail!(
            "{} failed Lazuli checks under {:?} security profile",
            input.display(),
            security_profile
        );
    }

    println!("{} passed Lazuli checks", input.display());
    Ok(())
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
