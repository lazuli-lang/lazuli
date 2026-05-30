//! T4 — Layer 3 (handler) coverage extraction.
//!
//! Parses Go's `-coverprofile` output (the format documented at
//! <https://go.dev/blog/cover>). Each line after the `mode:` header is
//! `<file>:<startLine>.<startCol>,<endLine>.<endCol> <numStatements> <count>`.
//! Coverage = sum of statements with `count > 0` over total statements.
//!
//! We could shell `go tool cover -func=<file>` for an authoritative
//! summary; we re-implement parsing here so the runner does not need
//! Go on PATH for coverage aggregation after `go test` has produced
//! the file. (When Go IS on PATH, `tool_cover_summary` is offered as
//! a verification helper.)

use std::path::Path;
use std::process::Command;

use crate::cmd_test_types::{CoverageMetric, CoverageVerdict};

/// Parse a `go test -coverprofile` file and return a coverage metric
/// keyed `handler_go`. Returns `None` when the file is missing or
/// empty (zero statements).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::runners::handler_coverage::parse_coverprofile;
///
/// // let metric = parse_coverprofile(Path::new("cover.out"));
/// ```
pub fn parse_coverprofile(path: &Path) -> Option<CoverageMetric> {
    let contents = std::fs::read_to_string(path).ok()?;
    let (covered, total) = parse_coverprofile_str(&contents)?;
    let pct = if total == 0 {
        0.0
    } else {
        (covered as f64 / total as f64) * 100.0
    };
    Some(CoverageMetric {
        id: "handler_go".into(),
        covered,
        total,
        pct: round2(pct),
        // Verdict is set by the aggregator once thresholds are
        // resolved; we default to pass and let it overwrite.
        verdict: CoverageVerdict::Pass,
        source: "go-coverprofile".into(),
        raw_file: Some(path.to_string_lossy().into_owned()),
    })
}

/// Pure parser, separated for unit tests. Returns
/// `(covered_statements, total_statements)`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::runners::handler_coverage::parse_coverprofile_str;
///
/// // let parsed = parse_coverprofile_str("mode: set\n...");
/// ```
pub fn parse_coverprofile_str(contents: &str) -> Option<(u64, u64)> {
    let mut total: u64 = 0;
    let mut covered: u64 = 0;
    let mut saw_mode = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("mode:") {
            saw_mode = true;
            continue;
        }
        // Format: <file>:<startLine>.<col>,<endLine>.<col> <numStmts> <count>
        let mut parts = line.rsplitn(3, ' ');
        let count = parts.next()?.parse::<i64>().ok()?;
        let num_stmts = parts.next()?.parse::<u64>().ok()?;
        // _file_block = parts.next(); — we ignore the block label
        let _ = parts.next()?;
        total += num_stmts;
        if count > 0 {
            covered += num_stmts;
        }
    }

    if !saw_mode && total == 0 {
        return None;
    }
    Some((covered, total))
}

/// Optional verification — shells `go tool cover -func=<file>` and
/// returns its stdout summary. Used by `--verbose` diagnostics and
/// CI logs; never the primary source of truth.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::runners::handler_coverage::tool_cover_summary;
///
/// // let summary = tool_cover_summary(Path::new("cover.out"));
/// ```
#[allow(dead_code)]
pub fn tool_cover_summary(path: &Path) -> Option<String> {
    let output = Command::new("go")
        .arg("tool")
        .arg("cover")
        .arg(format!("-func={}", path.display()))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "mode: set
the canonical pilot/app/features/post/handlers/create.go:10.20,15.2 4 1
the canonical pilot/app/features/post/handlers/create.go:17.20,20.2 2 0
the canonical pilot/app/features/post/handlers/update.go:5.20,8.2 3 1
";

    #[test]
    fn parses_coverprofile_lines() {
        let (covered, total) = parse_coverprofile_str(SAMPLE).unwrap();
        assert_eq!(covered, 7);
        assert_eq!(total, 9);
    }

    #[test]
    fn parses_metric_pct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cov.out");
        std::fs::write(&path, SAMPLE).unwrap();
        let metric = parse_coverprofile(&path).unwrap();
        assert_eq!(metric.id, "handler_go");
        assert_eq!(metric.covered, 7);
        assert_eq!(metric.total, 9);
        assert!((metric.pct - 77.78).abs() < 0.01);
    }

    #[test]
    fn missing_file_yields_none() {
        assert!(parse_coverprofile(Path::new("does-not-exist.cov")).is_none());
    }

    #[test]
    fn empty_no_mode_yields_none() {
        assert!(parse_coverprofile_str("").is_none());
    }

    #[test]
    fn mode_only_yields_zero_total() {
        let (c, t) = parse_coverprofile_str("mode: atomic\n").unwrap();
        assert_eq!(c, 0);
        assert_eq!(t, 0);
    }
}
