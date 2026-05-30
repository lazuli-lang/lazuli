//! T3 — Layer 4 (ts) runner. Shells the configured TS test runner
//! (`vitest` or `jest`) via `npx`, captures the runner-native JSON
//! report, normalizes into the unified [`LayerResult`] schema.
//!
//! Closed catalog: `vitest` and `jest`. Adding a runner requires an
//! explicit enum extension and a parser. Per the proposal §T3 we do
//! not pretend to support arbitrary JS test runners.
//!
//! Pre-flight: `npx <runner> --version`. Missing → `LayerVerdict::Skip`.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::Result;
use serde::Deserialize;

use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict, TestFailure};
use crate::lazurite_manifest::{Manifest, TestingTs};

/// Closed catalog of supported TypeScript test runners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsRunner {
    /// Vitest (canonical Lazurite scaffold default).
    Vitest,
    /// Jest (legacy, opt-in via `[testing.ts] runner = "jest"`).
    Jest,
}

impl TsRunner {
    /// Parse `[testing.ts] runner` into the typed enum; errors carry
    /// the closed catalog name list verbatim.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::runners::ts_test::TsRunner;
    /// assert_eq!(TsRunner::parse("vitest"), Ok(TsRunner::Vitest));
    /// ```
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "vitest" => Ok(TsRunner::Vitest),
            "jest" => Ok(TsRunner::Jest),
            other => Err(format!(
                "[testing.ts].runner = `{other}` is not supported (closed catalog: vitest | jest)"
            )),
        }
    }

    /// Stable lowercase identifier — round-trips through `parse`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::runners::ts_test::TsRunner;
    /// assert_eq!(TsRunner::Vitest.as_str(), "vitest");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            TsRunner::Vitest => "vitest",
            TsRunner::Jest => "jest",
        }
    }
}

/// Probe `npx <runner> --version`. Returns `None` when the runner is
/// not installed under the project's `node_modules`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::ts_test::{probe, TsRunner};
/// // let version = probe(TsRunner::Vitest);
/// ```
pub fn probe(runner: TsRunner) -> Option<String> {
    let output = Command::new("npx")
        .arg(runner.as_str())
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Execute the resolved TS runner (Vitest or Jest) under
/// `[testing.ts]`. Returns a `LayerVerdict::Skip` when neither the
/// manifest nor the canonical layout produces a config and discovery
/// root.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::runners::ts_test::run;
/// // let result = run(manifest.as_ref(), Path::new("."))?;
/// ```
pub fn run(manifest: Option<&Manifest>, project_root: &Path) -> Result<LayerResult> {
    let started = Instant::now();
    // Frente 1 — resolve effective `[testing.ts]` honoring authored
    // overrides + canonical layout-derived defaults. Returns None only
    // when the project is neither in the canonical layout nor declares
    // the block (back-compat skip path).
    let Some(cfg): Option<TestingTs> = manifest.and_then(|m| m.testing_ts_resolved(project_root))
    else {
        return Ok(LayerResult {
            layer: Layer::Ts,
            runner: "ts".into(),
            result: LayerVerdict::Skip,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            issues: 0,
            exit_code: None,
            command: None,
            duration_ms: started.elapsed().as_millis() as u64,
            failures: Vec::new(),
            runner_native_only: None,
            skip_reason: Some(
                "[testing.ts] not configured and project layout is not canonical (app/web/ or app/clients/<name>/)".into(),
            ),
        });
    };

    let runner = match TsRunner::parse(&cfg.runner) {
        Ok(r) => r,
        Err(err) => {
            return Ok(LayerResult {
                layer: Layer::Ts,
                runner: cfg.runner.clone(),
                result: LayerVerdict::Skip,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: None,
                duration_ms: started.elapsed().as_millis() as u64,
                failures: Vec::new(),
                runner_native_only: None,
                skip_reason: Some(err),
            });
        }
    };

    if probe(runner).is_none() {
        return Ok(LayerResult {
            layer: Layer::Ts,
            runner: runner.as_str().into(),
            result: LayerVerdict::Skip,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            issues: 0,
            exit_code: None,
            command: Some(format!("npx {} --version", runner.as_str())),
            duration_ms: started.elapsed().as_millis() as u64,
            failures: Vec::new(),
            runner_native_only: None,
            skip_reason: Some(format!(
                "`npx {}` not available; install the project's dev dependencies",
                runner.as_str()
            )),
        });
    }

    let mut cmd = Command::new("npx");
    cmd.arg(runner.as_str());

    // Runner-specific argument construction.
    match runner {
        TsRunner::Vitest => {
            // Vitest defaults to watch mode; --run forces single pass.
            // The proposal explicitly bakes this in to the manifest
            // default but we add it defensively when missing.
            if !cfg.flags.iter().any(|f| f == "--run") {
                cmd.arg("--run");
            }
            cmd.arg("--reporter=json");
            if let Some(c) = &cfg.config {
                cmd.arg(format!("--config={c}"));
            }
            if cfg.coverage {
                cmd.arg("--coverage");
                cmd.arg("--coverage.reporter=json-summary");
            }
            for flag in &cfg.flags {
                cmd.arg(flag);
            }
            if let Some(root) = &cfg.discovery_root {
                cmd.arg(root);
            }
        }
        TsRunner::Jest => {
            cmd.arg("--json");
            if let Some(c) = &cfg.config {
                cmd.arg(format!("--config={c}"));
            }
            if cfg.coverage {
                cmd.arg("--coverage");
                cmd.arg("--coverageReporters=json-summary");
            }
            for flag in &cfg.flags {
                cmd.arg(flag);
            }
            if let Some(root) = &cfg.discovery_root {
                cmd.arg(format!("--roots={root}"));
            }
        }
    }
    cmd.current_dir(project_root);

    let command_pretty = format!("npx {} {}", runner.as_str(), flag_summary(runner, &cfg));

    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Ok(LayerResult {
                layer: Layer::Ts,
                runner: runner.as_str().into(),
                result: LayerVerdict::Skip,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: Some(command_pretty),
                duration_ms: started.elapsed().as_millis() as u64,
                failures: Vec::new(),
                runner_native_only: None,
                skip_reason: Some(format!("failed to spawn {}: {err}", runner.as_str())),
            });
        }
    };

    let parsed = match runner {
        TsRunner::Vitest => parse_vitest_json(&output.stdout),
        TsRunner::Jest => parse_jest_json(&output.stdout),
    };
    let exit_code = output.status.code();
    let verdict = if exit_code == Some(0) && parsed.tests_failed == 0 {
        LayerVerdict::Pass
    } else {
        LayerVerdict::Fail
    };

    let coverage_artifact = if cfg.coverage {
        Some(
            project_root
                .join("coverage/coverage-summary.json")
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };

    Ok(LayerResult {
        layer: Layer::Ts,
        runner: runner.as_str().into(),
        result: verdict,
        tests_run: parsed.tests_run,
        tests_passed: parsed.tests_passed,
        tests_failed: parsed.tests_failed,
        issues: 0,
        exit_code,
        command: Some(command_pretty),
        duration_ms: started.elapsed().as_millis() as u64,
        failures: parsed.failures,
        runner_native_only: coverage_artifact,
        skip_reason: None,
    })
}

fn flag_summary(runner: TsRunner, cfg: &TestingTs) -> String {
    let mut parts: Vec<String> = match runner {
        TsRunner::Vitest => vec!["--run".into(), "--reporter=json".into()],
        TsRunner::Jest => vec!["--json".into()],
    };
    if cfg.coverage {
        parts.push("--coverage".into());
    }
    if let Some(c) = &cfg.config {
        parts.push(format!("--config={c}"));
    }
    parts.extend(cfg.flags.iter().cloned());
    if let Some(root) = &cfg.discovery_root {
        parts.push(root.clone());
    }
    parts.join(" ")
}

#[derive(Debug, Default)]
struct ParsedRun {
    tests_run: u32,
    tests_passed: u32,
    tests_failed: u32,
    failures: Vec<TestFailure>,
}

// --- Vitest JSON parser ---------------------------------------------
//
// Vitest's `--reporter=json` emits a JSON object that loosely matches
// Jest's schema: `testResults: [{name, assertionResults: [{title,
// status, failureMessages}]}], numPassedTests, numFailedTests`.
// We parse defensively — both runners share enough shape that one
// struct works.

#[derive(Debug, Deserialize)]
struct JsonReport {
    #[serde(default, rename = "numTotalTests")]
    num_total: u32,
    #[serde(default, rename = "numPassedTests")]
    num_passed: u32,
    #[serde(default, rename = "numFailedTests")]
    num_failed: u32,
    #[serde(default, rename = "testResults")]
    test_results: Vec<JsonTestFile>,
}

#[derive(Debug, Deserialize)]
struct JsonTestFile {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "assertionResults")]
    assertions: Vec<JsonAssertion>,
}

#[derive(Debug, Deserialize)]
struct JsonAssertion {
    #[serde(default)]
    title: String,
    #[serde(default, rename = "fullName")]
    full_name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default, rename = "failureMessages")]
    failure_messages: Vec<String>,
}

/// Parse Vitest's JSON reporter output. Shape is jest-compatible
/// today; the dedicated entry point lets us swap if Vitest diverges.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::ts_test::parse_vitest_json;
/// let parsed = parse_vitest_json(b"{}");
/// ```
pub fn parse_vitest_json(stdout: &[u8]) -> ParsedRun {
    parse_jest_style(stdout)
}

/// Parse Jest's JSON reporter output.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::ts_test::parse_jest_json;
/// let parsed = parse_jest_json(b"{}");
/// ```
pub fn parse_jest_json(stdout: &[u8]) -> ParsedRun {
    parse_jest_style(stdout)
}

fn parse_jest_style(stdout: &[u8]) -> ParsedRun {
    // Strip any trailing non-JSON noise (some runners print TTY
    // sequences before the JSON blob); we locate the last `}` and
    // slice up to it inclusive when the raw parse fails.
    let report: Option<JsonReport> = serde_json::from_slice(stdout).ok().or_else(|| {
        let text = std::str::from_utf8(stdout).ok()?;
        let start = text.find('{')?;
        let end = text.rfind('}')?;
        if start >= end {
            return None;
        }
        serde_json::from_str(&text[start..=end]).ok()
    });
    let Some(report) = report else {
        return ParsedRun::default();
    };

    let mut parsed = ParsedRun {
        tests_run: report.num_total,
        tests_passed: report.num_passed,
        tests_failed: report.num_failed,
        failures: Vec::new(),
    };

    for file in &report.test_results {
        for assertion in &file.assertions {
            if assertion.status == "failed" {
                let test_name = if !assertion.full_name.is_empty() {
                    assertion.full_name.clone()
                } else {
                    assertion.title.clone()
                };
                parsed.failures.push(TestFailure {
                    runner: "ts".into(),
                    package: Some(file.name.clone()),
                    test: test_name,
                    file: Some(file.name.clone()),
                    line: None,
                    message: assertion
                        .failure_messages
                        .join("\n")
                        .lines()
                        .next()
                        .unwrap_or("(no failure message)")
                        .to_string(),
                    duration_ms: assertion.duration,
                    flaky_suspected: false,
                });
            }
        }
    }

    // Defensive: when the report omits totals but lists assertions,
    // recount from assertions so the schema never reports 0/0/0.
    if parsed.tests_run == 0 {
        let mut run = 0u32;
        let mut passed = 0u32;
        let mut failed = 0u32;
        for file in &report.test_results {
            for assertion in &file.assertions {
                match assertion.status.as_str() {
                    "passed" => {
                        run += 1;
                        passed += 1;
                    }
                    "failed" => {
                        run += 1;
                        failed += 1;
                    }
                    _ => {}
                }
            }
        }
        parsed.tests_run = run;
        parsed.tests_passed = passed;
        parsed.tests_failed = failed;
    }

    parsed
}

#[cfg(test)]
mod tests {
    include!("ts_test_tests.rs");
}
