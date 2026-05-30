//! Framework-development dispatch (`lazuli-dev`).
//!
//! SPEC-20 2/n splits the framework-*development* commands off the published
//! `lazuli` binary entirely (not merely `hide = true`). These mean nothing to
//! an app developer and would be cognitive noise + a compat obligation on the
//! published surface:
//!
//! - `parse` — dump the parser skeleton AST.
//! - `spike-generate` — regenerate the runtime-form codegen spike fixtures.
//! - `examples` — manage THIS repo's curated example fixtures.
//! - `self-doctor` — audit Lazuli's own Rust under `INTERNAL-*` (the old
//!   `doctor --self`).
//!
//! Contributors run `cargo run -p lazuli_cli --bin lazuli-dev -- <cmd>`; the
//! `scripts/dev-check.sh` orchestrator wraps `self-doctor` alongside the other
//! freshness gates. The handlers + the `build_module_from_path` compile entry
//! they reach for live in the library crate, shared with [`crate::cli_run`].

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli_args::{CheckSecurityProfile, ExamplesCommand};
use crate::{commands, doctor};

#[derive(Debug, Parser)]
#[command(name = "lazuli-dev", version = env!("CARGO_PKG_VERSION"))]
#[command(
    about = "Lazuli framework-development tooling (contributors only; not an app-dev surface)"
)]
pub(crate) struct DevCli {
    #[command(subcommand)]
    command: DevCommands,
    /// Skip the Lazurite.toml [lazuli] runtime version pin check.
    #[arg(long, global = true)]
    allow_version_mismatch: bool,
}

#[derive(Debug, Subcommand)]
enum DevCommands {
    /// Dump the parser skeleton AST for a `.lzi` / `.lzx` file.
    Parse { input: PathBuf },
    /// Regenerate the runtime-form `customer.gen.go` / `customer.gen.ts`
    /// spike fixtures from a runtime spec (or the in-process fixture).
    SpikeGenerate {
        /// Workspace root (defaults to the current directory).
        #[arg(long, short, default_value = ".")]
        root: PathBuf,
        /// Optional JSON path to a serialised `RuntimeFeature`.
        #[arg(long)]
        spec: Option<PathBuf>,
    },
    /// Manage this repo's curated example fixtures (bundle / validate).
    Examples {
        #[command(subcommand)]
        sub: ExamplesCommand,
    },
    /// Audit the framework's own Rust source (`INTERNAL-*` findings) — the
    /// dogfood self-check formerly spelled `lazuli doctor --self`.
    SelfDoctor {
        /// Workspace root to audit. Defaults to the current directory; the
        /// self-audit walks `crates/lazuli_*/src/` beneath it.
        #[arg(default_value = ".")]
        input: PathBuf,
        #[arg(long, value_enum)]
        security_profile: Option<CheckSecurityProfile>,
        #[arg(long)]
        check_release: bool,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        coverage: bool,
        #[arg(long = "fail-on", action = clap::ArgAction::Append)]
        fail_on: Vec<String>,
    },
}

/// Parse `argv` and dispatch the framework-development command surface.
///
/// ## Examples
///
/// ```no_run
/// // The `lazuli-dev` binary is a one-line shell over this entry point:
/// fn main() -> anyhow::Result<()> {
///     lazuli_cli::run_dev()
/// }
/// ```
pub fn run_dev() -> Result<()> {
    let cli = DevCli::parse();
    match cli.command {
        DevCommands::Parse { input } => commands::parse::parse_command(&input),
        DevCommands::SpikeGenerate { root, spec } => {
            commands::spike_generate::spike_generate_command(&root, spec.as_deref())
        }
        DevCommands::Examples { sub } => match sub {
            ExamplesCommand::Bundle { out } => commands::examples::bundle_command(out.as_deref()),
            ExamplesCommand::Validate { check_decay } => {
                commands::examples::validate_command(check_decay)
            }
        },
        DevCommands::SelfDoctor {
            input,
            security_profile,
            check_release,
            format,
            coverage,
            fail_on,
        } => doctor::doctor_command_with_options(
            &input,
            security_profile.map(Into::into),
            check_release,
            cli.allow_version_mismatch,
            doctor::DoctorRuntimeOptions {
                format: Some(format),
                coverage,
                fail_on,
                self_audit: true,
            },
        ),
    }
}
