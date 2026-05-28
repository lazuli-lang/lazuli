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

/// One REPORT-COLUMN-MISMATCH-001 finding — a report column references
/// `row.<field>` but the source query's projection does not include
/// that field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report carrying the offending column.
    pub report: String,
    /// Column whose `row.<field>` reference is dangling.
    pub column: String,
    /// Field name the column tried to read but the projection lacks.
    pub unresolved_field: String,
    /// Source query name (the `from <name>` reference).
    pub source_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-COLUMN-MISMATCH-001";

    /// Render the "row.field not in source projection" message, naming
    /// the report, column, field, and source query.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_column_mismatch_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    ///     column: "Revenue".into(),
    ///     unresolved_field: "revenue_cents".into(),
    ///     source_name: "list_orders".into(),
    /// };
    /// assert!(f.message().contains("revenue_cents"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` column `{}` references `row.{}` which is not in source `{}`'s projection.",
            self.report, self.column, self.unresolved_field, self.source_name
        )
    }
}

/// Walk every report in `feature` and emit a finding for each column
/// whose `row.<field>` reference doesn't appear in the source query's
/// resolved projection. Cross-feature / opaque-SQL sources are skipped
/// (the rule prefers false negatives over false positives).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_column_mismatch_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a report referencing a missing column");
/// let _ = check(&feature, Path::new("sales.lzi"));
/// ```
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

    fn mk_feature(resources: Vec<Resource>, queries: Vec<Query>, reports: Vec<Report>) -> Feature {
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
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries,
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

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<&str>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
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
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only: false,
        }
    }

    fn mk_list(name: &str) -> Query {
        Query::List(ListQuery {
            name: name.into(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })
    }

    fn mk_report(src: &str, columns: Vec<(&str, &str)>) -> Report {
        Report {
            name: "r".into(),
            input: vec![],
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
