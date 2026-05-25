//! `lazuli lsp` — start the Lazuli Language Server over stdio.
//!
//! Thin shim: spins up a single-threaded tokio runtime and blocks on
//! `lazuli_lsp::serve_stdio()`, which speaks the LSP wire protocol on
//! stdin/stdout. The CLI surface accepts (and ignores) `--stdio` for
//! compatibility with VS Code's `vscode-languageclient`, which always
//! appends that flag when `TransportKind.stdio` is selected.
//!
//! The actual LSP server — diagnostics, hover, completion, code actions,
//! workspace symbols — lives in the `lazuli_lsp` crate. This handler is
//! purely the process entry point; everything else is reusable from
//! tests, editor plugins, or alternate transports.
//!
//! Cross-refs:
//! - `lazuli_lsp::serve_stdio` — actual server entry.
//! - `editors/vscode/` — the consumer that spawns this subcommand.

use anyhow::{Context, Result};

/// Handler for the `Commands::Lsp` clap arm. Blocks the current thread
/// running the LSP event loop until stdin is closed.
pub fn lsp_command() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start Lazuli LSP runtime")?;
    runtime.block_on(lazuli_lsp::serve_stdio());
    Ok(())
}
