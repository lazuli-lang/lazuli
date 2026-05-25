//! `lazuli parse <input>` — compile a `.lzi` to IR and dump it as
//! pretty-printed JSON.
//!
//! The thinnest possible introspection surface. Useful when an author
//! wants to see the post-lift IR a file produces without the inspect
//! command's expansion/filtering layers. Output is `serde_json`
//! pretty-printing of the `lazuli_ir::Module` — a wire format the LLM
//! and CI tooling can chew on.
//!
//! For typical agent workflows prefer `lazuli inspect --format=json`,
//! which is the stabilised, expansion-aware shape. `lazuli parse`
//! stays as the raw compile-to-IR shortcut for framework debugging.
//!
//! Cross-refs:
//! - `crate::build_module_from_path` — the actual compiler entry.
//! - `commands/inspect.rs` (TBD) — the expansion-aware surface.

use std::path::Path;

use anyhow::{Context, Result};

/// Handler for the `Commands::Parse` clap arm.
pub fn parse_command(input: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    println!("{}", serde_json::to_string_pretty(&app)?);
    Ok(())
}

/// Compile `.lzi`/`.lzx` source at `input` into a typed
/// `lazuli_ir::Module`. Wraps `crate::build_module_from_path` with a
/// human-readable error context. Kept private because every other
/// caller already constructs the module directly.
fn compile_to_ir(input: &Path) -> Result<lazuli_ir::Module> {
    crate::build_module_from_path(input).context("failed to compile .lzi file")
}
