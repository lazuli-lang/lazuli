//! REPORT-COLUMN-MISMATCH-001 — column refs `row.X` but `X` is not in
//! the source query's projection.
//!
//! Local-only resolution: looks up the source query in the same feature
//! and, when it resolves to a `query.list` or `query.sql` whose record
//! shape is discoverable (via underlying resource fields or `record`
//! declarations), cross-checks `row.<field>` against that shape.
//!
//! Conservative: skips checks when projection cannot be resolved
//! locally (cross-feature sources, opaque SQL returns, etc.). False
//! negatives are preferred over false positives.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Query, Record, ReportColumnSource, ReportSource, Resource, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub column: String,
    pub unresolved_field: String,
    pub source_name: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-COLUMN-MISMATCH-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` column `{}` references `row.{}` which is not in source `{}`'s projection.",
            self.report, self.column, self.unresolved_field, self.source_name
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in &feature.reports {
        let ReportSource::Query(qn) = &r.source;
        if qn.feature.is_some() {
            continue; // cross-feature → handled by another pass
        }
        let Some(projection) = resolve_local_projection(feature, &qn.name) else {
            continue;
        };
        for col in &r.columns {
            if let ReportColumnSource::RowField(field) = &col.source {
                if !projection.contains(field) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        report: r.name.clone(),
                        column: col.name.clone(),
                        unresolved_field: field.clone(),
                        source_name: qn.name.clone(),
                    });
                }
            }
        }
    }
    findings
}

fn resolve_local_projection(feature: &Feature, query_name: &str) -> Option<HashSet<String>> {
    let query = feature.queries.iter().find(|q| q.name() == query_name)?;
    match query {
        Query::List(_) => {
            // `query.list <name>` projects the matching resource. Best-effort:
            // the feature usually owns one resource; we cross-check the
            // resource whose name matches a singular/Pascal of the query's
            // family. Fall back to any single resource declared.
            let resource = pick_resource(feature)?;
            Some(resource_field_names(resource))
        }
        Query::Sql(q) => projection_from_type_ref(feature, &q.returns),
        Query::Lookup(_) => None,
    }
}

fn pick_resource(feature: &Feature) -> Option<&Resource> {
    if feature.resources.len() == 1 {
        return feature.resources.first();
    }
    None
}

fn resource_field_names(resource: &Resource) -> HashSet<String> {
    resource.fields.iter().map(|f| f.name.clone()).collect()
}

fn projection_from_type_ref(feature: &Feature, ty: &TypeRef) -> Option<HashSet<String>> {
    match ty {
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) if qn.feature.is_none() => {
            if let Some(rec) = feature.records.iter().find(|r| r.name == qn.name) {
                return Some(record_field_names(rec));
            }
            if let Some(res) = feature.resources.iter().find(|r| r.name == qn.name) {
                return Some(resource_field_names(res));
            }
            None
        }
        _ => None,
    }
}

fn record_field_names(record: &Record) -> HashSet<String> {
    record.fields.iter().map(|f| f.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Field, FieldConstraints, FileVisibility, ListQuery, Policies,
        PolicyRef, QualifiedName, Report, ReportColumn, ReportColumnSource, ReportFormat,
    };

    fn mk_feature(
        resources: Vec<Resource>,
        queries: Vec<Query>,
        reports: Vec<Report>,
    ) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources,
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
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<&str>) -> Resource {
        Resource {
            name: name.into(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: fields.into_iter().map(mk_field).collect(),
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
        }
    }

    fn mk_list(name: &str) -> Query {
        Query::List(ListQuery {
            name: name.into(),
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
        })
    }

    fn mk_report(src: &str, columns: Vec<(&str, &str)>) -> Report {
        Report {
            name: "r".into(),
            source: ReportSource::Query(QualifiedName {
                feature: None,
                name: src.into(),
            }),
            columns: columns
                .into_iter()
                .map(|(name, field)| ReportColumn {
                    name: name.into(),
                    source: ReportColumnSource::RowField(field.into()),
                    label: None,
                    format: None,
                    span_ref: None,
                })
                .collect(),
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
    fn missing_field_fires() {
        let feature = mk_feature(
            vec![mk_resource("Customer", vec!["id", "name"])],
            vec![mk_list("list")],
            vec![mk_report("list", vec![("ghost", "ghost_field")])],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_field, "ghost_field");
    }

    #[test]
    fn known_field_does_not_fire() {
        let feature = mk_feature(
            vec![mk_resource("Customer", vec!["id", "name"])],
            vec![mk_list("list")],
            vec![mk_report("list", vec![("name", "name")])],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
