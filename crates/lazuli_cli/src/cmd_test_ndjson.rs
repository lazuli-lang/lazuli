//! T5 — Streaming NDJSON event emitter for `lazuli test`.
//!
//! Each event is one JSON object on its own line. Closed catalog of
//! event types (per proposal §Output NDJSON):
//!   - `layer_start { layer, runner, [command], timestamp }`
//!   - `layer_complete { layer, result, duration_ms, [tests_*], [issues] }`
//!   - `finding { layer, rule, path, [line], message }`
//!   - `test_complete { layer, package, test, result, duration_ms }`
//!   - `coverage { metric, covered, total, pct, verdict, source }`
//!   - `summary { overall_verdict, layers_run, layers_failed, ... }`
//!   - `runner_skip { layer, runner, reason }`
//!
//! Emitter is sync: writes are flushed on every event so consumers
//! reading line-by-line see events as they happen. Output goes to the
//! Writer passed by the orchestrator (stdout for `--format ndjson`).

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::cmd_test_types::{CoverageMetric, Layer, LayerResult, LayerVerdict, RunReport};

pub struct NdjsonEmitter<W: Write> {
    out: W,
}

impl<W: Write> NdjsonEmitter<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    fn write_event(&mut self, value: Value) {
        // Best effort: NDJSON consumers prefer a missed event over a
        // crash mid-stream. Errors are silently dropped (matches the
        // wider runner convention).
        if let Ok(bytes) = serde_json::to_vec(&value) {
            let _ = self.out.write_all(&bytes);
            let _ = self.out.write_all(b"\n");
            let _ = self.out.flush();
        }
    }

    pub fn layer_start(&mut self, layer: Layer, runner: &str, command: Option<&str>) {
        let mut obj = serde_json::json!({
            "event": "layer_start",
            "layer": layer.as_str(),
            "runner": runner,
            "timestamp": iso_now(),
        });
        if let Some(c) = command {
            obj["command"] = Value::String(c.to_string());
        }
        self.write_event(obj);
    }

    pub fn layer_complete(&mut self, layer: &LayerResult) {
        self.write_event(serde_json::json!({
            "event": "layer_complete",
            "layer": layer.layer.as_str(),
            "runner": layer.runner,
            "result": verdict_str(layer.result),
            "tests_run": layer.tests_run,
            "tests_passed": layer.tests_passed,
            "tests_failed": layer.tests_failed,
            "issues": layer.issues,
            "duration_ms": layer.duration_ms,
        }));
    }

    pub fn runner_skip(&mut self, layer: Layer, runner: &str, reason: &str) {
        self.write_event(serde_json::json!({
            "event": "runner_skip",
            "layer": layer.as_str(),
            "runner": runner,
            "reason": reason,
        }));
    }

    pub fn finding(
        &mut self,
        layer: Layer,
        rule: &str,
        path: &str,
        line: Option<u32>,
        message: &str,
    ) {
        let mut obj = serde_json::json!({
            "event": "finding",
            "layer": layer.as_str(),
            "rule": rule,
            "path": path,
            "message": message,
        });
        if let Some(l) = line {
            obj["line"] = Value::from(l);
        }
        self.write_event(obj);
    }

    pub fn coverage(&mut self, metric: &CoverageMetric) {
        self.write_event(serde_json::json!({
            "event": "coverage",
            "metric": metric.id,
            "covered": metric.covered,
            "total": metric.total,
            "pct": metric.pct,
            "verdict": match metric.verdict {
                crate::cmd_test_types::CoverageVerdict::Pass => "pass",
                crate::cmd_test_types::CoverageVerdict::Warn => "warn",
                crate::cmd_test_types::CoverageVerdict::Block => "block",
            },
            "source": metric.source,
        }));
    }

    pub fn summary(&mut self, report: &RunReport) {
        self.write_event(serde_json::json!({
            "event": "summary",
            "overall_verdict": verdict_str(report.summary.overall),
            "layers_run": report.summary.layers_run,
            "layers_failed": report.summary.layers_failed,
            "layers_skipped": report.summary.layers_skipped,
            "coverage_warnings": report.summary.coverage_warnings,
            "coverage_blocks": report.summary.coverage_blocks,
            "duration_ms": report.summary.duration_ms,
        }));
    }
}

fn verdict_str(v: LayerVerdict) -> &'static str {
    match v {
        LayerVerdict::Pass => "pass",
        LayerVerdict::Fail => "fail",
        LayerVerdict::Skip => "skip",
    }
}

fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // No external chrono dep; emit a stable epoch-derived form so
    // consumers can parse without ambiguity.
    format!("epoch:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_test_types::{
        CoverageMetric, CoverageVerdict, Layer, LayerResult, LayerVerdict, RunReport, RunSummary,
    };

    fn buf() -> NdjsonEmitter<Vec<u8>> {
        NdjsonEmitter::new(Vec::new())
    }

    #[test]
    fn emits_layer_start_event() {
        let mut e = buf();
        e.layer_start(Layer::Handler, "go-test", Some("go test ./..."));
        let bytes = e.into_inner();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"event\":\"layer_start\""));
        assert!(text.contains("\"layer\":\"handler\""));
        assert!(text.contains("\"command\":\"go test ./...\""));
    }

    #[test]
    fn emits_coverage_event() {
        let mut e = buf();
        e.coverage(&CoverageMetric {
            id: "handler_go".into(),
            covered: 7,
            total: 9,
            pct: 77.78,
            verdict: CoverageVerdict::Pass,
            source: "go-coverprofile".into(),
            raw_file: None,
        });
        let text = String::from_utf8(e.into_inner()).unwrap();
        assert!(text.contains("\"metric\":\"handler_go\""));
        assert!(text.contains("\"verdict\":\"pass\""));
        assert!(text.contains("\"pct\":77.78"));
    }

    #[test]
    fn emits_summary_with_overall_verdict() {
        let mut e = buf();
        let report = RunReport {
            schema_version: 1,
            result: LayerVerdict::Fail,
            layers: vec![LayerResult {
                layer: Layer::Spec,
                runner: "lazuli-doctor".into(),
                result: LayerVerdict::Pass,
                tests_run: 1,
                tests_passed: 1,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: None,
                duration_ms: 1,
                failures: vec![],
                runner_native_only: None,
                skip_reason: None,
            }],
            coverage: None,
            summary: RunSummary {
                layers_run: 1,
                layers_failed: 0,
                layers_skipped: 0,
                coverage_warnings: 0,
                coverage_blocks: 0,
                overall: LayerVerdict::Pass,
                duration_ms: 10,
            },
        };
        e.summary(&report);
        let text = String::from_utf8(e.into_inner()).unwrap();
        assert!(text.contains("\"event\":\"summary\""));
        assert!(text.contains("\"overall_verdict\":\"pass\""));
    }
}
