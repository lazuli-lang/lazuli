//! `lazuli init <path>` — write a minimal starter `.lzi` to `<path>`.
//!
//! The thinnest scaffolding the CLI offers: creates parent directories
//! as needed and writes the in-binary `DEFAULT_TEMPLATE` (compiled in
//! from `examples/crm.lzi`) at the requested location. Authors run this
//! when they want a single-file Lazuli scratchpad without the full
//! `lazuli new` project tree (which adds `Lazurite.toml`, `dist/`,
//! `go.work`, frontend skeletons, and the rest of the scaffold).
//!
//! Cross-refs:
//! - `lazuli new`: full project scaffold with manifest + tooling.
//! - `crate::templates`: full project template tarball used by `new`.
//! - Constant `DEFAULT_TEMPLATE` lives in `main.rs` because it is also
//!   shared with a couple of other call sites; passed in as an argument
//!   here to keep this module free of `include_str!` indirection.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Handler for the `Commands::Init` clap arm. Writes `template` to
/// `path`, creating intermediate directories. Mirrors the pre-split
/// behaviour byte-for-byte: same `with_context` messages, same `println!`
/// echo, same `Ok(())` return shape.
pub fn init_command(path: &Path, template: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    fs::write(path, template)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("created {}", path.display());
    Ok(())
}
