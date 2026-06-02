//! `lazuli generate <kind>` — the codegen entry point. Walks the
//! IR-lifted project and emits the artifact for the chosen
//! `GenerateKind`: OpenAPI YAML, Lazuli Go (multi-file), TypeScript SDK
//! + hooks + Zod, Playwright fixtures, or one of the Wave 3 scaffold
//! kinds (feature, command, view, rule, transition, handler).
//!
//! This module is the dispatcher only. Each `GenerateKind` has a
//! sibling submodule that owns the actual emit:
//!
//! - `generate::go` — Lazuli Go user-code (the runtime target).
//! - `generate::ts` — TypeScript SDK / hooks / Zod / design tokens.
//! - `generate::openapi` — OpenAPI YAML.
//!
//! The Wave 3 scaffold kinds (`feature`, `command`, `view`, `rule`,
//! `transition`, `handler`, `playwright`) keep their existing
//! `cmd_generate_*::run` modules — the dispatcher simply forwards.
//!
//! The handler enforces the `Lazurite.toml [lazuli]` version pin
//! unless `--allow-version-mismatch` was passed at the top level. This
//! keeps `generate` honest: codegen never silently emits artifacts
//! against a mismatched compiler/runtime version.
//!
//! Cross-refs:
//! - `crate::version::enforce_manifest_pin` — the pin gate.
//! - `crate::GenerateKind` — the closed catalog of emit kinds.
//! - `commands/generate/go.rs` (TBD), `commands/generate/ts.rs`
//!   (TBD), `commands/generate/openapi.rs`.

pub mod go;
pub mod openapi;
pub mod ts;

use std::path::Path;

use anyhow::{Context, Result, bail};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::{
    GenerateKind, PlaywrightTarget, cmd_generate_command, cmd_generate_feature,
    cmd_generate_handler, cmd_generate_playwright, cmd_generate_rule, cmd_generate_transition,
    cmd_generate_view, doctor, lazurite_manifest, project_root_for_input, version,
};

/// Handler for the `Commands::Generate` clap arm.
///
/// Enforces the manifest pin (unless `allow_version_mismatch`) and
/// dispatches to the sibling submodule (`go`, `ts`, `openapi`) or one
/// of the Wave 3 scaffold runners by `kind`. Scaffold kinds receive
/// only their ident and the project root; codegen kinds receive the
/// full flag bag.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::generate::generate_command;
/// use lazuli_cli::GenerateKind;
///
/// // generate_command(GenerateKind::Go, Path::new("."), None, None, None, None,
/// //                  false, false, false, true, None)?;
/// ```
#[allow(clippy::too_many_arguments)]
pub fn generate_command(
    kind: GenerateKind,
    input: &Path,
    output: Option<&Path>,
    api_version: Option<&str>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
    allow_version_mismatch: bool,
    no_gate: bool,
    playwright_target: Option<PlaywrightTarget>,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = project_root_for_input(input);
        let manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to read {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;
        version::enforce_manifest_pin(manifest.as_ref())?;
    }

    // W0-4 (META-THEME) — the blocking doctor gate. For the artifact-emit
    // kinds (`go`, `ts`, `openapi`) run the full Correctness + Error
    // package doctor pass and REFUSE TO EMIT on any ERROR-severity
    // finding, so a broken tree never ships at exit 0. The Wave-3 scaffold
    // kinds (`feature`/`command`/`view`/…) only rewrite a single authored
    // `.lzi`/`.lzx` block and never touch the runtime tree, so they are
    // not gated here. `--check` (dry-run) still runs the gate: the whole
    // point of a dry run is to learn whether a real run would emit.
    if matches!(
        kind,
        GenerateKind::Go | GenerateKind::Ts | GenerateKind::Openapi
    ) {
        run_emit_gate(input, allow_version_mismatch, no_gate)?;
    }

    match kind {
        GenerateKind::Openapi => openapi::generate_openapi(input, output, api_version),
        GenerateKind::Go => go::generate_go(
            input,
            output,
            module,
            lazuli_go_version,
            check,
            with_source,
            allow_drops,
        ),
        GenerateKind::Feature => {
            reject_generate_feature_options(
                output,
                api_version,
                module,
                lazuli_go_version,
                check,
                with_source,
            )?;
            let name = input.to_str().context("feature name must be valid UTF-8")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_feature::run(name, &project_root)
        }
        GenerateKind::Handler => {
            let ident = input
                .to_str()
                .context("handler ident must be valid UTF-8")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_handler::run(ident, &project_root)
        }
        GenerateKind::Playwright => {
            let target = playwright_target
                .context("--playwright-target is required when kind=playwright")?;
            cmd_generate_playwright::run(input, target)
        }
        GenerateKind::Ts => ts::generate_ts(input, output, check),
        // Wave 3 — TDD/BDD-first scaffold generators. Each takes a
        // `<feature>.<name>` ident; the cmd_generate_*::run function
        // resolves the feature root from the manifest and appends the
        // construct block with @TODO authored: markers.
        GenerateKind::Command => {
            let ident = input
                .to_str()
                .context("command ident must be valid UTF-8 in `<feature>.<name>` form")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_command::run(ident, &project_root)
        }
        GenerateKind::View => {
            let ident = input
                .to_str()
                .context("view ident must be valid UTF-8 in `<feature>.<name>` form")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_view::run(ident, &project_root)
        }
        GenerateKind::Rule => {
            let ident = input
                .to_str()
                .context("rule ident must be valid UTF-8 in `<feature>.<name>` form")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_rule::run(ident, &project_root)
        }
        GenerateKind::Transition => {
            let ident = input.to_str().context(
                "transition ident must be valid UTF-8 in `<feature>.<workflow>.<name>` form",
            )?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_transition::run(ident, &project_root)
        }
    }
}

/// Resolve the doctor profile for the gate and run it ahead of emit.
///
/// Profile precedence mirrors `lazuli doctor` with no `--security-profile`
/// flag: manifest `[doctor] profile` > `strict`. (Generate has no
/// `--security-profile` flag of its own, so there is no flag tier here.)
/// The gate honors `--allow-version-mismatch` (drops `LAZULI-VERSION-*`)
/// and `--no-gate` / `LAZULI_NO_GATE=1` (loud bypass).
fn run_emit_gate(input: &Path, allow_version_mismatch: bool, no_gate: bool) -> Result<()> {
    let project_root = project_root_for_input(input);
    let security_profile = lazurite_manifest::load(&project_root)
        .ok()
        .flatten()
        .and_then(|m| m.doctor)
        .and_then(|d| d.profile)
        .as_deref()
        .and_then(SecurityProfile::parse)
        .unwrap_or(SecurityProfile::Strict);
    doctor::gate::run_generate_gate(
        input,
        security_profile,
        doctor::gate::GateBypass {
            no_gate,
            allow_version_mismatch,
        },
    )
}

/// Reject codegen-style flags on the `lazuli generate feature` arm —
/// scaffold kinds take only `<feature>.<name>` and produce a
/// deterministic file rewrite; `--out`, `--api-version`, `--module`,
/// `--check`, `--with-source` are all artifact-emitter flags and have
/// no meaning here. Pulled out into a helper so the typed signature
/// stays the discriminator without padding the dispatch arms with
/// repeated guards.
fn reject_generate_feature_options(
    output: Option<&Path>,
    api_version: Option<&str>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
) -> Result<()> {
    if output.is_some()
        || api_version.is_some()
        || module.is_some()
        || lazuli_go_version.is_some()
        || check
        || with_source
    {
        bail!(
            "`lazuli generate feature <name>` does not accept codegen flags like --out, --api-version, --module, --check, or --with-source"
        );
    }
    Ok(())
}
