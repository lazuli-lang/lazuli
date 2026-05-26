//! Shared types for `lazuli test` orchestration — canonical schema
//! per `docs/proposals/lazuli-test-runner-2026-05-24.md` §Output.
//!
//! Layer runners (`runners::spec`, `runners::go_test`, `runners::playwright`,
//! `runners::ts_test`) all produce [`LayerResult`]. The orchestrator
//! aggregates these + optional [`CoverageReport`] into a [`RunReport`]
//! that serializes to the JSON schema in the proposal.
//!
//! The schema is the canonical wire; text/NDJSON renderers in
//! `cmd_test_output.rs` are pure projections.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Closed enum of layer identifiers. Used in CLI `--layer` flag, NDJSON
/// events, and JSON output. Order matches the proposal's deterministic
/// execution order (spec → view → handler → ts → e2e).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Spec,
    View,
    Handler,
    Ts,
    E2e,
}

impl Layer {
    /// Every layer in canonical execution order. Iterated by the
    /// orchestrator when no explicit `--layer` filter was given.
    pub const ALL: &'static [Layer] = &[
        Layer::Spec,
        Layer::View,
        Layer::Handler,
        Layer::Ts,
        Layer::E2e,
    ];

    /// Stable lowercase identifier used in CLI flags, NDJSON events,
    /// and JSON output.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_types::Layer;
    /// assert_eq!(Layer::Spec.as_str(), "spec");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Spec => "spec",
            Layer::View => "view",
            Layer::Handler => "handler",
            Layer::Ts => "ts",
            Layer::E2e => "e2e",
        }
    }

    /// Inverse of `as_str`. Returns `None` for any value outside the
    /// closed catalog so the CLI surfaces a typed error.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_types::Layer;
    /// assert_eq!(Layer::parse("ts"), Some(Layer::Ts));
    /// assert_eq!(Layer::parse("nope"), None);
    /// ```
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "spec" => Some(Layer::Spec),
            "view" => Some(Layer::View),
            "handler" => Some(Layer::Handler),
            "ts" => Some(Layer::Ts),
            "e2e" => Some(Layer::E2e),
            _ => None,
        }
    }
}

/// Outcome of a single layer run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerVerdict {
    Pass,
    Fail,
    /// Layer was skipped (sub-runner not on PATH, or layer not
    /// configured). Distinct from `fail` so CI gates can be precise.
    Skip,
}

/// One failure record inside a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    /// Native runner name ("go-test", "playwright", "vitest", "jest").
    pub runner: String,
    /// Package or spec path identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Test name.
    pub test: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flaky_suspected: bool,
}

/// Per-layer execution result. Schema mirrors proposal §Output JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    pub layer: Layer,
    /// Native runner identifier ("lazuli-doctor", "go-test", "playwright",
    /// "vitest", "jest").
    pub runner: String,
    pub result: LayerVerdict,
    /// Total number of tests (or issues, for spec/view).
    #[serde(default)]
    pub tests_run: u32,
    #[serde(default)]
    pub tests_passed: u32,
    #[serde(default)]
    pub tests_failed: u32,
    /// Number of structured findings (e.g. doctor issues).
    #[serde(default, skip_serializing_if = "skip_zero")]
    pub issues: u32,
    /// Sub-runner exit code, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Quoted, human-readable command that was executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Wall-clock duration of this layer's run.
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<TestFailure>,
    /// Optional pointer to a native artifact the runner produced
    /// (Playwright HTML report path, go coverprofile path, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_native_only: Option<String>,
    /// Reason for `LayerVerdict::Skip`. Populated for diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

fn skip_zero(n: &u32) -> bool {
    *n == 0
}

/// One per-layer coverage metric. Per proposal §Coverage aggregation,
/// each metric carries `covered`, `total`, and an explicit
/// `verdict` (`pass` / `warn` / `block`) — never an aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageMetric {
    /// Stable layer-metric identifier. Closed catalog:
    /// `spec_predicate`, `spec_actor_matrix`, `spec_transition_state`,
    /// `view_extensibility`, `handler_go`, `view_e2e_pair`,
    /// `ts_lines`, `ts_branches`.
    pub id: String,
    /// Numerator. Either lines covered, constructs covered, or some
    /// other layer-defined count.
    pub covered: u64,
    /// Denominator.
    pub total: u64,
    /// 0.0–100.0; pre-computed for renderers that don't want to
    /// re-divide.
    pub pct: f64,
    pub verdict: CoverageVerdict,
    /// Where the number came from. Closed catalog of methods so
    /// audits can reconstruct the count.
    pub source: String,
    /// Raw artifact pointer (path to go coverprofile, c8 json
    /// report, etc.) when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_file: Option<String>,
}

/// Verdict for one [`CoverageMetric`] against its configured gate.
///
/// `Pass` ≥ green threshold; `Warn` in the configured warn band but
/// still acceptable; `Block` below the hard threshold and gates the
/// CI surface non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageVerdict {
    /// Metric meets or exceeds the green threshold.
    Pass,
    /// Metric in the warn band — surface but do not gate.
    Warn,
    /// Metric below the hard threshold — gate.
    Block,
}

/// Aggregate disclosure block. Informational by default; the runner
/// refuses to gate on aggregate unless `--aggregate-method` is set
/// explicitly (proposal §Coverage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageAggregate {
    pub pct: f64,
    /// Closed catalog of methods: `weighted-by-construct-count`,
    /// `arithmetic-mean`, `min-of-layers`, `none`.
    pub method: String,
    /// Standard disclosure string per proposal.
    pub disclosure: String,
}

/// Full coverage report. One entry per metric in `layers`, plus the
/// informational aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub layers: Vec<CoverageMetric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<CoverageAggregate>,
}

impl CoverageReport {
    /// Find a per-layer metric by its stable `id` (e.g. `handler_go`).
    /// Returns `None` when the layer did not report that metric.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_types::CoverageReport;
    /// // let metric = report.metric("handler_go");
    /// ```
    pub fn metric(&self, id: &str) -> Option<&CoverageMetric> {
        self.layers.iter().find(|m| m.id == id)
    }
}

/// Top-level run summary. Mirrors proposal §Output JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub layers_run: u32,
    pub layers_failed: u32,
    pub layers_skipped: u32,
    pub coverage_warnings: u32,
    pub coverage_blocks: u32,
    #[serde(rename = "overall_verdict")]
    pub overall: LayerVerdict,
    pub duration_ms: u64,
}

/// Canonical report emitted by `lazuli test`. Mirrors proposal §Output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    pub result: LayerVerdict,
    pub layers: Vec<LayerResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageReport>,
    pub summary: RunSummary,
}

/// Closed parser for `--fail-on` argument. Mirrors TDD/BDD Wave 2.2.
/// Currently supports:
///   - `category:<Category>`            — fail on any finding in a
///     doctor rule category (e.g. `TestDiscipline`).
///   - `coverage:<metric_id>=<pct>`     — fail when a specific
///     coverage metric falls below the threshold.
///   - `coverage:aggregate=<pct>`       — fail when aggregate falls
///     below threshold. ONLY honored when `--aggregate-method` is
///     also set on the CLI (the runner enforces that pairing).
#[derive(Debug, Clone, PartialEq)]
pub enum FailOnSpec {
    Category(String),
    Coverage {
        /// Layer metric id (e.g. `handler_go`, `spec_predicate`) OR
        /// the literal `aggregate`.
        metric: String,
        threshold: f64,
    },
}

impl FailOnSpec {
    /// Parse one `--fail-on <spec>` value. Returns an error string the
    /// CLI can surface verbatim.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_types::FailOnSpec;
    ///
    /// let spec = FailOnSpec::parse("category:TestDiscipline").unwrap();
    /// assert!(matches!(spec, FailOnSpec::Category(_)));
    /// ```
    pub fn parse(raw: &str) -> Result<Self, String> {
        if let Some(rest) = raw.strip_prefix("category:") {
            if rest.is_empty() {
                return Err(format!("--fail-on {raw}: category name is required"));
            }
            return Ok(Self::Category(rest.to_string()));
        }
        if let Some(rest) = raw.strip_prefix("coverage:") {
            let (metric, threshold) = rest
                .split_once('=')
                .ok_or_else(|| format!("--fail-on {raw}: expected coverage:<metric>=<pct>"))?;
            if metric.is_empty() {
                return Err(format!("--fail-on {raw}: metric id is required"));
            }
            let pct: f64 = threshold
                .parse()
                .map_err(|_| format!("--fail-on {raw}: threshold `{threshold}` is not a number"))?;
            if !(0.0..=100.0).contains(&pct) {
                return Err(format!("--fail-on {raw}: threshold must be in 0..=100"));
            }
            return Ok(Self::Coverage {
                metric: metric.to_string(),
                threshold: pct,
            });
        }
        Err(format!(
            "--fail-on {raw}: unsupported form (expected `category:<name>` or `coverage:<metric>=<pct>`)"
        ))
    }
}

/// Lightweight aggregator wrapper used by the orchestrator before
/// rendering. Holds raw layer results plus accumulated wall clock.
#[derive(Debug, Default)]
pub struct RunAccumulator {
    pub layer_results: Vec<LayerResult>,
    pub started_at_ms: u64,
    pub coverage: Option<CoverageReport>,
    pub project_root: PathBuf,
}

impl RunAccumulator {
    /// Roll the staged per-layer results into a [`RunReport`]. The
    /// returned summary picks `Fail` whenever any layer failed or any
    /// coverage metric blocked; otherwise `Pass`. `total_duration_ms`
    /// is the orchestrator's measured wall-clock.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_types::RunAccumulator;
    /// let report = RunAccumulator::default().finalize(0);
    /// ```
    pub fn finalize(self, total_duration_ms: u64) -> RunReport {
        let layers_run = self.layer_results.len() as u32;
        let layers_failed = self
            .layer_results
            .iter()
            .filter(|l| l.result == LayerVerdict::Fail)
            .count() as u32;
        let layers_skipped = self
            .layer_results
            .iter()
            .filter(|l| l.result == LayerVerdict::Skip)
            .count() as u32;

        let (coverage_warnings, coverage_blocks) = match &self.coverage {
            Some(cov) => {
                let warn = cov
                    .layers
                    .iter()
                    .filter(|m| m.verdict == CoverageVerdict::Warn)
                    .count() as u32;
                let block = cov
                    .layers
                    .iter()
                    .filter(|m| m.verdict == CoverageVerdict::Block)
                    .count() as u32;
                (warn, block)
            }
            None => (0, 0),
        };

        let overall = if layers_failed > 0 || coverage_blocks > 0 {
            LayerVerdict::Fail
        } else {
            LayerVerdict::Pass
        };

        RunReport {
            schema_version: 1,
            result: overall,
            layers: self.layer_results,
            coverage: self.coverage,
            summary: RunSummary {
                layers_run,
                layers_failed,
                layers_skipped,
                coverage_warnings,
                coverage_blocks,
                overall,
                duration_ms: total_duration_ms,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_round_trips_through_str() {
        for layer in Layer::ALL {
            assert_eq!(Layer::parse(layer.as_str()), Some(*layer));
        }
        assert_eq!(Layer::parse("nope"), None);
    }

    #[test]
    fn fail_on_parses_category() {
        let spec = FailOnSpec::parse("category:TestDiscipline").unwrap();
        assert_eq!(spec, FailOnSpec::Category("TestDiscipline".into()));
    }

    #[test]
    fn fail_on_parses_coverage_metric() {
        let spec = FailOnSpec::parse("coverage:handler_go=70").unwrap();
        assert_eq!(
            spec,
            FailOnSpec::Coverage {
                metric: "handler_go".into(),
                threshold: 70.0
            }
        );
    }

    #[test]
    fn fail_on_parses_coverage_aggregate() {
        let spec = FailOnSpec::parse("coverage:aggregate=85").unwrap();
        assert_eq!(
            spec,
            FailOnSpec::Coverage {
                metric: "aggregate".into(),
                threshold: 85.0
            }
        );
    }

    #[test]
    fn fail_on_rejects_unknown_prefix() {
        assert!(FailOnSpec::parse("severity:error").is_err());
    }

    #[test]
    fn fail_on_rejects_missing_threshold() {
        assert!(FailOnSpec::parse("coverage:handler_go").is_err());
    }

    #[test]
    fn fail_on_rejects_non_numeric_threshold() {
        assert!(FailOnSpec::parse("coverage:handler_go=high").is_err());
    }

    #[test]
    fn fail_on_rejects_out_of_range() {
        assert!(FailOnSpec::parse("coverage:handler_go=150").is_err());
        assert!(FailOnSpec::parse("coverage:handler_go=-1").is_err());
    }

    #[test]
    fn report_aggregation_picks_fail_when_any_layer_failed() {
        let acc = RunAccumulator {
            layer_results: vec![
                LayerResult {
                    layer: Layer::Spec,
                    runner: "lazuli-doctor".into(),
                    result: LayerVerdict::Pass,
                    tests_run: 1,
                    tests_passed: 1,
                    tests_failed: 0,
                    issues: 0,
                    exit_code: Some(0),
                    command: None,
                    duration_ms: 10,
                    failures: vec![],
                    runner_native_only: None,
                    skip_reason: None,
                },
                LayerResult {
                    layer: Layer::Handler,
                    runner: "go-test".into(),
                    result: LayerVerdict::Fail,
                    tests_run: 5,
                    tests_passed: 4,
                    tests_failed: 1,
                    issues: 0,
                    exit_code: Some(1),
                    command: Some("go test ./...".into()),
                    duration_ms: 100,
                    failures: vec![],
                    runner_native_only: None,
                    skip_reason: None,
                },
            ],
            ..Default::default()
        };
        let report = acc.finalize(120);
        assert_eq!(report.summary.layers_run, 2);
        assert_eq!(report.summary.layers_failed, 1);
        assert_eq!(report.summary.overall, LayerVerdict::Fail);
    }

    #[test]
    fn report_aggregation_picks_pass_when_all_layers_passed() {
        let acc = RunAccumulator {
            layer_results: vec![LayerResult {
                layer: Layer::Spec,
                runner: "lazuli-doctor".into(),
                result: LayerVerdict::Pass,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: None,
                duration_ms: 5,
                failures: vec![],
                runner_native_only: None,
                skip_reason: None,
            }],
            ..Default::default()
        };
        let report = acc.finalize(10);
        assert_eq!(report.summary.overall, LayerVerdict::Pass);
    }
}
