//! Layer 5 (e2e) runner — shells `npx playwright test --reporter=json`.
//!
//! Per the proposal §T2, Playwright produces a self-describing JSON
//! report; we shell out, capture stdout, and parse a minimal subset
//! (suites → specs → tests with status + duration). When the
//! `playwright-report/` HTML artifact is present we surface its
//! path under `runner_native_only` rather than try to embed.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::Result;
use serde::Deserialize;

use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict, TestFailure};
use crate::lazurite_manifest::{Manifest, TestingPlaywright};

/// Probe `npx playwright --version`. Returns `None` if Playwright
/// is not installed under the project's `node_modules`.
pub fn probe() -> Option<String> {
    let output = Command::new("npx")
        .arg("playwright")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Execute `npx playwright test --reporter=json` against the
/// resolved e2e config and project. Surfaces a
/// `LayerVerdict::Skip` when Playwright is absent.
pub fn run(manifest: Option<&Manifest>, project_root: &Path) -> Result<LayerResult> {
    let started = Instant::now();
    // Frente 1 — resolve effective `[testing.playwright]` honoring
    // authored overrides + canonical layout-derived defaults
    // (`app/web/playwright.config.ts` or
    // `app/clients/<name>/playwright.config.ts`).
    let cfg: TestingPlaywright = manifest
        .and_then(|m| m.testing_playwright_resolved(project_root))
        .unwrap_or_default();

    // If neither manifest nor convention has e2e content, skip the
    // layer gracefully. The orchestrator only invokes us when it
    // believes the layer is configured.
    if probe().is_none() {
        return Ok(LayerResult {
            layer: Layer::E2e,
            runner: "playwright".into(),
            result: LayerVerdict::Skip,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            issues: 0,
            exit_code: None,
            command: Some("npx playwright test".into()),
            duration_ms: started.elapsed().as_millis() as u64,
            failures: Vec::new(),
            runner_native_only: None,
            skip_reason: Some("`npx playwright` not available".into()),
        });
    }

    let mut cmd = Command::new("npx");
    cmd.arg("playwright").arg("test").arg("--reporter=json");
    if let Some(c) = &cfg.config {
        cmd.arg(format!("--config={c}"));
    }
    if let Some(w) = cfg.workers {
        cmd.arg(format!("--workers={w}"));
    }
    if let Some(p) = &cfg.project {
        cmd.arg(format!("--project={p}"));
    }
    let discovery = cfg.discovery_root.clone().unwrap_or_else(|| "e2e/".into());
    cmd.arg(&discovery);
    for flag in &cfg.flags {
        cmd.arg(flag);
    }
    cmd.current_dir(project_root);

    let command_pretty = format!("npx playwright test --reporter=json {discovery}");

    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Ok(LayerResult {
                layer: Layer::E2e,
                runner: "playwright".into(),
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
                skip_reason: Some(format!("failed to spawn playwright: {err}")),
            });
        }
    };

    let parsed = parse_playwright_json(&output.stdout);
    let html_report = project_root.join("playwright-report/index.html");
    let runner_native_only = if html_report.exists() {
        Some(html_report.to_string_lossy().into_owned())
    } else {
        None
    };

    let exit_code = output.status.code();
    let verdict = if exit_code == Some(0) && parsed.tests_failed == 0 {
        LayerVerdict::Pass
    } else {
        LayerVerdict::Fail
    };

    Ok(LayerResult {
        layer: Layer::E2e,
        runner: "playwright".into(),
        result: verdict,
        tests_run: parsed.tests_run,
        tests_passed: parsed.tests_passed,
        tests_failed: parsed.tests_failed,
        issues: 0,
        exit_code,
        command: Some(command_pretty),
        duration_ms: started.elapsed().as_millis() as u64,
        failures: parsed.failures,
        runner_native_only,
        skip_reason: None,
    })
}

#[derive(Debug, Default)]
struct ParsedRun {
    tests_run: u32,
    tests_passed: u32,
    tests_failed: u32,
    failures: Vec<TestFailure>,
}

#[derive(Debug, Deserialize)]
struct PwReport {
    #[serde(default)]
    suites: Vec<PwSuite>,
}

#[derive(Debug, Deserialize)]
struct PwSuite {
    #[serde(default)]
    title: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    suites: Vec<PwSuite>,
    #[serde(default)]
    specs: Vec<PwSpec>,
}

#[derive(Debug, Deserialize)]
struct PwSpec {
    #[serde(default)]
    title: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    tests: Vec<PwTest>,
}

#[derive(Debug, Deserialize)]
struct PwTest {
    #[serde(default)]
    results: Vec<PwResult>,
}

#[derive(Debug, Deserialize)]
struct PwResult {
    #[serde(default)]
    status: String,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    error: Option<PwError>,
}

#[derive(Debug, Deserialize)]
struct PwError {
    #[serde(default)]
    message: String,
}

/// Parse Playwright's `--reporter=json` blob into a runner-agnostic
/// `ParsedRun` shape. Invalid JSON returns a default (empty) run.
pub fn parse_playwright_json(stdout: &[u8]) -> ParsedRun {
    let report: PwReport = match serde_json::from_slice(stdout) {
        Ok(r) => r,
        Err(_) => return ParsedRun::default(),
    };
    let mut parsed = ParsedRun::default();
    for suite in &report.suites {
        walk_suite(suite, &mut parsed);
    }
    parsed
}

fn walk_suite(suite: &PwSuite, parsed: &mut ParsedRun) {
    for inner in &suite.suites {
        walk_suite(inner, parsed);
    }
    for spec in &suite.specs {
        walk_spec(spec, &suite.file, &suite.title, parsed);
    }
}

fn walk_spec(spec: &PwSpec, suite_file: &str, suite_title: &str, parsed: &mut ParsedRun) {
    let file = if !spec.file.is_empty() {
        spec.file.as_str()
    } else {
        suite_file
    };
    for test in &spec.tests {
        for result in &test.results {
            match result.status.as_str() {
                "passed" => {
                    parsed.tests_run += 1;
                    parsed.tests_passed += 1;
                }
                "failed" | "timedOut" | "interrupted" => {
                    parsed.tests_run += 1;
                    parsed.tests_failed += 1;
                    let message = result
                        .error
                        .as_ref()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| format!("playwright status={}", result.status));
                    parsed.failures.push(TestFailure {
                        runner: "playwright".into(),
                        package: Some(suite_title.to_string()),
                        test: spec.title.clone(),
                        file: Some(file.to_string()),
                        line: Some(spec.line),
                        message,
                        duration_ms: Some(result.duration),
                        flaky_suspected: result.status == "timedOut",
                    });
                }
                "skipped" | "pending" => {}
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_playwright_pass_and_fail() {
        let json = br#"{
          "suites": [
            { "title": "post", "file": "e2e/post.spec.ts",
              "specs": [
                { "title": "creates draft", "file": "e2e/post.spec.ts", "line": 4,
                  "tests": [ { "results": [ { "status": "passed", "duration": 120 } ] } ] },
                { "title": "publishes", "file": "e2e/post.spec.ts", "line": 12,
                  "tests": [ { "results": [
                    { "status": "failed", "duration": 5000,
                      "error": { "message": "expected URL /post/1" } } ] } ] }
              ] }
          ]
        }"#;
        let parsed = parse_playwright_json(json);
        assert_eq!(parsed.tests_run, 2);
        assert_eq!(parsed.tests_passed, 1);
        assert_eq!(parsed.tests_failed, 1);
        assert_eq!(parsed.failures[0].test, "publishes");
        assert_eq!(parsed.failures[0].duration_ms, Some(5000));
    }

    #[test]
    fn empty_input_yields_zero() {
        let parsed = parse_playwright_json(b"");
        assert_eq!(parsed.tests_run, 0);
    }
}
