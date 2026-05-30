//! `lazuli doctor` CLI entry layer.
//!
//! The package-wide orchestration engine was lifted into the
//! `lazuli_doctor_run` crate (Wave D3) so the LSP can run the same engine
//! in-editor. What stays here is the **CLI-only** layer:
//!
//! - the `doctor_command` / `doctor_command_with_options` clap entry
//!   points (version-pin enforcement, `--check-release`, `--self`,
//!   text/JSON rendering, `--coverage` summary, `--fail-on` gate);
//! - the **boundary injection** that keeps the engine free of
//!   `lazuli_lsp` and the TS-codegen subsystem:
//!   - file-local "shape" diagnostics computed via
//!     `lazuli_lsp::diagnostics_for_source_with_profile` and fed into
//!     [`lazuli_doctor_run::run_package`] through the [`FileLocalInjector`];
//!   - the package-level `SCHEMA-RICH-001` finding, computed here (it
//!     needs `crate::build_module_from_path` + the zod-slot codegen
//!     helpers) and handed in as `run_package`'s `injected` argument;
//! - `--check-release` (`release`), `--self` (`self_audit`), the
//!   `DoctorReport` JSON assembly (`report_build`), the runtime-options
//!   bag (`runtime_options`), and the `schema_rich_001` rule itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use lazuli_doctor_run::{DoctorDiagnostic, DoctorSeverity, run_package};
use tower_lsp::lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString};

mod release;
mod report_build;
mod runtime_options;
pub mod schema_rich_001;
mod self_audit;

// `folder::vocab_client_src_001` is reached by the frontend scaffold
// invariants test via `crate::doctor::folder::…`; re-export the engine's
// module so that path keeps resolving. Only the (cfg(test)) scaffold test
// consumes it, hence the allow on non-test builds.
#[allow(unused_imports)]
pub use lazuli_doctor_run::folder;
use report_build::build_doctor_report;
pub(crate) use report_build::doctor_diagnostics_json;
pub use runtime_options::DoctorRuntimeOptions;

/// Convert a `tower_lsp` `Diagnostic` (the wire shape the LSP file-local
/// pass produces) into the engine's [`DoctorDiagnostic`] envelope.
///
/// This adapter lives on the CLI side so `lazuli_doctor_run` stays free
/// of any `tower_lsp` dependency — the engine accepts pre-converted
/// `DoctorDiagnostic` rows through its file-local injector.
fn doctor_diagnostic_from_lsp(path: PathBuf, diagnostic: &LspDiagnostic) -> DoctorDiagnostic {
    let severity = match diagnostic.severity {
        Some(DiagnosticSeverity::ERROR) => DoctorSeverity::Error,
        Some(DiagnosticSeverity::WARNING) => DoctorSeverity::Warning,
        Some(DiagnosticSeverity::INFORMATION) => DoctorSeverity::Info,
        Some(DiagnosticSeverity::HINT) => DoctorSeverity::Hint,
        _ => DoctorSeverity::Warning,
    };
    let code = diagnostic
        .code
        .as_ref()
        .map(|code| match code {
            NumberOrString::String(value) => value.clone(),
            NumberOrString::Number(value) => value.to_string(),
        })
        .unwrap_or_else(|| "diagnostic".to_owned());

    DoctorDiagnostic {
        path,
        line: diagnostic.range.start.line as usize + 1,
        column: diagnostic.range.start.character as usize + 1,
        severity,
        code,
        message: diagnostic.message.clone(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

/// Build the file-local diagnostic injector the CLI hands to
/// [`run_package`]: for each `.lzi` / `.lzx` source it runs the LSP's
/// `diagnostics_for_source_with_profile` pass at the requested profile
/// and lifts every row into a [`DoctorDiagnostic`]. This is the exact
/// per-file work the engine used to do inline before the D3 lift.
fn cli_file_local(
    path: &Path,
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    lazuli_lsp::diagnostics_for_source_with_profile(source, security_profile)
        .iter()
        .map(|diagnostic| doctor_diagnostic_from_lsp(path.to_path_buf(), diagnostic))
        .collect()
}

/// Compute the package-level `SCHEMA-RICH-001` findings the CLI injects
/// into the engine. This rule walks generated `dist/ts-*/*.zod.ts` and
/// reaches into the TS-codegen module loader (`crate::build_module_from_path`
/// + the zod-slot helpers), which the engine deliberately does not depend
/// on — so the CLI computes it and hands it in.
fn schema_rich_injected(project_root: &Path) -> Vec<DoctorDiagnostic> {
    schema_rich_001::check(project_root)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            path: doctor_rule_path(project_root, finding.path),
            line: finding.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: schema_rich_001::Finding::CODE.to_owned(),
            message: finding.message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

/// Strip the project root prefix off a rule-emitted path, byte-for-byte
/// identical to the engine's own `doctor_rule_path` (which renders
/// findings relative to the project root). `SCHEMA-RICH-001` paths are
/// absolute `dist/ts-*/*.zod.ts` paths under the project root, so this
/// renders them as the same relative location the inline call produced.
fn doctor_rule_path(project_root: &Path, path: PathBuf) -> PathBuf {
    path.strip_prefix(project_root)
        .unwrap_or(&path)
        .to_path_buf()
}

/// Run the doctor package engine with the CLI's boundary injection
/// (file-local via `lazuli_lsp`, package-level `SCHEMA-RICH-001` via the
/// TS-codegen helpers) and return the loaded [`DoctorPackage`].
///
/// Shared by the `lazuli doctor` text/JSON entry and the MCP
/// `doctor_diagnostics_json` surface so both produce the identical merged
/// stream.
pub(crate) fn run_package_cli(
    input: &Path,
    security_profile: SecurityProfile,
) -> Result<lazuli_doctor_run::DoctorPackage> {
    let project_root = lazuli_doctor_run::entry_support::doctor_project_root(input);
    let file_local = |path: &Path, source: &str| cli_file_local(path, source, security_profile);
    let injected = schema_rich_injected(&project_root);
    // v2 — build the engine's severity `ResolvedDoctorConfig` from the
    // on-disk `Lazurite.toml` + the `--security-profile` flag, exactly the
    // inputs the engine used to read internally. `from_doctor` takes the
    // profile from the flag (NOT `[doctor] profile`), matching today's CLI
    // behavior, so the config VALUE — and thus the diagnostic stream — is
    // byte-identical to before. The engine still loads the manifest for its
    // non-severity uses; this only moves WHO builds the severity config.
    let manifest = lazuli_manifest::lazurite_manifest::load(&project_root)
        .ok()
        .flatten();
    let config = lazuli_doctor_config::ResolvedDoctorConfig::from_doctor(
        manifest.as_ref().and_then(|m| m.doctor.as_ref()),
        security_profile,
    );
    run_package(input, &config, &file_local, injected)
}

/// Handler for `lazuli doctor` — runs the full diagnostic surface
/// with the default runtime options.
pub(crate) fn doctor_command(
    input: &Path,
    security_profile: Option<SecurityProfile>,
    check_release: bool,
    allow_version_mismatch: bool,
) -> Result<()> {
    doctor_command_with_options(
        input,
        security_profile,
        check_release,
        allow_version_mismatch,
        DoctorRuntimeOptions::default(),
    )
}

/// Run `lazuli doctor` with a fully-specified [`DoctorRuntimeOptions`]
/// bag. The load-bearing entry point — every clap-side flag translates
/// into a field of `opts`.
pub(crate) fn doctor_command_with_options(
    input: &Path,
    security_profile: Option<SecurityProfile>,
    check_release: bool,
    allow_version_mismatch: bool,
    opts: DoctorRuntimeOptions,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = lazuli_doctor_run::entry_support::doctor_project_root(input);
        let manifest =
            lazuli_manifest::lazurite_manifest::load(&project_root).with_context(|| {
                format!(
                    "failed to load {}",
                    project_root.join("Lazurite.toml").display()
                )
            })?;
        crate::version::enforce_manifest_pin(manifest.as_ref())?;
    }

    // Precedence: explicit `--security-profile` flag > toml `[doctor]
    // profile` > default strict. When no flag was given (`None`), defer to
    // the manifest's declared profile (the wart fix — previously the flag's
    // clap default always won, masking `[doctor] profile`). This keeps the
    // CLI consistent with the LSP, which already honors `[doctor] profile`
    // via `ResolvedDoctorConfig::resolve_reading_profile`.
    let security_profile = security_profile.unwrap_or_else(|| {
        let project_root = lazuli_doctor_run::entry_support::doctor_project_root(input);
        lazuli_manifest::lazurite_manifest::load(&project_root)
            .ok()
            .flatten()
            .and_then(|m| m.doctor)
            .and_then(|d| d.profile)
            .as_deref()
            .and_then(SecurityProfile::parse)
            .unwrap_or(SecurityProfile::Strict)
    });

    if check_release {
        return release::doctor_release_command(input);
    }

    // W3 — `--self` short-circuits the standard IR pipeline and walks
    // the framework's Rust source instead.
    if opts.self_audit {
        return self_audit::doctor_self_command(input, &opts);
    }

    let package = run_package_cli(input, security_profile)?;
    let diagnostics = package.diagnostics();

    // Wave 2 — JSON output surface. When `--format json` (or ndjson) is
    // requested, emit the canonical `DoctorReport` schema and short-circuit.
    let format = parse_doctor_format(opts.format.as_deref());
    let want_json = matches!(
        format,
        crate::doctor_report::DoctorFormat::Json | crate::doctor_report::DoctorFormat::Ndjson
    );

    if want_json {
        let report = build_doctor_report(&diagnostics, opts.coverage, &package);
        let payload = serde_json::to_string_pretty(&report)
            .context("failed to serialize DoctorReport JSON")?;
        println!("{payload}");
        // Wave 2.2 — fail-on gate
        let specs =
            parse_fail_on_specs(&opts.fail_on).map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
        if crate::doctor_report::report_fails_gate(&report, &specs)
            || diagnostics
                .iter()
                .any(|d| d.severity == DoctorSeverity::Error)
        {
            bail!("{} failed Lazuli doctor checks", input.display());
        }
        return Ok(());
    }

    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if opts.coverage {
        // Wave 6 — emit per-layer coverage summary at end of text output.
        let report = build_doctor_report(&diagnostics, true, &package);
        if let Some(cov) = &report.coverage {
            println!("\nCoverage report (schema_version {}):", cov.schema_version);
            for (layer, info) in &cov.layers {
                println!(
                    "  {:<24} {:>6.1}%  {:<8} ({}/{})",
                    layer, info.pct, info.verdict, info.covered, info.total
                );
            }
            println!(
                "  gate: {} (below_block={:?}, below_warn={:?})",
                cov.gate_result.verdict, cov.gate_result.below_block, cov.gate_result.below_warn
            );
        }
    }

    // Wave 2.2 — text-mode fail-on still gates
    let specs =
        parse_fail_on_specs(&opts.fail_on).map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
    if !specs.is_empty() {
        let report = build_doctor_report(&diagnostics, opts.coverage, &package);
        if crate::doctor_report::report_fails_gate(&report, &specs) {
            bail!(
                "{} failed Lazuli doctor checks (--fail-on gate)",
                input.display()
            );
        }
    }

    if has_error {
        bail!("{} failed Lazuli doctor checks", input.display());
    }

    println!("{} passed Lazuli doctor checks", input.display());
    Ok(())
}

/// Translate the wire-level `--format` string into a typed
/// `DoctorFormat`. Unknown values fall back to `Text`.
fn parse_doctor_format(input: Option<&str>) -> crate::doctor_report::DoctorFormat {
    use crate::doctor_report::DoctorFormat;
    match input.unwrap_or("text").to_ascii_lowercase().as_str() {
        "text" => DoctorFormat::Text,
        "json" => DoctorFormat::Json,
        "ndjson" => DoctorFormat::Ndjson,
        "auto" => DoctorFormat::Auto,
        _ => DoctorFormat::Text,
    }
}

/// Parse every `--fail-on <spec>` flag into a `FailOnSpec`.
fn parse_fail_on_specs(
    inputs: &[String],
) -> std::result::Result<Vec<crate::doctor_report::FailOnSpec>, String> {
    inputs
        .iter()
        .map(|s| crate::doctor_report::FailOnSpec::parse(s))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// Regression for the R1.C real-world sweep — the LSP fires both
    /// `app-env-contract` and `env-schema-contract` on the same line of a
    /// `registry.env` block when the env declaration shape is invalid; the
    /// doctor layer dedupes them in favor of `env-schema-contract`. This
    /// bridges the CLI's `lazuli_lsp`-backed file-local injector into the
    /// engine's dedupe, so it lives here (not in `lazuli_doctor_run`, which
    /// must not depend on `lazuli_lsp`). Moved from
    /// `lazuli_cli::doctor::tests::manifest_plugin` in the D3 lift.
    #[test]
    fn doctor_cli_env_dedupe_keeps_env_schema_contract() {
        let source = "registry\n  env\n    group storage\n      server S3_ENDPOINT: Text\n";

        // The LSP file-local pass (the CLI's injector) is intentionally
        // noisy: it emits BOTH codes on the offending line.
        let lsp_codes: Vec<String> =
            lazuli_lsp::diagnostics_for_source_with_profile(source, SecurityProfile::Strict)
                .iter()
                .map(|d| {
                    doctor_diagnostic_from_lsp(std::path::PathBuf::from("registry.lzi"), d).code
                })
                .collect();
        assert!(
            lsp_codes.iter().any(|c| c == "app-env-contract")
                && lsp_codes.iter().any(|c| c == "env-schema-contract"),
            "LSP should still emit both codes (dedupe lives in doctor); got: {lsp_codes:?}"
        );

        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-env-dedupe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp project");
        let target = root.join("registry.lzi");
        fs::write(&target, source).expect("write registry.lzi");

        let diagnostics = run_package_cli(&target, SecurityProfile::Strict)
            .expect("load registry.lzi package")
            .diagnostics();
        let _ = fs::remove_dir_all(&root);

        let env_line_codes: Vec<&str> = diagnostics
            .iter()
            .filter(|d| {
                d.line == 4 && (d.code == "app-env-contract" || d.code == "env-schema-contract")
            })
            .map(|d| d.code.as_str())
            .collect();

        assert_eq!(
            env_line_codes.len(),
            1,
            "exactly one of app-env-contract / env-schema-contract should survive dedupe; got: {env_line_codes:?}"
        );
        assert_eq!(
            env_line_codes[0], "env-schema-contract",
            "env-schema-contract should win the dedupe (registry-scoped owner)"
        );
    }
}
