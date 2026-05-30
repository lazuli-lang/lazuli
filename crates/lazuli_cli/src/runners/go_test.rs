//! Layer 3 (handler) runner — shells `go test -json`.
//!
//! The `-json` flag emits one JSON object per event (action: `run`,
//! `pass`, `fail`, `output`, ...). We parse the stream into a
//! [`LayerResult`]. Failures carry the package + test name + the
//! last `output` line before the `fail` event (best-effort message).
//!
//! Pre-flight: `go version`. When Go is not on PATH the layer is
//! reported as `skip` with `skip_reason` set so the orchestrator can
//! surface the gap without failing the whole run.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::Result;
use serde::Deserialize;

use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict, TestFailure};
use crate::lazurite_manifest::{Manifest, TestingGo};

/// Per-call overrides for the Go test runner. Knobs that come from
/// the CLI flag bag rather than the manifest live here.
#[derive(Debug, Default)]
pub struct GoRunOptions {
    /// Override the default `<app_dir>/features/...` package pattern.
    pub package_pattern: Option<String>,
    /// Extra args appended after the `go test` flags but before the
    /// pattern. Mirrors `lazuli test --layer handler -- -run TestX`.
    pub extra_args: Vec<String>,
    /// Force coverage on/off; `None` means honor the manifest.
    pub coverage_override: Option<bool>,
}

/// Probe `go version`. Returns `None` if Go is not on PATH.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::go_test::probe;
/// // let version = probe();
/// ```
pub fn probe() -> Option<String> {
    let output = Command::new("go").arg("version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Execute `go test -json` against the project's handler packages.
/// Honors `[testing.go]` manifest knobs + the per-call `opts` for
/// coverage, package pattern, and extra args. Surfaces a
/// `LayerVerdict::Skip` when the Go toolchain is absent so the
/// orchestrator can render a clean skip row.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::runners::go_test::{run, GoRunOptions};
///
/// // let result = run(manifest.as_ref(), Path::new("."), GoRunOptions::default())?;
/// ```
pub fn run(
    manifest: Option<&Manifest>,
    project_root: &Path,
    opts: GoRunOptions,
) -> Result<LayerResult> {
    let started = Instant::now();
    let go_cfg: TestingGo = manifest
        .and_then(|m| m.testing.as_ref())
        .and_then(|t| t.go.as_ref())
        .cloned()
        .unwrap_or_default();

    if probe().is_none() {
        return Ok(LayerResult {
            layer: Layer::Handler,
            runner: "go-test".into(),
            result: LayerVerdict::Skip,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            issues: 0,
            exit_code: None,
            command: Some("go test".into()),
            duration_ms: started.elapsed().as_millis() as u64,
            failures: Vec::new(),
            runner_native_only: None,
            skip_reason: Some("`go` not on PATH (install Go >= 1.21)".into()),
        });
    }

    let pattern = opts
        .package_pattern
        .or(go_cfg.package_pattern)
        .unwrap_or_else(|| default_package_pattern(manifest));

    let coverage = opts.coverage_override.unwrap_or(go_cfg.coverage);
    let coverage_out = if coverage {
        Some(go_cfg.coverage_out.clone().unwrap_or_else(|| {
            project_root
                .join("dist/coverage/handler.cov.out")
                .to_string_lossy()
                .into_owned()
        }))
    } else {
        None
    };

    let mut cmd = Command::new("go");
    cmd.arg("test").arg("-json");
    if let Some(out) = &coverage_out {
        if let Some(parent) = Path::new(out).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        cmd.arg(format!("-coverprofile={}", out));
    }
    for flag in &go_cfg.flags {
        cmd.arg(flag);
    }
    for arg in &opts.extra_args {
        cmd.arg(arg);
    }
    cmd.arg(&pattern);
    cmd.current_dir(project_root);

    let command_pretty = format!(
        "go test -json{} {}{}",
        if let Some(out) = &coverage_out {
            format!(" -coverprofile={}", out)
        } else {
            String::new()
        },
        go_cfg.flags.join(" "),
        if go_cfg.flags.is_empty() {
            pattern.clone()
        } else {
            format!(" {pattern}")
        }
    );

    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Ok(LayerResult {
                layer: Layer::Handler,
                runner: "go-test".into(),
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
                skip_reason: Some(format!("failed to spawn `go test`: {err}")),
            });
        }
    };

    let parsed = parse_go_test_json(&output.stdout);
    let exit_code = output.status.code();
    let verdict = if exit_code == Some(0) && parsed.tests_failed == 0 {
        LayerVerdict::Pass
    } else {
        LayerVerdict::Fail
    };

    Ok(LayerResult {
        layer: Layer::Handler,
        runner: "go-test".into(),
        result: verdict,
        tests_run: parsed.tests_run,
        tests_passed: parsed.tests_passed,
        tests_failed: parsed.tests_failed,
        issues: 0,
        exit_code,
        command: Some(command_pretty),
        duration_ms: started.elapsed().as_millis() as u64,
        failures: parsed.failures,
        runner_native_only: coverage_out,
        skip_reason: None,
    })
}

fn default_package_pattern(manifest: Option<&Manifest>) -> String {
    if let Some(m) = manifest
        && let Some(subdir) = m.lazurite.as_ref().and_then(|l| l.app_dir.as_deref())
    {
        return format!("./{subdir}/features/...");
    }
    "./...".to_string()
}

#[derive(Debug, Default)]
pub(crate) struct ParsedRun {
    tests_run: u32,
    tests_passed: u32,
    tests_failed: u32,
    failures: Vec<TestFailure>,
}

#[derive(Debug, Deserialize)]
struct GoTestEvent {
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Package")]
    package: Option<String>,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Output")]
    output: Option<String>,
    #[serde(rename = "Elapsed")]
    elapsed: Option<f64>,
}

/// Public for unit tests. Parses Go `-json` event stream — one JSON
/// object per line, terminating events `pass`/`fail`/`skip` per test.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::go_test::parse_go_test_json;
///
/// // let run = parse_go_test_json(stdout_bytes);
/// ```
pub(crate) fn parse_go_test_json(stdout: &[u8]) -> ParsedRun {
    parse_go_test_json_impl(stdout)
}

fn parse_go_test_json_impl(stdout: &[u8]) -> ParsedRun {
    use std::collections::HashMap;

    let mut last_output: HashMap<(String, String), String> = HashMap::new();
    let mut parsed = ParsedRun::default();

    for line in stdout.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let event: GoTestEvent = match serde_json::from_slice(line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let Some(test) = event.test.clone() else {
            continue; // Package-level event.
        };
        let pkg = event.package.clone().unwrap_or_default();
        match event.action.as_str() {
            "output" => {
                if let Some(out) = event.output {
                    last_output
                        .entry((pkg, test))
                        .and_modify(|prev| {
                            *prev = out.clone();
                        })
                        .or_insert(out);
                }
            }
            "pass" => {
                parsed.tests_run += 1;
                parsed.tests_passed += 1;
            }
            "fail" => {
                parsed.tests_run += 1;
                parsed.tests_failed += 1;
                let message = last_output
                    .get(&(pkg.clone(), test.clone()))
                    .cloned()
                    .unwrap_or_else(|| "go test fail (no output captured)".into());
                let (file, line) = parse_file_line(&message);
                parsed.failures.push(TestFailure {
                    runner: "go-test".into(),
                    package: Some(pkg),
                    test: test.clone(),
                    file,
                    line,
                    message: message.trim().to_string(),
                    duration_ms: event.elapsed.map(|s| (s * 1000.0) as u64),
                    flaky_suspected: false,
                });
            }
            _ => {}
        }
    }

    parsed
}

/// Best-effort extraction of `<file>:<line>` from a Go test output
/// line such as `    handlers/compute_balance_test.go:42: expected 100, got 0`.
fn parse_file_line(s: &str) -> (Option<String>, Option<u32>) {
    let trimmed = s.trim_start();
    let segment = trimmed
        .split_whitespace()
        .find(|part| part.contains(".go:"));
    let Some(seg) = segment else {
        return (None, None);
    };
    let (file, rest) = match seg.split_once(".go:") {
        Some((f, r)) => (format!("{f}.go"), r),
        None => return (None, None),
    };
    let line_str = rest.split(':').next().unwrap_or("");
    let line = line_str
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse::<u32>()
        .ok();
    (Some(file), line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pass_and_fail_events() {
        let stdout = br#"{"Action":"run","Package":"app/features/post","Test":"TestA"}
{"Action":"output","Package":"app/features/post","Test":"TestA","Output":"   handlers/post_test.go:12: expected 1, got 2\n"}
{"Action":"fail","Package":"app/features/post","Test":"TestA","Elapsed":0.012}
{"Action":"run","Package":"app/features/post","Test":"TestB"}
{"Action":"pass","Package":"app/features/post","Test":"TestB","Elapsed":0.005}
"#;
        let parsed = parse_go_test_json_impl(stdout);
        assert_eq!(parsed.tests_run, 2);
        assert_eq!(parsed.tests_passed, 1);
        assert_eq!(parsed.tests_failed, 1);
        assert_eq!(parsed.failures.len(), 1);
        let f = &parsed.failures[0];
        assert_eq!(f.test, "TestA");
        assert_eq!(f.file.as_deref(), Some("handlers/post_test.go"));
        assert_eq!(f.line, Some(12));
        assert_eq!(f.duration_ms, Some(12));
    }

    #[test]
    fn ignores_unparseable_lines() {
        let stdout = b"not json\n{\"Action\":\"pass\",\"Package\":\"p\",\"Test\":\"T\"}\n";
        let parsed = parse_go_test_json_impl(stdout);
        assert_eq!(parsed.tests_run, 1);
        assert_eq!(parsed.tests_passed, 1);
    }

    #[test]
    fn extracts_file_line_from_message() {
        let (f, l) = parse_file_line("    handlers/x_test.go:42: nope");
        assert_eq!(f.as_deref(), Some("handlers/x_test.go"));
        assert_eq!(l, Some(42));
    }

    #[test]
    fn missing_file_line_returns_none() {
        let (f, l) = parse_file_line("unknown panic");
        assert!(f.is_none());
        assert!(l.is_none());
    }
}
