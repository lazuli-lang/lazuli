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

use xtask::tmlanguage;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next();
    match cmd.as_deref() {
        Some("gen-tmlanguage") => {
            let check = args.any(|a| a == "--check");
            match tmlanguage::run(check) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("dump-groups") => {
            tmlanguage::dump_groups();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown xtask subcommand `{other}`");
            eprintln!("usage: cargo xtask gen-tmlanguage [--check]");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("usage: cargo xtask gen-tmlanguage [--check]");
            ExitCode::FAILURE
        }
    }
}
