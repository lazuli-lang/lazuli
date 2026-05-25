//! `lazuli test` — unified test runner across spec / view / handler /
//! ts / e2e layers.
//!
//! Wire-thin orchestrator: shells out to each layer's native runner
//! (`go test`, Playwright, Vitest/Jest) rather than reimplementing
//! execution. Parses the `--fail-on` composable threshold flag,
//! validates that `--aggregate-method` is provided whenever a
//! `coverage:aggregate=<N>` gate is requested, and delegates to
//! `crate::cmd_test::run`.
//!
//! Cross-refs:
//! - `crate::cmd_test::run` — the orchestrator.
//! - `crate::cmd_test_types::{Layer, FailOnSpec}` — typed forms of the
//!   `--layer` and `--fail-on` CLI surfaces.
//! - `docs/proposals/lazuli-test-runner-2026-05-24.md` — design.
//!
//! The `Layer` conversion from the clap-side `TestLayerFlag` happens at
//! the dispatch site in `main.rs`, so this function takes the typed
//! `Vec<Layer>` directly — keeping `cmd_test_types` free of any
//! `clap` dependency.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::{cmd_test, cmd_test_types};

/// Handler for the `Commands::Test` clap arm.
pub fn test_command(
    input: PathBuf,
    layers: Vec<cmd_test_types::Layer>,
    format: String,
    coverage: bool,
    fail_on: Vec<String>,
    watch: bool,
    fail_fast: bool,
    aggregate_method: Option<String>,
    extra_args: Vec<String>,
) -> Result<()> {
    let format = cmd_test::OutputFormat::parse(&format).map_err(|e| anyhow::anyhow!(e))?;
    let mut parsed_fail_on = Vec::new();
    for raw in &fail_on {
        let spec = cmd_test_types::FailOnSpec::parse(raw).map_err(|e| anyhow::anyhow!(e))?;
        parsed_fail_on.push(spec);
    }
    // Gate coverage:aggregate=<N> behind --aggregate-method per
    // proposal §Coverage.
    if aggregate_method.is_none()
        && parsed_fail_on.iter().any(|s| {
            matches!(s, cmd_test_types::FailOnSpec::Coverage { metric, .. } if metric == "aggregate")
        })
    {
        bail!(
            "--fail-on coverage:aggregate=<N> requires --aggregate-method to be set explicitly"
        );
    }

    let opts = cmd_test::TestOptions {
        input,
        layers,
        format,
        coverage,
        fail_on: parsed_fail_on,
        watch,
        fail_fast,
        aggregate_method,
        extra_args,
    };
    cmd_test::run(opts)
}
