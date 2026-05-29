//! REPORT-INPUT-UNBOUND-001 — a declared `report input` param is never
//! consumed by the `source` query (the source has no param of the same
//! name). An unused report input is almost certainly a bug: the param
//! is parsed + validated off the request but can never reach the query
//! that produces the report rows.
//!
//! Static check (W5 GAP-REPORT-01): cross-reference each report `input`
//! param name against the names of the local `source` query's params.
//! `query.list` and `query.sql` both carry `params: Vec<TypedSlot>`; an
//! input with no matching source param fires.
//!
//! Local-only: cross-feature sources are skipped (the cross-feature
//! doctor pass can't see the foreign query's params from this feature
//! view). Severity: warning — an unbound input is suspicious but does
//! not break codegen (the param simply rides the request context unused).

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Query, ReportSource};

/// One REPORT-INPUT-UNBOUND-001 finding — a report `input` param has no
/// matching param on the resolved local source query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report carrying the unbound input.
    pub report: String,
    /// Local source query the report binds to.
    pub source_name: String,
    /// Declared input param that the source query never consumes.
    pub param: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-INPUT-UNBOUND-001";

    /// Render the "input param not consumed by source" message, naming
    /// the report, the param, and the source query.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_input_unbound_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     report: "billing_summary".into(),
    ///     source_name: "billing_rows".into(),
    ///     param: "period_start".into(),
    /// };
    /// assert!(f.message().contains("period_start"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` declares input param `{}` but its source query `{}` has no param of \
             that name; the param is parsed off the request yet never reaches the source. \
             Add a matching param to `{}` or drop the unused input.",
            self.report, self.param, self.source_name, self.source_name
        )
    }
}

/// Walk every report in `feature` and emit a finding for each declared
/// `input` param whose name has no matching param on the resolved local
/// source query. Cross-feature sources are skipped (handled — or rather
/// deferred — by the cross-feature doctor pass).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_input_unbound_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with an unbound report input");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in &feature.reports {
        if r.input.is_empty() {
            continue;
        }
        let ReportSource::Query(qn) = &r.source;
        if qn.feature.is_some() {
            continue; // cross-feature → can't resolve foreign params here
        }
        let Some(source_params) = local_query_param_names(feature, &qn.name) else {
            // Source query not found locally (typo, or resolved by a
            // later pass). REPORT-COLUMN-MISMATCH / cross-feature own
            // that diagnostic; we only fire when we can prove the
            // mismatch against a resolved local query.
            continue;
        };
        for slot in &r.input {
            if !source_params.iter().any(|p| p == &slot.name) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    report: r.name.clone(),
                    source_name: qn.name.clone(),
                    param: slot.name.clone(),
                });
            }
        }
    }
    findings
}

/// Collect the param names of a local `query.list` / `query.sql` by
/// name. Returns `None` when no local query matches (so the caller can
/// skip rather than false-fire). `query.lookup` is not a legal report
/// source (REPORT-SOURCE-KIND-001 owns that), so its params are ignored.
fn local_query_param_names(feature: &Feature, name: &str) -> Option<Vec<String>> {
    feature.queries.iter().find_map(|q| match q {
        Query::List(q) if q.name == name => {
            Some(q.params.iter().map(|p| p.name.clone()).collect())
        }
        Query::Sql(q) if q.name == name => Some(q.params.iter().map(|p| p.name.clone()).collect()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, FileVisibility, ListQuery, Policies, PolicyRef, QualifiedName,
        Report, ReportColumn, ReportColumnSource, ReportFormat, TypeRef, TypedSlot,
    };

    fn mk_feature(queries: Vec<Query>, reports: Vec<Report>) -> Feature {
        Feature {
            name: "billing".into(),
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

    fn slot(name: &str) -> TypedSlot {
        TypedSlot {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Date),
            required: true,
            constraints: Default::default(),
            validate_skip: false,
        }
    }

    fn list_query(name: &str, params: Vec<&str>) -> Query {
        Query::List(ListQuery {
            name: name.into(),
            public_contract: None,
            params: params.into_iter().map(slot).collect(),
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

    fn mk_report(source: QualifiedName, input: Vec<TypedSlot>) -> Report {
        Report {
            name: "billing_summary".into(),
            input,
            source: ReportSource::Query(source),
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

    fn local(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    #[test]
    fn unbound_input_fires() {
        // Source query has only `period_start`; report declares an
        // extra `period_end` that the source can't consume.
        let q = list_query("billing_rows", vec!["period_start"]);
        let report = mk_report(local("billing_rows"), vec![slot("period_start"), slot("period_end")]);
        let feature = mk_feature(vec![q], vec![report]);
        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].param, "period_end");
        assert_eq!(findings[0].source_name, "billing_rows");
        assert_eq!(Finding::CODE, "REPORT-INPUT-UNBOUND-001");
    }

    #[test]
    fn all_inputs_bound_does_not_fire() {
        let q = list_query("billing_rows", vec!["period_start", "period_end"]);
        let report = mk_report(local("billing_rows"), vec![slot("period_start"), slot("period_end")]);
        let feature = mk_feature(vec![q], vec![report]);
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn no_input_block_does_not_fire() {
        let q = list_query("billing_rows", vec![]);
        let report = mk_report(local("billing_rows"), vec![]);
        let feature = mk_feature(vec![q], vec![report]);
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn cross_feature_source_is_skipped() {
        // Foreign source params aren't visible from this feature view;
        // skip rather than false-fire.
        let report = mk_report(
            QualifiedName {
                feature: Some("ledger".into()),
                name: "rows".into(),
            },
            vec![slot("period_start")],
        );
        let feature = mk_feature(vec![], vec![report]);
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn unknown_local_source_is_skipped() {
        // No local query named `missing` — defer to other rules.
        let report = mk_report(local("missing"), vec![slot("period_start")]);
        let feature = mk_feature(vec![], vec![report]);
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }
}
