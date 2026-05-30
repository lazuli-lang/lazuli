//! `xtask` — dev-only build tasks for the Lazuli workspace.
//!
//! Today it owns one job: projecting the `lazuli_keywords::ALL` registry to
//! the TextMate keyword-alternation repository rules (`#kw-*`) of the VS Code
//! grammar, so new keywords are auto-highlighted and drift is impossible.
//!
//! Boundary: this tool is a *pure projector*. It deps `lazuli_keywords` +
//! `serde_json` ONLY — never the parser/analyzer/runtime. The registry is the
//! single input.

use std::process::ExitCode;

use xtask::{catalog_reference, keyword_reference, tmlanguage};

const USAGE: &str =
    "usage: cargo xtask <gen-tmlanguage | gen-keyword-reference | gen-catalog-reference> [--check]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next();
    match cmd.as_deref() {
        Some("gen-tmlanguage") => {
            let check = args.any(|a| a == "--check");
            finish(tmlanguage::run(check))
        }
        Some("gen-keyword-reference") => {
            let check = args.any(|a| a == "--check");
            finish(keyword_reference::run(check))
        }
        Some("gen-catalog-reference") => {
            let check = args.any(|a| a == "--check");
            finish(catalog_reference::run(check))
        }
        Some("dump-groups") => {
            tmlanguage::dump_groups();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown xtask subcommand `{other}`");
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Map a subcommand `Result` to a process exit code, printing any error.
fn finish(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
