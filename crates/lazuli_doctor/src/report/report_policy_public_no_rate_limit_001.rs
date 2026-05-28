//! REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001 — `policy` includes `@scope.public`
//! but no `rate_limit` declared. Public reports are an obvious DoS vector;
//! a quota is mandatory.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PolicyRef};

/// One REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001 finding — a report's
/// policy admits `@scope.public` without an explicit `rate_limit`.
/// Public reports are an obvious DoS vector; a quota is mandatory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report missing the rate limit.
    pub report: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001";

    /// Render the "public report without rate limit" message naming
    /// the report. The text is short — the remediation is unambiguous.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_policy_public_no_rate_limit_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    /// };
    /// assert!(f.message().contains("rate_limit"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` policy includes `@scope.public` but no `rate_limit` is declared. \
             Public-scope reports require an explicit quota.",
            self.report
        )
    }
}

fn is_public_policy(policy: &PolicyRef) -> bool {
    match policy {
        PolicyRef::Atom(s) => s.contains("@scope.public"),
        // `Local` / `External` policy categories — we cannot resolve
        // their atoms here without the policies-block context. Doctor's
        // full pipeline expands these elsewhere; v0.2 of the local rule
        // catches the inline-atom form, which is the dominant shape.
        _ => false,
    }
}

/// Walk every report in `feature` and emit a finding for each whose
/// policy admits `@scope.public` but lacks a `rate_limit`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_policy_public_no_rate_limit_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a public report without a rate limit");
/// let _ = check(&feature, Path::new("sales.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .reports
        .iter()
        .filter(|r| is_public_policy(&r.policy) && r.rate_limit.is_none())
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
        Defaults, FileVisibility, Policies, QualifiedName, Report, ReportColumn,
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

    fn mk_report(policy: PolicyRef, rate_limit: Option<&str>) -> Report {
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
            policy,
            policy_expr: None,
            // `ir-rate-limit-env-aware` cell 1 — lift the legacy string
            // fixture parameter through `RateLimitSpec::from_default`.
            rate_limit: rate_limit
                .map(|literal| lazuli_ir::RateLimitSpec::from_default(literal.to_owned())),
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn public_without_rate_limit_fires() {
        let feature = mk_feature(vec![mk_report(
            PolicyRef::Atom("@scope.public".into()),
            None,
        )]);
        assert_eq!(check(&feature, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn public_with_rate_limit_does_not_fire() {
        let feature = mk_feature(vec![mk_report(
            PolicyRef::Atom("@scope.public".into()),
            Some("10 per hour per user"),
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn non_public_policy_skipped() {
        let feature = mk_feature(vec![mk_report(PolicyRef::Atom("@role.admin".into()), None)]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
