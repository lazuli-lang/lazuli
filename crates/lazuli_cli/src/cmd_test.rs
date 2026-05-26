//! `lazuli test` orchestrator — drives per-layer runners, aggregates
//! results, applies `--fail-on` gates, renders output, and returns
//! the right exit code.
//!
//! This is the integration glue between:
//!   - `cmd_test_types` — canonical schema
//!   - `runners::{spec, go_test, playwright, ts_test, handler_coverage}`
//!   - `coverage_aggregator`
//!   - `cmd_test_output` (text/json)
//!   - `cmd_test_ndjson` (streaming)
//!   - `cmd_test_fail_fast`
//!   - `cmd_test_watch`

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::cmd_test_fail_fast::FailFastCoordinator;
use crate::cmd_test_ndjson::NdjsonEmitter;
use crate::cmd_test_output::{render_json, render_text};
use crate::cmd_test_types::{FailOnSpec, Layer, LayerResult, LayerVerdict, RunAccumulator};
use crate::cmd_test_watch::{DebounceBuffer, WatchDispatcher, spawn_watcher, watch_channel};
use crate::coverage_aggregator::{self, SpecTotals};
use crate::lazurite_manifest::{self, Manifest};
use crate::runners;

/// CLI surface — owned by `main.rs`'s `Commands::Test` variant.
#[derive(Debug, Clone, Default)]
pub struct TestOptions {
    pub input: PathBuf,
    pub layers: Vec<Layer>,
    pub format: OutputFormat,
    pub coverage: bool,
    pub fail_on: Vec<FailOnSpec>,
    pub watch: bool,
    pub fail_fast: bool,
    /// `--aggregate-method <method>` — when set, the coverage report
    /// emits an aggregate block. Without this, `--fail-on
    /// coverage:aggregate=N` is rejected per proposal §Coverage.
    pub aggregate_method: Option<String>,
    /// Pass-through args following `--` on the CLI. Currently honored
    /// only for the handler layer.
    pub extra_args: Vec<String>,
}

/// Closed catalog of `--format` values accepted by `lazuli test`.
///
/// `Text` is the default and renders a colorized human report.
/// `Json` is the snapshot shape consumed by tooling; `Ndjson` streams
/// `cmd_test_ndjson` events one per line as the orchestrator runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Colorized human report (default).
    #[default]
    Text,
    /// Single-shot JSON snapshot at the end of the run.
    Json,
    /// Streamed newline-delimited JSON events.
    Ndjson,
}

impl OutputFormat {
    /// Parse the textual `--format <value>` argument into a typed
    /// `OutputFormat`. Returns an `Err(String)` whose message names the
    /// closed catalog so the CLI surface can echo it directly.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "ndjson" => Ok(Self::Ndjson),
            other => Err(format!(
                "--format {other}: expected `text` | `json` | `ndjson`"
            )),
        }
    }
}

/// Entry point called by `main.rs`.
///
/// Routes to `run_watch` when `opts.watch` is set, otherwise calls
/// `run_once` and `std::process::exit`s with the returned exit code so
/// the rendered output already on stdout/stderr stays intact.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test::{run, TestOptions};
///
/// // run(TestOptions::default())?;
/// ```
pub fn run(opts: TestOptions) -> Result<()> {
    if opts.watch {
        return run_watch(opts);
    }
    let exit_code = run_once(&opts)?;
    if exit_code != 0 {
        // Surface as a non-zero process exit while keeping the
        // rendered output intact. anyhow!"…" without context preserves
        // the message the runner already wrote to stdout/stderr.
        std::process::exit(exit_code);
    }
    Ok(())
}

/// One full pass across layers. Returns the process exit code per
/// proposal §Exit code matrix.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test::{run_once, TestOptions};
///
/// // let code = run_once(&TestOptions::default())?;
/// ```
pub fn run_once(opts: &TestOptions) -> Result<i32> {
    let project_root = project_root_for(&opts.input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to load {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;

    let plan = resolve_plan(opts, manifest.as_ref(), &project_root);
    if plan.is_empty() {
        bail!("lazuli test: no layers selected (use --layer spec|view|handler|ts|e2e)");
    }

    let started = Instant::now();
    let mut accumulator = RunAccumulator {
        project_root: project_root.clone(),
        ..Default::default()
    };
    let mut ff = FailFastCoordinator::new(opts.fail_fast);
    let mut ndjson_emitter = if opts.format == OutputFormat::Ndjson {
        Some(NdjsonEmitter::new(std::io::stdout()))
    } else {
        None
    };

    for layer in &plan {
        if ff.should_skip() {
            // Emit synthetic skip layer so the report carries the
            // entire plan, not just the executed prefix.
            let skipped = LayerResult {
                layer: *layer,
                runner: runner_name(*layer).into(),
                result: LayerVerdict::Skip,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: None,
                duration_ms: 0,
                failures: vec![],
                runner_native_only: None,
                skip_reason: Some("skipped by --fail-fast".into()),
            };
            if let Some(em) = ndjson_emitter.as_mut() {
                em.runner_skip(*layer, runner_name(*layer), "fail-fast tripped");
            }
            accumulator.layer_results.push(skipped);
            continue;
        }

        if let Some(em) = ndjson_emitter.as_mut() {
            em.layer_start(*layer, runner_name(*layer), None);
        }
        let result = execute_layer(*layer, &opts, manifest.as_ref(), &project_root)?;
        if let Some(em) = ndjson_emitter.as_mut() {
            em.layer_complete(&result);
        }
        ff.observe(&result);
        accumulator.layer_results.push(result);
    }

    // Coverage.
    let mut coverage = None;
    if opts.coverage {
        let report = coverage_aggregator::build_coverage_report(
            &accumulator.layer_results,
            &project_root,
            spec_totals(&accumulator.layer_results),
            opts.aggregate_method.as_deref(),
        );
        coverage = Some(report);
    }

    // Apply --fail-on coverage gates.
    let mut coverage_errors: Vec<String> = Vec::new();
    if let Some(cov) = coverage.as_mut() {
        let coverage_specs: Vec<FailOnSpec> = opts
            .fail_on
            .iter()
            .filter(|s| matches!(s, FailOnSpec::Coverage { .. }))
            .cloned()
            .collect();
        if !coverage_specs.is_empty() {
            coverage_errors = coverage_aggregator::apply_fail_on(cov, &coverage_specs);
        }
        if let Some(em) = ndjson_emitter.as_mut() {
            for m in &cov.layers {
                em.coverage(m);
            }
        }
    }
    accumulator.coverage = coverage;

    let total_ms = started.elapsed().as_millis() as u64;
    let mut report = accumulator.finalize(total_ms);

    // If coverage blocks, force overall fail.
    if !coverage_errors.is_empty() {
        report.result = LayerVerdict::Fail;
        report.summary.overall = LayerVerdict::Fail;
    }

    // Render — NDJSON already streamed events; final summary closes
    // the stream.
    match opts.format {
        OutputFormat::Json => {
            render_json(&report, &mut std::io::stdout())?;
        }
        OutputFormat::Ndjson => {
            if let Some(em) = ndjson_emitter.as_mut() {
                em.summary(&report);
            }
        }
        OutputFormat::Text => {
            render_text(&report, &mut std::io::stdout())?;
            for err in &coverage_errors {
                eprintln!("coverage gate: {err}");
            }
        }
    }

    let exit_code = if matches!(report.result, LayerVerdict::Pass) {
        0
    } else {
        1
    };
    Ok(exit_code)
}

/// `--watch` mode. Runs one initial pass, then watches for changes
/// and re-runs the affected layer(s).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test::{run_watch, TestOptions};
///
/// // run_watch(TestOptions { watch: true, ..TestOptions::default() })?;
/// ```
pub fn run_watch(opts: TestOptions) -> Result<()> {
    let project_root = project_root_for(&opts.input);
    eprintln!("lazuli test --watch: watching {}", project_root.display());
    let _ = run_once(&opts);

    let (tx, rx) = watch_channel();
    let _watcher = spawn_watcher(&project_root, tx)?;

    // Capture by move; closures must not borrow `opts` so we clone.
    let opts_clone = opts.clone();
    let mut buf = DebounceBuffer::new();
    loop {
        let timeout = buf
            .next_tick()
            .unwrap_or_else(|| std::time::Duration::from_millis(1_000));
        match rx.recv_timeout(timeout) {
            Ok(event) => {
                eprintln!(
                    "lazuli test --watch: change in {} (layer={})",
                    event.path.display(),
                    event.layer.as_str()
                );
                buf.push(event);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        for (layer, _paths) in buf.drain_ready() {
            // Re-run only the affected layer; --fail-fast is honored
            // because we serialize layer execution.
            let mut narrowed = opts_clone.clone();
            narrowed.layers = vec![layer];
            let _ = run_once(&narrowed);
        }
    }
    Ok(())
}

fn execute_layer(
    layer: Layer,
    opts: &TestOptions,
    manifest: Option<&Manifest>,
    project_root: &Path,
) -> Result<LayerResult> {
    match layer {
        Layer::Spec => runners::spec::run(&opts.input, Layer::Spec),
        Layer::View => runners::spec::run(&opts.input, Layer::View),
        Layer::Handler => runners::go_test::run(
            manifest,
            project_root,
            runners::go_test::GoRunOptions {
                package_pattern: None,
                extra_args: opts.extra_args.clone(),
                coverage_override: if opts.coverage { Some(true) } else { None },
            },
        ),
        Layer::Ts => runners::ts_test::run(manifest, project_root),
        Layer::E2e => runners::playwright::run(manifest, project_root),
    }
}

fn runner_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Spec | Layer::View => "lazuli-doctor",
        Layer::Handler => "go-test",
        Layer::Ts => "ts",
        Layer::E2e => "playwright",
    }
}

fn project_root_for(input: &Path) -> PathBuf {
    if input.is_file() {
        input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        input.to_path_buf()
    }
}

fn resolve_plan(
    opts: &TestOptions,
    manifest: Option<&Manifest>,
    project_root: &Path,
) -> Vec<Layer> {
    if !opts.layers.is_empty() {
        return opts.layers.clone();
    }
    // Frente 1 — honor `[testing] default_layers` when present, or
    // fall back to the canonical default
    // `["handler_go", "view_extensibility"]` via the manifest
    // accessor so pilots can omit the field entirely.
    if let Some(m) = manifest {
        let layers = m.testing_default_layers();
        if !layers.is_empty() {
            return layers.iter().filter_map(|s| Layer::parse(s)).collect();
        }
    }
    // Conventional discovery — spec always; handler when manifest
    // declares [testing.go] or feature dir exists; ts when [testing.ts]
    // set; e2e when [testing.playwright] set OR `e2e/` exists.
    let mut plan = vec![Layer::Spec, Layer::View];
    let has_features_dir =
        project_root.join("app/features").is_dir() || project_root.join("features").is_dir();
    if has_features_dir
        || manifest
            .and_then(|m| m.testing.as_ref())
            .and_then(|t| t.go.as_ref())
            .is_some()
    {
        plan.push(Layer::Handler);
    }
    if manifest
        .and_then(|m| m.testing.as_ref())
        .and_then(|t| t.ts.as_ref())
        .is_some()
    {
        plan.push(Layer::Ts);
    }
    if project_root.join("e2e").is_dir()
        || manifest
            .and_then(|m| m.testing.as_ref())
            .and_then(|t| t.playwright.as_ref())
            .is_some()
    {
        plan.push(Layer::E2e);
    }
    plan
}

/// Hint extractor — the orchestrator hands the spec runner's
/// [`LayerResult.tests_run`] (count of constructs analyzed) to the
/// aggregator so it can synthesize a `spec_predicate` covered/total
/// pair. Until Wave 6.2 lands the per-rule numerator, we treat
/// `tests_passed` as covered and `tests_run` as total — a coarse but
/// honest first projection.
fn spec_totals(layers: &[LayerResult]) -> Option<SpecTotals> {
    let spec = layers.iter().find(|l| l.layer == Layer::Spec);
    let view = layers.iter().find(|l| l.layer == Layer::View);
    if spec.is_none() && view.is_none() {
        return None;
    }
    Some(SpecTotals {
        spec_covered: spec.map(|l| l.tests_passed as u64).unwrap_or(0),
        spec_total: spec.map(|l| l.tests_run as u64).unwrap_or(0),
        view_covered: view.map(|l| l.tests_passed as u64).unwrap_or(0),
        view_total: view.map(|l| l.tests_run as u64).unwrap_or(0),
    })
}

/// Dispatcher impl wiring the watch loop into the orchestrator.
/// Currently unused (run_watch inlines the call site) but kept for
/// integration tests that want to drive the watch loop manually.
pub struct LayerDispatcher {
    pub opts: TestOptions,
}

impl WatchDispatcher for LayerDispatcher {
    fn dispatch(&mut self, layer: Layer, _paths: &[PathBuf]) -> Result<()> {
        let mut narrowed = self.opts.clone();
        narrowed.layers = vec![layer];
        let _ = run_once(&narrowed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_format() {
        assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::parse("ndjson").unwrap(), OutputFormat::Ndjson);
        assert!(OutputFormat::parse("yaml").is_err());
    }

    #[test]
    fn project_root_for_file_uses_parent() {
        // Use a real tempfile path so `is_file()` returns true; the
        // string-only "/x/y/app.lzi" path does not exist on disk so
        // the fallback branch (return path as-is) kicks in.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.lzi");
        std::fs::write(&file, "").unwrap();
        let p = project_root_for(&file);
        assert_eq!(p, dir.path().to_path_buf());
    }

    #[test]
    fn project_root_for_dir_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let p = project_root_for(dir.path());
        assert_eq!(p, dir.path().to_path_buf());
    }

    #[test]
    fn resolve_plan_honors_explicit_layers() {
        let opts = TestOptions {
            layers: vec![Layer::Handler, Layer::Ts],
            ..Default::default()
        };
        let plan = resolve_plan(&opts, None, Path::new("."));
        assert_eq!(plan, vec![Layer::Handler, Layer::Ts]);
    }

    /// Ensure layers_for_path is reachable via the public re-export from
    /// the watch module; smoke for the watch hook wiring.
    #[test]
    fn watch_classifier_reachable() {
        let layers =
            crate::cmd_test_watch::layers_for_path(Path::new("/proj/app.lzi"), Path::new("/proj"));
        assert!(layers.contains(&Layer::Spec));
    }
}
