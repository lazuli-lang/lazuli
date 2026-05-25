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
pub fn export_command(target: cmd_design::ExportTarget, out: &Path) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    let design_path = cmd_design::default_design_path(&project_root);
    let design = cmd_design::read_design(&design_path)?;
    cmd_design::export(out, target, &design)
}

/// Handler for `DesignCommand::Diff`.
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
