//! Output renderers for `lazuli test`. Text / JSON projections of the
//! canonical [`RunReport`].

use std::io::Write;

use crate::cmd_test_types::{LayerVerdict, RunReport};

pub fn render_json<W: Write>(report: &RunReport, out: &mut W) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(report).map_err(std::io::Error::other)?;
    out.write_all(s.as_bytes())?;
    out.write_all(b"\n")
}

pub fn render_text<W: Write>(report: &RunReport, out: &mut W) -> std::io::Result<()> {
    writeln!(out, "Running {} layer(s):", report.summary.layers_run)?;
    writeln!(out)?;
    for (idx, layer) in report.layers.iter().enumerate() {
        let total = report.layers.len();
        writeln!(
            out,
            "[{}/{}] {:<8} ({})",
            idx + 1,
            total,
            layer.layer.as_str(),
            layer.runner
        )?;
        if let Some(cmd) = &layer.command {
            writeln!(out, "  $ {cmd}")?;
        }
        if let Some(reason) = &layer.skip_reason {
            writeln!(out, "  ~ SKIP: {reason}")?;
        }
        for f in &layer.failures {
            writeln!(
                out,
                "  x {}{}: {}",
                f.test,
                f.line.map(|l| format!(":{l}")).unwrap_or_default(),
                f.message.lines().next().unwrap_or("")
            )?;
        }
        writeln!(
            out,
            "  -> {} ({} run, {} passed, {} failed{}, {}ms)",
            verdict(layer.result),
            layer.tests_run,
            layer.tests_passed,
            layer.tests_failed,
            if layer.issues > 0 {
                format!(", {} issue(s)", layer.issues)
            } else {
                String::new()
            },
            layer.duration_ms
        )?;
        writeln!(out)?;
    }

    if let Some(cov) = &report.coverage {
        writeln!(out, "Coverage:")?;
        for m in &cov.layers {
            writeln!(
                out,
                "  {:<24} {}/{} ({:.1}%) [{}]",
                m.id,
                m.covered,
                m.total,
                m.pct,
                match m.verdict {
                    crate::cmd_test_types::CoverageVerdict::Pass => "pass",
                    crate::cmd_test_types::CoverageVerdict::Warn => "warn",
                    crate::cmd_test_types::CoverageVerdict::Block => "block",
                }
            )?;
        }
        if let Some(agg) = &cov.aggregate {
            writeln!(
                out,
                "  aggregate              {:.1}% ({})",
                agg.pct, agg.method
            )?;
            writeln!(out, "  disclosure: {}", agg.disclosure)?;
        }
        writeln!(out)?;
    }

    writeln!(
        out,
        "Overall: {} ({} run, {} failed, {} skipped, {}ms)",
        verdict(report.summary.overall),
        report.summary.layers_run,
        report.summary.layers_failed,
        report.summary.layers_skipped,
        report.summary.duration_ms,
    )?;
    Ok(())
}

fn verdict(v: LayerVerdict) -> &'static str {
    match v {
        LayerVerdict::Pass => "PASS",
        LayerVerdict::Fail => "FAIL",
        LayerVerdict::Skip => "SKIP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_test_types::{
        CoverageMetric, CoverageReport, CoverageVerdict, Layer, LayerResult, RunSummary,
    };

    fn sample() -> RunReport {
        RunReport {
            schema_version: 1,
            result: LayerVerdict::Pass,
            layers: vec![LayerResult {
                layer: Layer::Spec,
                runner: "lazuli-doctor".into(),
                result: LayerVerdict::Pass,
                tests_run: 3,
                tests_passed: 3,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: Some("lazuli doctor app.lzi".into()),
                duration_ms: 50,
                failures: vec![],
                runner_native_only: None,
                skip_reason: None,
            }],
            coverage: Some(CoverageReport {
                layers: vec![CoverageMetric {
                    id: "handler_go".into(),
                    covered: 7,
                    total: 9,
                    pct: 77.78,
                    verdict: CoverageVerdict::Pass,
                    source: "go-coverprofile".into(),
                    raw_file: None,
                }],
                aggregate: None,
            }),
            summary: RunSummary {
                layers_run: 1,
                layers_failed: 0,
                layers_skipped: 0,
                coverage_warnings: 0,
                coverage_blocks: 0,
                overall: LayerVerdict::Pass,
                duration_ms: 60,
            },
        }
    }

    #[test]
    fn json_renderer_writes_schema_version() {
        let report = sample();
        let mut buf = Vec::new();
        render_json(&report, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"schema_version\""));
        assert!(s.contains("\"handler_go\""));
    }

    #[test]
    fn text_renderer_contains_overall() {
        let report = sample();
        let mut buf = Vec::new();
        render_text(&report, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("Overall: PASS"));
        assert!(s.contains("handler_go"));
    }
}
