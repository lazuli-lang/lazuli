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
///
/// Compiles `input` to IR via `build_module_from_path` and dumps the
/// pretty-printed JSON to stdout. Errors propagate with the input path
/// in their context message.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::parse::parse_command;
///
/// // parse_command(Path::new("examples/crm.lzi"))?;
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_errors_on_missing_input() {
        let result = parse_command(Path::new("__lazuli_no_such_file.lzi"));
        assert!(result.is_err());
    }
}
