//! REPORT-STORAGE-AMBIGUOUS-001 — report omits `storage` and package
//! declares ≠ 1 `object_storage` capabilities; require explicit `storage <ref>`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One REPORT-STORAGE-AMBIGUOUS-001 finding — a report omits its
/// `storage` reference and the package declares a number of
/// `object_storage` capabilities other than exactly one (so the
/// implicit default is ambiguous).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report missing the storage reference.
    pub report: String,
    /// Number of `object_storage` capabilities the package declares.
    pub cap_count: usize,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-STORAGE-AMBIGUOUS-001";

    /// Render the "report storage ambiguous" message, naming the
    /// report and the count of candidates.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_storage_ambiguous_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    ///     cap_count: 2,
    /// };
    /// assert!(f.message().contains("object_storage"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` omits `storage` and the package declares {} `object_storage` \
             capabilities; declare `storage <ref>` explicitly.",
            self.report, self.cap_count
        )
    }
}

/// Walk every report in `feature` and emit a finding when storage is
/// omitted but the package has any number of `object_storage`
/// capabilities other than exactly one.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_storage_ambiguous_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with reports + multiple object_storage caps");
/// let _ = check(&feature, &[], Path::new("sales.lzi"));
/// ```
pub fn check(
    feature: &Feature,
    object_storage_caps: &[String],
    path: &Path,
) -> Vec<Finding> {
    if object_storage_caps.len() == 1 {
        return Vec::new();
    }
    feature
        .reports
        .iter()
        .filter(|r| r.storage.is_none())
        .map(|r| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            report: r.name.clone(),
            cap_count: object_storage_caps.len(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, Policies, PolicyRef, QualifiedName, Report, ReportColumn,
        ReportColumnSource, ReportFormat, ReportSource,
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

    fn mk_report(storage: Option<&str>) -> Report {
        Report {
            name: "r".into(),
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
            storage: storage.map(|s| QualifiedName {
                feature: None,
                name: s.into(),
            }),
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
    fn implicit_with_two_caps_fires() {
        let feature = mk_feature(vec![mk_report(None)]);
        let caps = vec!["a".into(), "b".into()];
        assert_eq!(check(&feature, &caps, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn implicit_with_single_cap_does_not_fire() {
        let feature = mk_feature(vec![mk_report(None)]);
        assert!(check(&feature, &["a".into()], Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn explicit_storage_does_not_fire() {
        let feature = mk_feature(vec![mk_report(Some("files"))]);
        let caps = vec!["a".into(), "b".into()];
        assert!(check(&feature, &caps, Path::new("f.lzi")).is_empty());
    }
}
