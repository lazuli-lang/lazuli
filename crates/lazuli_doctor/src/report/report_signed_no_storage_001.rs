//! REPORT-SIGNED-NO-STORAGE-001 — `visibility:signed` but the package
//! has no `object_storage` capability.
//!
//! Takes the package's declared `object_storage` capability names from
//! the doctor pipeline. When the package has none, every signed report
//! is rejected.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, FileVisibility};

/// One REPORT-SIGNED-NO-STORAGE-001 finding — a report declares
/// `visibility:signed` but the package has no `object_storage`
/// capability bound for the signed-URL backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report carrying the signed visibility without storage.
    pub report: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-SIGNED-NO-STORAGE-001";

    /// Render the "signed but no storage" message, naming the report
    /// and steering the author to declare a capability or bind storage.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_signed_no_storage_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    /// };
    /// assert!(f.message().contains("object_storage"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` declares `visibility:signed` but the package has no \
             `object_storage` capability. Declare one in `registry.lzi` or bind \
             `storage <ref>` explicitly.",
            self.report
        )
    }
}

/// Walk every report in `feature` and emit a finding for each with
/// `visibility:signed` when `object_storage_caps` is empty (the
/// package has bound no signed-URL backend).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_signed_no_storage_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a signed report and no storage");
/// let _ = check(&feature, &[], Path::new("sales.lzi"));
/// ```
pub fn check(feature: &Feature, object_storage_caps: &[String], path: &Path) -> Vec<Finding> {
    if !object_storage_caps.is_empty() {
        return Vec::new();
    }
    feature
        .reports
        .iter()
        .filter(|r| matches!(r.visibility, FileVisibility::Signed) && r.storage.is_none())
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
        Defaults, Policies, PolicyRef, QualifiedName, Report, ReportColumn, ReportColumnSource,
        ReportFormat, ReportSource,
    };

    fn mk_feature(reports: Vec<Report>) -> Feature {
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

    fn mk_signed_report() -> Report {
        Report {
            name: "r".into(),
            input: vec![],
            source: ReportSource::Query(QualifiedName {
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
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".into()),
            filename: None,
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn signed_with_no_caps_fires() {
        let feature = mk_feature(vec![mk_signed_report()]);
        let findings = check(&feature, &[], Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn signed_with_caps_does_not_fire() {
        let feature = mk_feature(vec![mk_signed_report()]);
        assert!(check(&feature, &["files".into()], Path::new("f.lzi")).is_empty());
    }
}
