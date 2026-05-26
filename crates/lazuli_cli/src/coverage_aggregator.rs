//! T4 — Coverage aggregator.
//!
//! Pulls per-layer reports from `LayerResult`s + native artifacts
//! (Go coverprofile, Vitest/Jest `coverage-summary.json`) and builds
//! the canonical [`CoverageReport`] per proposal §Coverage.
//!
//! The aggregator is deliberately schema-stable: when a metric is not
//! available we omit it rather than synthesize zeros. The aggregate
//! disclosure is informational; gating is per-layer via [`FailOnSpec`].

use std::path::Path;

use serde::Deserialize;

use crate::cmd_test_types::{
    CoverageAggregate, CoverageMetric, CoverageReport, CoverageVerdict, FailOnSpec, Layer,
    LayerResult,
};
use crate::runners::handler_coverage;

/// Build the canonical coverage report from per-layer results +
/// project_root. The aggregator looks for:
///   - `handler_go` from each handler [`LayerResult.runner_native_only`]
///     coverprofile path.
///   - `ts_lines` / `ts_branches` from each ts layer's
///     coverage-summary artifact (vitest/jest --coverage with the
///     `json-summary` reporter writes `coverage/coverage-summary.json`).
///   - `spec_*` from the spec [`LayerResult.issues`] count + the
///     orchestrator's `spec_totals` hint (when provided).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::coverage_aggregator::build_coverage_report;
///
/// // let report = build_coverage_report(&layers, Path::new("."), None, None);
/// ```
pub fn build_coverage_report(
    layers: &[LayerResult],
    project_root: &Path,
    spec_totals: Option<SpecTotals>,
    aggregate_method: Option<&str>,
) -> CoverageReport {
    let mut metrics: Vec<CoverageMetric> = Vec::new();

    // Spec/view metrics — informational until Wave 6.2 lands per-rule
    // covered/total. We emit one synthetic metric per layer when the
    // caller hands us totals; otherwise we omit (do not synthesize).
    if let Some(totals) = spec_totals {
        if totals.spec_total > 0 {
            metrics.push(metric(
                "spec_predicate",
                totals.spec_covered,
                totals.spec_total,
                "lazuli-doctor",
            ));
        }
        if totals.view_total > 0 {
            metrics.push(metric(
                "view_extensibility",
                totals.view_covered,
                totals.view_total,
                "lazuli-doctor",
            ));
        }
    }

    // Handler (Go) — look for coverprofile artifacts on each handler
    // layer.
    for layer in layers.iter().filter(|l| l.layer == Layer::Handler) {
        if let Some(path) = layer.runner_native_only.as_deref() {
            if let Some(metric) = handler_coverage::parse_coverprofile(Path::new(path)) {
                metrics.push(metric);
            }
        }
    }

    // TS coverage — parse vitest/jest coverage-summary.json. We look
    // at the canonical default path (`<project>/coverage/coverage-summary.json`)
    // because both Vitest and Jest write there by default when
    // `--coverage` is set with the `json-summary` reporter.
    if layers.iter().any(|l| l.layer == Layer::Ts) {
        let summary_path = project_root.join("coverage/coverage-summary.json");
        if let Some((lines, branches)) = parse_coverage_summary(&summary_path) {
            metrics.push(metric_from_pct("ts_lines", lines, "c8-json-summary"));
            metrics.push(metric_from_pct("ts_branches", branches, "c8-json-summary"));
        }
    }

    // Assign verdicts: until per-metric thresholds exist on the
    // manifest, every metric is `pass`. The downstream `--fail-on
    // coverage:<id>=<pct>` evaluator updates verdicts in place.
    for m in &mut metrics {
        m.verdict = verdict_for_pct(m.pct, None, None);
    }

    let aggregate = aggregate_method.map(|method| aggregate_for(method, &metrics));

    CoverageReport {
        layers: metrics,
        aggregate,
    }
}

/// Apply [`FailOnSpec::Coverage`] thresholds to the report. Updates
/// per-metric verdicts to `Block` when the metric falls below its
/// threshold. Returns the count of (warns, blocks) emitted.
///
/// Note: `coverage:aggregate=<N>` requires the report to already carry
/// an aggregate (caller must pass `--aggregate-method`). When asked to
/// gate on aggregate without one, we mark a synthetic Block on the
/// (absent) aggregate by returning an error from the caller.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::coverage_aggregator::apply_fail_on;
///
/// // let errs = apply_fail_on(&mut report, &specs);
/// ```
pub fn apply_fail_on(report: &mut CoverageReport, specs: &[FailOnSpec]) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    for spec in specs {
        let FailOnSpec::Coverage { metric, threshold } = spec else {
            continue;
        };
        if metric == "aggregate" {
            match &report.aggregate {
                Some(agg) => {
                    if agg.pct < *threshold {
                        errors.push(format!(
                            "coverage:aggregate {:.1}% < threshold {:.1}% (method={})",
                            agg.pct, threshold, agg.method
                        ));
                    }
                }
                None => {
                    errors.push(format!(
                        "coverage:aggregate={threshold:.1} requires --aggregate-method to be set"
                    ));
                }
            }
            continue;
        }

        match report.layers.iter_mut().find(|m| &m.id == metric) {
            Some(m) => {
                if m.pct < *threshold {
                    m.verdict = CoverageVerdict::Block;
                    errors.push(format!(
                        "coverage:{} {:.1}% < threshold {:.1}%",
                        m.id, m.pct, threshold
                    ));
                }
            }
            None => {
                errors.push(format!(
                    "coverage:{metric} is not reported; nothing to gate on"
                ));
            }
        }
    }

    errors
}

/// Orchestrator-supplied hint for `spec_predicate` / `view_*` metrics
/// when the spec runner doesn't expose raw counts. Each field
/// defaults to 0 and the aggregator only emits the corresponding
/// metric when `*_total > 0`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpecTotals {
    /// Spec predicates covered by tests.
    pub spec_covered: u64,
    /// Total spec predicates in the run.
    pub spec_total: u64,
    /// View constructs covered.
    pub view_covered: u64,
    /// Total view constructs.
    pub view_total: u64,
}

fn metric(id: &str, covered: u64, total: u64, source: &str) -> CoverageMetric {
    let pct = if total == 0 {
        0.0
    } else {
        (covered as f64 / total as f64) * 100.0
    };
    CoverageMetric {
        id: id.into(),
        covered,
        total,
        pct: round2(pct),
        verdict: CoverageVerdict::Pass,
        source: source.into(),
        raw_file: None,
    }
}

fn metric_from_pct(id: &str, pct: f64, source: &str) -> CoverageMetric {
    CoverageMetric {
        id: id.into(),
        covered: 0,
        total: 0,
        pct: round2(pct),
        verdict: CoverageVerdict::Pass,
        source: source.into(),
        raw_file: None,
    }
}

fn verdict_for_pct(_pct: f64, _warn: Option<f64>, _block: Option<f64>) -> CoverageVerdict {
    // Default `pass`; `--fail-on coverage:<id>=<N>` re-evaluates.
    // Manifest-side default warn/block thresholds will land in Wave 6.5;
    // wiring is in place for that day.
    CoverageVerdict::Pass
}

fn aggregate_for(method: &str, metrics: &[CoverageMetric]) -> CoverageAggregate {
    if metrics.is_empty() {
        return CoverageAggregate {
            pct: 0.0,
            method: method.into(),
            disclosure: NO_METRICS_DISCLOSURE.into(),
        };
    }
    let pct = match method {
        "weighted-by-construct-count" => {
            let (covered, total): (u64, u64) = metrics
                .iter()
                .fold((0, 0), |(c, t), m| (c + m.covered, t + m.total));
            if total == 0 {
                metrics.iter().map(|m| m.pct).sum::<f64>() / metrics.len() as f64
            } else {
                (covered as f64 / total as f64) * 100.0
            }
        }
        "arithmetic-mean" => metrics.iter().map(|m| m.pct).sum::<f64>() / metrics.len() as f64,
        "min-of-layers" => metrics.iter().map(|m| m.pct).fold(f64::INFINITY, f64::min),
        _ => 0.0,
    };
    CoverageAggregate {
        pct: round2(pct),
        method: method.into(),
        disclosure: DEFAULT_DISCLOSURE.into(),
    }
}

const DEFAULT_DISCLOSURE: &str =
    "aggregate is informational only; gating uses per-layer thresholds";
const NO_METRICS_DISCLOSURE: &str = "no per-layer metrics available; aggregate is undefined";

#[derive(Debug, Deserialize)]
struct CoverageSummary {
    #[serde(default)]
    total: CoverageSummaryTotal,
}

#[derive(Debug, Default, Deserialize)]
struct CoverageSummaryTotal {
    #[serde(default)]
    lines: CoverageSummaryEntry,
    #[serde(default)]
    branches: CoverageSummaryEntry,
}

#[derive(Debug, Default, Deserialize)]
struct CoverageSummaryEntry {
    #[serde(default)]
    pct: f64,
}

fn parse_coverage_summary(path: &Path) -> Option<(f64, f64)> {
    let contents = std::fs::read_to_string(path).ok()?;
    let summary: CoverageSummary = serde_json::from_str(&contents).ok()?;
    Some((summary.total.lines.pct, summary.total.branches.pct))
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict};

    fn handler_result(coverprofile: Option<&str>) -> LayerResult {
        LayerResult {
            layer: Layer::Handler,
            runner: "go-test".into(),
            result: LayerVerdict::Pass,
            tests_run: 1,
            tests_passed: 1,
            tests_failed: 0,
            issues: 0,
            exit_code: Some(0),
            command: None,
            duration_ms: 1,
            failures: Vec::new(),
            runner_native_only: coverprofile.map(|s| s.to_string()),
            skip_reason: None,
        }
    }

    #[test]
    fn aggregates_handler_coverage_from_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cov.out");
        std::fs::write(
            &path,
            "mode: set
pkg/a.go:1.1,2.1 5 1
pkg/b.go:1.1,2.1 5 0
",
        )
        .unwrap();
        let layer = handler_result(Some(path.to_str().unwrap()));
        let report = build_coverage_report(&[layer], dir.path(), None, None);
        let m = report.metric("handler_go").expect("handler_go metric");
        assert_eq!(m.covered, 5);
        assert_eq!(m.total, 10);
        assert!((m.pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn no_aggregate_when_method_absent() {
        let report = build_coverage_report(&[], Path::new("."), None, None);
        assert!(report.aggregate.is_none());
    }

    #[test]
    fn aggregate_weighted_by_construct_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cov.out");
        std::fs::write(
            &path,
            "mode: set
pkg/a.go:1.1,2.1 80 1
pkg/b.go:1.1,2.1 20 0
",
        )
        .unwrap();
        let layer = handler_result(Some(path.to_str().unwrap()));
        let report = build_coverage_report(
            &[layer],
            dir.path(),
            None,
            Some("weighted-by-construct-count"),
        );
        let agg = report.aggregate.unwrap();
        assert!((agg.pct - 80.0).abs() < 0.01);
        assert_eq!(agg.method, "weighted-by-construct-count");
    }

    #[test]
    fn apply_fail_on_blocks_below_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cov.out");
        std::fs::write(&path, "mode: set\npkg/a.go:1.1,2.1 10 1\n").unwrap();
        let layer = handler_result(Some(path.to_str().unwrap()));
        let mut report = build_coverage_report(&[layer], dir.path(), None, None);
        let specs = vec![FailOnSpec::Coverage {
            metric: "handler_go".into(),
            threshold: 99.0,
        }];
        // 100% coverage so threshold 99 passes.
        let errs = apply_fail_on(&mut report, &specs);
        assert!(errs.is_empty());

        // Lower the metric to force block.
        report.layers[0].pct = 50.0;
        let errs = apply_fail_on(&mut report, &specs);
        assert_eq!(errs.len(), 1);
        assert_eq!(report.layers[0].verdict, CoverageVerdict::Block);
    }

    #[test]
    fn apply_fail_on_aggregate_requires_method() {
        let mut report = CoverageReport {
            layers: vec![],
            aggregate: None,
        };
        let specs = vec![FailOnSpec::Coverage {
            metric: "aggregate".into(),
            threshold: 70.0,
        }];
        let errs = apply_fail_on(&mut report, &specs);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("--aggregate-method"));
    }

    #[test]
    fn apply_fail_on_unknown_metric_errors() {
        let mut report = CoverageReport {
            layers: vec![],
            aggregate: None,
        };
        let specs = vec![FailOnSpec::Coverage {
            metric: "ghost".into(),
            threshold: 50.0,
        }];
        let errs = apply_fail_on(&mut report, &specs);
        assert_eq!(errs.len(), 1);
    }
}
