//! `lazuli design import|export|diff` — bridge between the canonical
//! `design.lzi` design-token catalog and external design-tool formats
//! (Figma export bundles, Style Dictionary JSON).
//!
//! `import` populates / refreshes a project's `design.lzi` from an
//! external token dump (`--overwrite` to replace, otherwise refuse on
//! conflict). `export` writes the inverse — the existing `design.lzi`
//! serialized into the chosen external format. `diff` reports drift
//! between `design.lzi` and an external snapshot, exiting non-zero
//! when changes are detected so CI can gate on it.
//!
//! Format / target plumbing (`ImportFormat`, `ExportTarget`) lives in
//! `crate::cmd_design`; this module is the thin dispatcher that
//! resolves the project root, computes the canonical `design.lzi`
//! path, and forwards into the typed call.
//!
//! Cross-refs:
//! - `crate::cmd_design::{import, export, diff, read_design,
//!   default_design_path}` — the actual implementations.
//! - `commands/generate/design.rs` (TBD) — the codegen-side surface
//!   that emits `dist/ts-*/design/` from the same catalog.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cmd_design;

/// Handler for `DesignCommand::Import`.
///
/// Resolves the project root and the canonical `design.lzi` path, then
/// delegates to [`cmd_design::import`] with the chosen external format
/// and overwrite policy.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::ImportFormat;
/// use lazuli_cli::commands::design::import_command;
///
/// // import_command(Path::new("figma-export.json"), ImportFormat::Figma, false)?;
/// ```
pub fn import_command(
    from: &Path,
    format: cmd_design::ImportFormat,
    overwrite: bool,
) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    let design_path = cmd_design::default_design_path(&project_root);
    cmd_design::import(from, format, &design_path, overwrite)
}

/// Handler for `DesignCommand::Export`.
///
/// Reads `design.lzi` from the resolved project root and writes its
/// serialization to `out` in the requested `target` format.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::ExportTarget;
/// use lazuli_cli::commands::design::export_command;
///
/// // export_command(ExportTarget::StyleDictionary, Path::new("tokens.json"))?;
/// ```
pub fn export_command(target: cmd_design::ExportTarget, out: &Path) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    let design_path = cmd_design::default_design_path(&project_root);
    let design = cmd_design::read_design(&design_path)?;
    cmd_design::export(out, target, &design)
}

/// Handler for `DesignCommand::Diff`.
///
/// Diffs the project's `design.lzi` against an external snapshot at
/// `against`, prints the rendered report, and `bail!`s non-zero when
/// the report is non-empty so CI can gate on drift.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::design::diff_command;
///
/// // diff_command(Path::new("figma-export.json"))?;
/// ```
pub fn diff_command(against: &Path) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    let design_path = cmd_design::default_design_path(&project_root);
    let design = cmd_design::read_design(&design_path)?;
    let report = cmd_design::diff(against, &design)?;
    print!("{}", report.render());
    if report.is_empty() {
        Ok(())
    } else {
        bail!("design diff found changes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_against_missing_path_errors() {
        // No `design.lzi` next to the test cwd should surface as an error
        // rather than panic.
        let result = diff_command(Path::new("__lazuli_no_such_external.json"));
        assert!(result.is_err());
    }
}
