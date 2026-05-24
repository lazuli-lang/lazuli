//! Structured doctor reports — JSON / NDJSON / text rendering layers over
//! the same payload.
//!
//! Wave 2 of the TDD/BDD-first proposal (2026-05-23). This module is the
//! agent-first surface: every signal a Lazuli developer might consume is
//! emitted here as canonical JSON. Text rendering, colored output, and the
//! NDJSON streaming watcher all delegate to the same `DoctorReport` shape.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_doctor::RuleCategory;
use serde::{Deserialize, Serialize};

/// JSON schema version. Additive-only: new fields are `Option`; existing
/// fields keep their type signature. Breaking the schema bumps this number
/// and is gated by a proposal.
pub const SCHEMA_VERSION: u32 = 1;

/// Severity tag emitted in JSON output. Mirrors the internal
/// `DoctorSeverity` enum but is serializable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }
}

/// Top-level pass/fail result. Computed from the per-finding severities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportResult {
    Pass,
    PassWithWarnings,
    Fail,
}

impl ReportResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::PassWithWarnings => "pass_with_warnings",
            Self::Fail => "fail",
        }
    }
}

/// Span over the source range of a finding. End coordinates are optional —
/// most rules today emit a single anchor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanJson {
    pub line: usize,
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

/// Construct facts attached to a finding when the rule can identify the
/// authoring site (`command foo`, `view bar`, `transition baz`). Optional —
/// many rules fire at file scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructJson {
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
}

/// Suggested fix payload. `auto_applicable` discriminates between fixes the
/// agent can blindly apply (`lazuli fix --apply`) and judgement-heavy
/// suggestions (`auto_applicable: false`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixJson {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    pub auto_applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
}

/// IDE / CLI grouping bucket. Mirrors the text rendering's category banners.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupJson {
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub construct_kind: Option<String>,
}

/// One row in `findings[]`. Mirrors the LSP `Diagnostic.data` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingJson {
    pub rule: String,
    pub category: String,
    pub severity: String,
    pub path: String,
    pub span: SpanJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub construct: Option<ConstructJson>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<FixJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupJson>,
}

/// Aggregate counts. Each `by_*` map is sorted (BTreeMap) for deterministic
/// output — agents diff doctor runs across commits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoctorSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub by_category: BTreeMap<String, usize>,
    pub by_feature: BTreeMap<String, usize>,
    pub by_rule: BTreeMap<String, usize>,
}

/// Top-level structured report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub result: String,
    pub summary: DoctorSummary,
    pub findings: Vec<FindingJson>,
    /// Wave 6 — per-layer coverage report when `--coverage` was passed.
    /// Uses the canonical `lazuli_doctor::coverage::CoverageReport` shape.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub coverage: Option<lazuli_doctor::coverage::CoverageReport>,
}

impl DoctorReport {
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            result: ReportResult::Pass.as_str().to_string(),
            summary: DoctorSummary::default(),
            findings: Vec::new(),
            coverage: None,
        }
    }
}

/// Builder helper: derive a `FindingJson` from minimal inputs. Callers
/// inside `doctor.rs` use this to fill the structured surface without
/// rewriting the 60+ existing `DoctorDiagnostic` construction sites.
pub struct FindingBuilder {
    pub rule: String,
    pub category: RuleCategory,
    pub severity: Severity,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub construct: Option<ConstructJson>,
    pub fix: Option<FixJson>,
    pub feature_name: Option<String>,
}

impl FindingBuilder {
    pub fn build(self) -> FindingJson {
        let group = Some(GroupJson {
            category: self.category.as_str().to_string(),
            feature: self.feature_name.clone(),
            construct_kind: self.construct.as_ref().map(|c| c.kind.clone()),
        });
        FindingJson {
            rule: self.rule,
            category: self.category.as_str().to_string(),
            severity: self.severity.as_str().to_string(),
            path: self.path.display().to_string(),
            span: SpanJson {
                line: self.line,
                column: self.column,
                end_line: None,
                end_column: None,
            },
            construct: self.construct,
            message: self.message,
            fix: self.fix,
            group,
        }
    }
}

/// Compute `result` field from summary counts.
pub fn classify_result(summary: &DoctorSummary) -> ReportResult {
    if summary.errors > 0 {
        ReportResult::Fail
    } else if summary.warnings > 0 {
        ReportResult::PassWithWarnings
    } else {
        ReportResult::Pass
    }
}

/// Filter spec for `--fail-on`. Composable: multiple specs combine with OR.
#[derive(Debug, Clone)]
pub enum FailOnSpec {
    Severity(Severity),
    Category(RuleCategory),
    Rule(String),
}

impl FailOnSpec {
    pub fn parse(input: &str) -> Result<Self, String> {
        if let Some(cat) = input.strip_prefix("category:") {
            return RuleCategory::parse(cat.trim())
                .map(Self::Category)
                .ok_or_else(|| format!("unknown category in --fail-on: {cat}"));
        }
        if let Some(rule) = input.strip_prefix("rule:") {
            return Ok(Self::Rule(rule.trim().to_string()));
        }
        match input.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Severity(Severity::Error)),
            "warning" => Ok(Self::Severity(Severity::Warning)),
            "info" => Ok(Self::Severity(Severity::Info)),
            "hint" => Ok(Self::Severity(Severity::Hint)),
            other => Err(format!(
                "unknown --fail-on value: {other} (expected severity, category:X, or rule:X)"
            )),
        }
    }

    pub fn matches(&self, finding: &FindingJson) -> bool {
        match self {
            Self::Severity(s) => finding.severity == s.as_str(),
            Self::Category(c) => finding.category == c.as_str(),
            Self::Rule(r) => &finding.rule == r,
        }
    }
}

/// Returns true if the report contains at least one finding matching any
/// of the specs. Empty `specs` means "fail on error" (default).
pub fn report_fails_gate(report: &DoctorReport, specs: &[FailOnSpec]) -> bool {
    if specs.is_empty() {
        return report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Error.as_str());
    }
    report
        .findings
        .iter()
        .any(|f| specs.iter().any(|s| s.matches(f)))
}

/// Rendering format selector for the CLI `--format` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DoctorFormat {
    /// Auto-detect: `text` when stdout is a TTY, `json` otherwise.
    Auto,
    Text,
    Json,
    Ndjson,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fail_on_severity() {
        match FailOnSpec::parse("warning").unwrap() {
            FailOnSpec::Severity(Severity::Warning) => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_fail_on_category() {
        match FailOnSpec::parse("category:TestDiscipline").unwrap() {
            FailOnSpec::Category(RuleCategory::TestDiscipline) => {}
            _ => panic!("wrong variant"),
        }
        match FailOnSpec::parse("category:test_discipline").unwrap() {
            FailOnSpec::Category(RuleCategory::TestDiscipline) => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_fail_on_rule() {
        match FailOnSpec::parse("rule:TEST-MISSING-AUTHORED-001").unwrap() {
            FailOnSpec::Rule(r) => assert_eq!(r, "TEST-MISSING-AUTHORED-001"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_fail_on_rejects_unknown() {
        assert!(FailOnSpec::parse("bogus").is_err());
        assert!(FailOnSpec::parse("category:unknown").is_err());
    }

    #[test]
    fn report_gate_default_is_error() {
        let mut report = DoctorReport::empty();
        report.findings.push(FindingJson {
            rule: "X".into(),
            category: "Other".into(),
            severity: "warning".into(),
            path: "p".into(),
            span: SpanJson { line: 1, column: 1, end_line: None, end_column: None },
            construct: None,
            message: "m".into(),
            fix: None,
            group: None,
        });
        // Empty specs + only warnings → does NOT gate.
        assert!(!report_fails_gate(&report, &[]));
        // category:Other → gates.
        let specs = vec![FailOnSpec::Category(RuleCategory::Other)];
        assert!(report_fails_gate(&report, &specs));
    }
}
