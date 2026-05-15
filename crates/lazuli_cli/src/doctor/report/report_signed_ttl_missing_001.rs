//! REPORT-SIGNED-TTL-MISSING-001 — `visibility = signed` requires `signed_ttl`.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, FileVisibility};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-SIGNED-TTL-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` has `visibility:signed` but no `signed_ttl`. \
             Declare e.g. `signed_ttl 1h`.",
            self.report
        )
    }
}

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
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_report(vis: FileVisibility, ttl: Option<&str>) -> Report {
        Report {
            name: "r".into(),
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
