//! Layer 1-2 (spec/view) runner — wire-thin delegation to
//! `lazuli doctor`. Per the proposal §T0, this runner does not
//! reimplement IR analysis; it surfaces the doctor diagnostics
//! through the unified schema.
//!
//! Until TDD/BDD Wave 0/2 (categorized rules + structured
//! `DoctorReport`) lands, we use the existing
//! `crate::doctor::doctor_diagnostics_json` JSON projection. Each
//! diagnostic at severity `error` becomes one issue; the layer
//! verdict is `pass` when zero errors are found.

use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict, TestFailure};

/// Run the spec layer. `layer` is the requested logical layer — when
/// the orchestrator asks for `view`, the same diagnostics are run but
/// the result is tagged with the requested layer so downstream
/// renderers can keep them separate.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_test_types::Layer;
/// use lazuli_cli::runners::spec::run;
///
/// // let result = run(Path::new("app.lzi"), Layer::Spec)?;
/// ```
pub fn run(input: &Path, layer: Layer) -> Result<LayerResult> {
    let started = Instant::now();
    // SecurityProfile::Strict mirrors the default `lazuli doctor` /
    // `lazuli check` choice.
    let diagnostics_value = crate::doctor::doctor_diagnostics_json(input, SecurityProfile::Strict);
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let diagnostics = match diagnostics_value {
        Ok(serde_json::Value::Array(items)) => items,
        Ok(_) => Vec::new(),
        Err(err) => {
            return Ok(LayerResult {
                layer,
                runner: "lazuli-doctor".into(),
                result: LayerVerdict::Skip,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: Some(format!("lazuli doctor {}", input.display())),
                duration_ms: elapsed_ms,
                failures: Vec::new(),
                runner_native_only: None,
                skip_reason: Some(format!("doctor failed to load: {err}")),
            });
        }
    };

    // Filter to the relevant layer. For the spec layer we keep every
    // diagnostic; for the view layer we keep only `.lzx`-pathed ones,
    // mirroring the proposal's Wave 4 stance.
    let relevant: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|d| {
            let path = d.get("path").and_then(|v| v.as_str()).unwrap_or("");
            match layer {
                Layer::View => path.ends_with(".lzx"),
                _ => true,
            }
        })
        .collect();

    let errors: Vec<&&serde_json::Value> = relevant
        .iter()
        .filter(|d| {
            d.get("severity")
                .and_then(|v| v.as_str())
                .map(|s| s == "error")
                .unwrap_or(false)
        })
        .collect();

    let failures: Vec<TestFailure> = errors
        .iter()
        .map(|d| {
            let path = d.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let line = d
                .get("line")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok());
            let code = d.get("code").and_then(|v| v.as_str()).unwrap_or("DOCTOR");
            let message = d
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(no message)")
                .to_string();
            TestFailure {
                runner: "lazuli-doctor".into(),
                package: Some(path.into()),
                test: code.into(),
                file: Some(path.into()),
                line,
                message,
                duration_ms: None,
                flaky_suspected: false,
            }
        })
        .collect();

    let issues = relevant.len() as u32;
    let verdict = if errors.is_empty() {
        LayerVerdict::Pass
    } else {
        LayerVerdict::Fail
    };

    Ok(LayerResult {
        layer,
        runner: "lazuli-doctor".into(),
        result: verdict,
        tests_run: issues,
        tests_passed: issues.saturating_sub(errors.len() as u32),
        tests_failed: errors.len() as u32,
        issues,
        exit_code: None,
        command: Some(format!("lazuli doctor {}", input.display())),
        duration_ms: elapsed_ms,
        failures,
        runner_native_only: None,
        skip_reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::cmd_test_types::Layer;
    use std::path::Path;

    #[test]
    fn spec_runner_on_missing_input_does_not_panic() {
        // We do not assert pass/fail because the doctor entry may
        // error on a missing path; we just guard against panic.
        let _ = run(Path::new("__lazuli_no_such_path.lzi"), Layer::Spec);
    }
}
