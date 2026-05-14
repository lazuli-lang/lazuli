//! REPORT-SOURCE-KIND-001 — `source` resolves to `query.lookup`.
//! Only `query.list` and `query.sql` are valid report sources.
//!
//! Local-only check: looks up the source in the same feature's queries.
//! Cross-feature sources are skipped (the doctor cross-feature pass
//! handles those later in the pipeline).

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Query, ReportSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub source_name: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-SOURCE-KIND-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` source `{}` resolves to `query.lookup`; only `query.list` and \
             `query.sql` may be report sources.",
            self.report, self.source_name
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in &feature.reports {
        let ReportSource::Query(qn) = &r.source;
        if qn.feature.is_some() {
            continue; // cross-feature → cross-feature pass
        }
        let kind = feature.queries.iter().find_map(|q| match q {
            Query::List(q) if q.name == qn.name => Some("list"),
            Query::Lookup(q) if q.name == qn.name => Some("lookup"),
            Query::Sql(q) if q.name == qn.name => Some("sql"),
            _ => None,
        });
        if kind == Some("lookup") {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                report: r.name.clone(),
                source_name: qn.name.clone(),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, KeyClause, ListQuery, LookupQuery, Path as IrPath, Policies,
        PolicyRef, QualifiedName, Report, ReportColumn, ReportColumnSource, ReportFormat,
    };

    fn mk_feature(queries: Vec<Query>, reports: Vec<Report>) -> Feature {
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
            queries,
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_report(src_name: &str) -> Report {
        Report {
            name: "r".into(),
            source: ReportSource::Query(QualifiedName {
                feature: None,
                name: src_name.to_owned(),
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
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn lookup_source_fires() {
        let lookup = Query::Lookup(LookupQuery {
            name: "by_id".into(),
            params: vec![],
            keys: vec![KeyClause {
                path: IrPath::from_segments(["id".to_owned()]),
                equals: lazuli_ir::Expr::Path(IrPath::from_segments(["id".to_owned()])),
            }],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            previous_names: vec![],
            span_ref: None,
        });
        let feature = mk_feature(vec![lookup], vec![mk_report("by_id")]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].source_name, "by_id");
    }

    #[test]
    fn list_source_does_not_fire() {
        let list = Query::List(ListQuery {
            name: "list".into(),
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            previous_names: vec![],
            span_ref: None,
        });
        let feature = mk_feature(vec![list], vec![mk_report("list")]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
