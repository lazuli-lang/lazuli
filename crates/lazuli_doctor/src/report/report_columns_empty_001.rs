//! REPORT-COLUMNS-EMPTY-001 — `report` declares no columns.
//!
//! Trigger: `Report.columns.is_empty()`. Severity: error.
//! See `docs/proposals/report-vocab.md` v0.2 §Diagnostic table.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One REPORT-COLUMNS-EMPTY-001 finding — a report has no entries in
/// its `columns` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report that has no columns.
    pub report: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-COLUMNS-EMPTY-001";

    /// Render the "report has no columns" message naming the report.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_columns_empty_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    /// };
    /// assert!(f.message().contains("weekly_sales"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` declares no columns. A `columns` block with at least one entry is required.",
            self.report
        )
    }
}

/// Walk every report in `feature` and emit a finding for each with an
/// empty `columns` block.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_columns_empty_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with an empty-columns report");
/// let _ = check(&feature, Path::new("sales.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .reports
        .iter()
        .filter(|r| r.columns.is_empty())
        .map(|r| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            report: r.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, Policies, PolicyRef, Report, ReportFormat, ReportSource,
    };

    fn mk_feature_with(reports: Vec<Report>) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            pollers: vec![],
            reports,
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_report(name: &str, columns: Vec<lazuli_ir::ReportColumn>) -> Report {
        Report {
            name: name.to_owned(),
            input: vec![],
            source: ReportSource::Query(lazuli_ir::QualifiedName {
                feature: None,
                name: "list".into(),
            }),
            columns,
            formats: vec![ReportFormat::Csv],
            storage: None,
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".into()),
            filename: None,
            policy: PolicyRef::Atom("@policy.read".into()),
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn empty_columns_fires() {
        let feature = mk_feature_with(vec![mk_report("empty", vec![])]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].report, "empty");
        assert_eq!(Finding::CODE, "REPORT-COLUMNS-EMPTY-001");
    }

    #[test]
    fn populated_columns_does_not_fire() {
        let col = lazuli_ir::ReportColumn {
            name: "id".into(),
            source: lazuli_ir::ReportColumnSource::RowField("id".into()),
            label: None,
            format: None,
            span_ref: None,
        };
        let feature = mk_feature_with(vec![mk_report("ok", vec![col])]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
