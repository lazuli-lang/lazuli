//! `lazuli profile <pprof>` — read a pprof profile and report top-N
//! ops by Lazuli semantics.
//!
//! Thin axis dispatcher over `crate::profile::run_profile`. Accepts
//! `--by cpu|alloc|block` (closed catalog; rejects anything else with
//! a typed error) and `--format text|json`. The actual pprof parsing
//! and the `@semantic.<word>` rollup live in `crate::profile`.
//!
//! Cross-refs:
//! - `crate::profile::run_profile` — pprof reader + rollup.
//! - `crate::profile::format_report` — the text renderer.
//! - The `Commands::Profile` clap arm in `main.rs` — surface.

use std::path::Path;

use anyhow::{Result, bail};

use crate::profile;

/// Handler for the `Commands::Profile` clap arm.
pub fn profile_command(profile_path: &Path, top: usize, by: &str, format: &str) -> Result<()> {
    let axis = match by {
        "cpu" => profile::ProfileAxis::Cpu,
        "alloc" => profile::ProfileAxis::Alloc,
        "block" => profile::ProfileAxis::Block,
        other => bail!("unknown profile axis `{other}`; expected cpu, alloc, or block"),
    };
    let report = profile::run_profile(profile_path, top, axis)
        .map_err(|err| anyhow::anyhow!("failed to read profile: {err}"))?;
    match format {
        "text" => {
            print!("{}", profile::format_report(&report));
            Ok(())
        }
        "json" => {
            let payload = serde_json::json!({
                "top_ops": report.top_ops,
                "top_patterns": report.top_patterns,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            Ok(())
        }
        other => bail!("unknown profile format `{other}`; expected text or json"),
    }
}
