//! REPORT-SIGNED-TTL-MISSING-001 — `visibility = signed` requires `signed_ttl`.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, FileVisibility};

/// One REPORT-SIGNED-TTL-MISSING-001 finding — a report with
/// `visibility:signed` doesn't declare a `signed_ttl` lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report missing the TTL.
    pub report: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-SIGNED-TTL-MISSING-001";

    /// Render the "signed visibility without TTL" message naming the
    /// report and suggesting a sample duration.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_signed_ttl_missing_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    /// };
    /// assert!(f.message().contains("signed_ttl"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` has `visibility:signed` but no `signed_ttl`. \
             Declare e.g. `signed_ttl 1h`.",
            self.report
        )
    }
}

/// Walk every report in `feature` and emit a finding for each with
/// `visibility:signed` that has no `signed_ttl`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_signed_ttl_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a signed report missing signed_ttl");
/// let _ = check(&feature, Path::new("sales.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .reports
        .iter()
        .filter(|r| matches!(r.visibility, FileVisibility::Signed) && r.signed_ttl.is_none())
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
        Defaults, Policies, PolicyRef, Report, ReportColumn, ReportColumnSource, ReportFormat,
        ReportSource,
    };

    fn mk_feature(reports: Vec<Report>) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
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

    fn mk_report(vis: FileVisibility, ttl: Option<&str>) -> Report {
        Report {
            name: "r".into(),
            input: vec![],
            source: ReportSource::Query(lazuli_ir::QualifiedName {
                feature: None,
                name: "list".into(),
            }),
            columns: vec![ReportColumn {
                name: "id".into(),
                source: ReportColumnSource::RowField("id".into()),
                label: None,
                format: None,
                span_ref: None,
            }],
            formats: vec![ReportFormat::Csv],
            storage: None,
            visibility: vis,
            signed_ttl: ttl.map(str::to_owned),
            filename: None,
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn signed_without_ttl_fires() {
        let feature = mk_feature(vec![mk_report(FileVisibility::Signed, None)]);
        assert_eq!(check(&feature, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn signed_with_ttl_does_not_fire() {
        let feature = mk_feature(vec![mk_report(FileVisibility::Signed, Some("1h"))]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
