//! Published-CLI dispatch (`lazuli`).
//!
//! Hoisted out of `main.rs` (SPEC-20 2/n) so the command logic lives in the
//! **library** crate and a second binary (`lazuli-dev`, the framework-dev
//! surface) can reuse the same handlers + the `build_module_from_path` compile
//! pipeline. `main.rs` is now a thin shell over [`run`]; the framework-dev
//! commands (`parse`, `spike-generate`, `examples`, `self-doctor`) dispatch
//! from [`crate::cli_dev::run_dev`] and are NOT reachable from this surface.

use std::path::Path;

use anyhow::Result;
use clap::Parser;

use crate::cli_args::{Cli, Commands, DesignCommand, MigrateCommand, TranslateCommand};
use crate::{commands, doctor};

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");

/// Parse `argv` and dispatch the published (app-developer) command surface.
///
/// The framework-dev commands live in [`crate::cli_dev`]; this function never
/// sees them — they are a separate clap surface on the `lazuli-dev` binary.
///
/// ## Examples
///
/// ```no_run
/// // The `lazuli` binary is a one-line shell over this entry point:
/// fn main() -> anyhow::Result<()> {
///     lazuli_cli::run()
/// }
/// ```
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            input,
            security_profile,
        } => commands::check::check_command(
            &input,
            security_profile.into(),
            cli.allow_version_mismatch,
        ),
        Commands::Doctor {
            input,
            security_profile,
            check_release,
            format,
            coverage,
            fail_on,
        } => doctor::doctor_command_with_options(
            &input,
            // Precedence is resolved in doctor::run: explicit
            // --security-profile flag > toml `[doctor] profile` > strict.
            // `None` here means "no flag given; honor the manifest".
            security_profile.map(Into::into),
            check_release,
            cli.allow_version_mismatch,
            doctor::DoctorRuntimeOptions {
                format: Some(format),
                coverage,
                fail_on,
                self_audit: false,
            },
        ),
        Commands::Inspect {
            input,
            expand,
            include,
            format,
        } => commands::inspect::inspect_command(&input, &expand, format, &include),
        Commands::Debug {
            error,
            capsule,
            project,
            format,
        } => commands::debug::debug_command(&project, error.as_deref(), capsule, &format),
        Commands::Profile {
            profile,
            top,
            by,
            format,
        } => commands::profile::profile_command(&profile, top, &by, &format),
        Commands::Init { path } => commands::init::init_command(&path, DEFAULT_TEMPLATE),
        Commands::New {
            project_name,
            template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        } => commands::new::new_command(
            project_name.as_deref(),
            &template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        ),
        Commands::Lsp { stdio: _ } => commands::lsp::lsp_command(),
        Commands::Plan { input, check } => commands::plan::plan_command(&input, check.as_deref()),
        Commands::Generate {
            kind,
            input,
            output,
            api_version,
            module,
            lazuli_go_version,
            check,
            with_source,
            allow_drops,
            playwright_target,
        } => commands::generate::generate_command(
            kind,
            &input,
            output.as_deref(),
            api_version.as_deref(),
            module.as_deref(),
            lazuli_go_version.as_deref(),
            check,
            with_source,
            allow_drops,
            cli.allow_version_mismatch,
            playwright_target,
        ),
        Commands::Dev {
            path,
            out,
            no_run,
            debounce,
        } => commands::dev::dev_command(path, out, no_run, debounce),
        Commands::Migrate { sub } => match sub {
            MigrateCommand::Up { target, yes } => commands::migrate::up_command(target, yes),
            MigrateCommand::Down { steps, yes } => commands::migrate::down_command(steps, yes),
            MigrateCommand::Status => commands::migrate::status_command(),
            MigrateCommand::Dsl {
                from,
                to,
                dry_run,
                path,
            } => commands::migrate::dsl_command(&from, &to, dry_run, path),
        },
        Commands::Design { sub } => match sub {
            DesignCommand::Import {
                from,
                format,
                overwrite,
            } => commands::design::import_command(&from, format.into(), overwrite),
            DesignCommand::Export { target, out } => {
                commands::design::export_command(target.into(), &out)
            }
            DesignCommand::Diff { against } => commands::design::diff_command(&against),
        },
        Commands::Upgrade {
            from,
            to,
            target,
            dry_run,
        } => commands::upgrade::upgrade_command(&from, &to, &target, dry_run),
        Commands::Seed { only, force } => commands::seed::seed_command(only.as_deref(), force),
        Commands::Changelog { from, to, output } => {
            commands::changelog::changelog_command(&from, &to, output.as_deref())
        }
        Commands::Translate { sub } => match sub {
            TranslateCommand::Extract {
                input,
                out,
                locale,
                check,
            } => commands::translate::translate_extract_command(
                &input,
                &out,
                locale.as_deref(),
                check,
            ),
        },
        Commands::Mcp => commands::mcp::mcp_command(),
        Commands::Test {
            input,
            layer,
            format,
            coverage,
            fail_on,
            watch,
            fail_fast,
            aggregate_method,
            extra_args,
        } => commands::test::test_command(
            input,
            layer.into_iter().map(Into::into).collect(),
            format,
            coverage,
            fail_on,
            watch,
            fail_fast,
            aggregate_method,
            extra_args,
        ),
    }
}

/// Back-compat shim for callers that pre-date the W4.5 R2 split of
/// `lazuli generate go` into `commands/generate/go.rs`. `dev::regen`
/// and a handful of integration tests still spell the call as
/// `crate::generate_go(...)`; rather than touch those modules
/// (whose ownership lives outside W4.5 R2's scope), we keep this
/// single-line forward shim. Internal callers should use
/// `crate::commands::generate::go::generate_go` directly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_go(
    input: &Path,
    output: Option<&Path>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
) -> Result<()> {
    commands::generate::go::generate_go(
        input,
        output,
        module,
        lazuli_go_version,
        check,
        with_source,
        allow_drops,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_subcommand_routes() {
        let cli = Cli::try_parse_from(["lazuli", "check", "app.lzi"]).expect("parse");
        assert!(matches!(cli.command, Commands::Check { .. }));
    }

    #[test]
    fn doctor_subcommand_routes() {
        let cli = Cli::try_parse_from(["lazuli", "doctor", "."]).expect("parse");
        assert!(matches!(cli.command, Commands::Doctor { .. }));
    }

    #[test]
    fn new_subcommand_routes() {
        let cli = Cli::try_parse_from(["lazuli", "new", "my-app"]).expect("parse");
        assert!(matches!(cli.command, Commands::New { .. }));
    }

    #[test]
    fn dev_only_subcommand_is_not_on_the_published_surface() {
        // `parse` / `self-doctor` live on the `lazuli-dev` binary only.
        assert!(Cli::try_parse_from(["lazuli", "self-doctor", "."]).is_err());
    }
}
