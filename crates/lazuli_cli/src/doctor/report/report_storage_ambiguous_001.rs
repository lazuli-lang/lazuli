//! REPORT-STORAGE-AMBIGUOUS-001 — report omits `storage` and package
//! declares ≠ 1 `object_storage` capabilities; require explicit `storage <ref>`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub cap_count: usize,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-STORAGE-AMBIGUOUS-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` omits `storage` and the package declares {} `object_storage` \
             capabilities; declare `storage <ref>` explicitly.",
            self.report, self.cap_count
        )
    }
}

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
            reports,
            previous_names: vec![],
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
