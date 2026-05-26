//! `lazuli profile <profile.pb.gz>` — group pprof samples by Lazuli labels.
//!
//! v0 shells out to `go tool pprof -raw` and parses its stable text form
//! instead of vendoring the pprof protobuf schema into the CLI crate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// Closed catalog of pprof axes `lazuli profile` rolls up by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileAxis {
    /// CPU samples.
    Cpu,
    /// Allocated-bytes samples.
    Alloc,
    /// Blocking samples.
    Block,
}

/// Full report emitted by `run_profile` — `top_ops` and
/// `top_patterns` are pre-sorted by `percent` / `total`.
#[derive(Debug)]
pub struct ProfileReport {
    /// Top ops by share of the chosen axis.
    pub top_ops: Vec<OpFrame>,
    /// Top semantic patterns aggregated across ops.
    pub top_patterns: Vec<PatternFrame>,
}

/// One op-level pprof row keyed by the closed
/// `capsule.feature.kind.op` ladder.
#[derive(Debug, serde::Serialize)]
pub struct OpFrame {
    /// Capsule the op lives in.
    pub capsule: String,
    /// Feature.
    pub feature: String,
    /// Op kind (`command`/`query`/...).
    pub kind: String,
    /// Op name.
    pub op: String,
    /// Semantic pattern observed (e.g. `n_plus_one`).
    pub pattern: String,
    /// Pattern catalog version.
    pub pattern_version: String,
    /// Share of the axis total.
    pub percent: f64,
    /// Axis unit (`samples`, `bytes`, …).
    pub units: String,
}

/// One pattern-level rollup across ops.
#[derive(Debug, serde::Serialize)]
pub struct PatternFrame {
    /// Pattern name.
    pub pattern: String,
    /// Catalog version.
    pub version: String,
    /// Total of the axis attributed to this pattern.
    pub total: f64,
    /// Axis unit.
    pub units: String,
    /// Number of distinct ops contributing to the pattern.
    pub op_count: usize,
}

#[derive(Debug, Default)]
struct Accum {
    total: f64,
    pattern: String,
    version: String,
}

/// Shell `go tool pprof -raw` on `profile_path`, parse the stable
/// text form, and roll up the samples by the chosen axis into the
/// closed `(capsule, feature, kind, op)` ladder. Returns the top-N
/// ops + the cross-op pattern aggregation.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::profile::{run_profile, ProfileAxis};
///
/// // let report = run_profile(Path::new("cpu.pprof"), 20, ProfileAxis::Cpu)?;
/// ```
pub fn run_profile(
    profile_path: &Path,
    top_n: usize,
    axis: ProfileAxis,
) -> Result<ProfileReport, Box<dyn std::error::Error>> {
    let output = Command::new("go")
        .args(["tool", "pprof", "-raw"])
        .arg(profile_path)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "go tool pprof -raw failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let raw = String::from_utf8(output.stdout)?;
    Ok(report_from_raw(&raw, top_n, axis))
}

/// Render a [`ProfileReport`] as the canonical text projection: two
/// sections (`Top ops:` then `Top patterns:`), one indented line per
/// entry. JSON output bypasses this and ships `top_ops` /
/// `top_patterns` directly.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::profile::format_report;
/// // let text = format_report(&report);
/// ```
pub fn format_report(report: &ProfileReport) -> String {
    let mut out = String::new();
    out.push_str("Top ops:\n");
    for op in &report.top_ops {
        out.push_str(&format!(
            "  {}.{}.{}.{}  {:.1}%  (pattern: {} {})\n",
            op.capsule, op.feature, op.kind, op.op, op.percent, op.pattern, op.pattern_version,
        ));
    }
    out.push_str("\nTop patterns:\n");
    for p in &report.top_patterns {
        out.push_str(&format!(
            "  {} {}   {:.1} {}  across {} ops\n",
            p.pattern, p.version, p.total, p.units, p.op_count,
        ));
    }
    out
}

fn report_from_raw(raw: &str, top_n: usize, axis: ProfileAxis) -> ProfileReport {
    let units = units_for(axis).to_owned();
    let mut ops: BTreeMap<(String, String, String, String), Accum> = BTreeMap::new();
    let mut pending_value: Option<f64> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(value) = sample_value(trimmed, axis) {
            pending_value = Some(value);
            continue;
        }
        let labels = parse_labels(trimmed);
        if labels.is_empty() {
            continue;
        }
        let Some(value) = pending_value.take() else {
            continue;
        };
        let Some(op) = labels.get("op") else {
            continue;
        };
        let key = (
            labels.get("capsule").cloned().unwrap_or_default(),
            labels.get("feature").cloned().unwrap_or_default(),
            labels.get("kind").cloned().unwrap_or_default(),
            op.clone(),
        );
        let entry = ops.entry(key).or_insert_with(|| Accum {
            pattern: labels
                .get("pattern")
                .cloned()
                .unwrap_or_else(|| "unknown".to_owned()),
            version: labels
                .get("pattern_version")
                .or_else(|| labels.get("version"))
                .cloned()
                .unwrap_or_else(|| "unknown".to_owned()),
            total: 0.0,
        });
        entry.total += value;
    }

    let total: f64 = ops.values().map(|acc| acc.total).sum();
    let mut top_ops: Vec<OpFrame> = ops
        .iter()
        .map(|((capsule, feature, kind, op), acc)| OpFrame {
            capsule: capsule.clone(),
            feature: feature.clone(),
            kind: kind.clone(),
            op: op.clone(),
            pattern: acc.pattern.clone(),
            pattern_version: acc.version.clone(),
            percent: if total > 0.0 {
                acc.total * 100.0 / total
            } else {
                0.0
            },
            units: units.clone(),
        })
        .collect();
    top_ops.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    top_ops.truncate(top_n);

    let mut pattern_totals: BTreeMap<
        (String, String),
        (f64, BTreeSet<(String, String, String, String)>),
    > = BTreeMap::new();
    for (key, acc) in &ops {
        let bucket = pattern_totals
            .entry((acc.pattern.clone(), acc.version.clone()))
            .or_insert_with(|| (0.0, BTreeSet::new()));
        bucket.0 += acc.total;
        bucket.1.insert(key.clone());
    }
    let mut top_patterns: Vec<PatternFrame> = pattern_totals
        .into_iter()
        .map(|((pattern, version), (total, op_keys))| PatternFrame {
            pattern,
            version,
            total,
            units: units.clone(),
            op_count: op_keys.len(),
        })
        .collect();
    top_patterns.sort_by(|a, b| b.total.total_cmp(&a.total));
    top_patterns.truncate(top_n);

    ProfileReport {
        top_ops,
        top_patterns,
    }
}

fn sample_value(line: &str, axis: ProfileAxis) -> Option<f64> {
    if !line.contains(':') {
        return None;
    }
    let head = line.split(':').next()?.trim();
    let nums: Vec<f64> = head
        .split_whitespace()
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();
    if nums.is_empty() {
        return None;
    }
    let idx = match axis {
        ProfileAxis::Cpu => nums.len().saturating_sub(1),
        ProfileAxis::Alloc => 0,
        ProfileAxis::Block => 0,
    };
    nums.get(idx).copied()
}

fn parse_labels(line: &str) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    for part in line.split_whitespace() {
        let Some((key, rest)) = part.split_once(':') else {
            continue;
        };
        let value = rest.trim();
        if value.starts_with('[') && value.ends_with(']') && value.len() >= 2 {
            labels.insert(key.to_owned(), value[1..value.len() - 1].to_owned());
        }
    }
    labels
}

fn units_for(axis: ProfileAxis) -> &'static str {
    match axis {
        ProfileAxis::Cpu => "ns",
        ProfileAxis::Alloc => "bytes",
        ProfileAxis::Block => "samples",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = r#"
Samples:
samples/count cpu/nanoseconds
          1 100: 1 2
                capsule:[crm] feature:[customer] kind:[command] op:[create_customer] pattern:[command_pgx_insert] pattern_version:[v1]
          1 300: 3 4
                capsule:[crm] feature:[invoice] kind:[query] op:[list] pattern:[query_pgx_list] pattern_version:[v1]
          1 100: 5 6
                capsule:[crm] feature:[order] kind:[query] op:[list] pattern:[query_pgx_list] pattern_version:[v1]
"#;

    #[test]
    fn profile_groups_frames_by_op_labels() {
        let report = report_from_raw(RAW, 2, ProfileAxis::Cpu);
        assert_eq!(report.top_ops.len(), 2);
        assert_eq!(report.top_ops[0].feature, "invoice");
        assert_eq!(report.top_ops[0].op, "list");
        assert!((report.top_ops[0].percent - 60.0).abs() < 0.01);
    }

    #[test]
    fn profile_pattern_attribution_aggregates_across_ops() {
        let report = report_from_raw(RAW, 10, ProfileAxis::Cpu);
        let pattern = report
            .top_patterns
            .iter()
            .find(|p| p.pattern == "query_pgx_list")
            .expect("query pattern");
        assert_eq!(pattern.op_count, 2);
        assert_eq!(pattern.total, 400.0);
    }

    #[test]
    fn profile_reads_frozen_pprof_fixture() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_profile.pb.gz");
        let report = run_profile(&fixture, 10, ProfileAxis::Cpu).expect("fixture profile");
        assert!(
            report
                .top_ops
                .iter()
                .any(|op| op.feature == "customer" && op.op == "create_customer"),
            "expected customer op in report: {report:?}"
        );
        assert!(
            report
                .top_patterns
                .iter()
                .any(|pattern| pattern.pattern == "query_pgx_list"),
            "expected query pattern in report: {report:?}"
        );
    }
}
