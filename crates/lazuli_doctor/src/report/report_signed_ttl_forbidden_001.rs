//! REPORT-SIGNED-TTL-FORBIDDEN-001 — `signed_ttl` declared with
//! `visibility:public|private`. Signed-only.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, FileVisibility};

/// One REPORT-SIGNED-TTL-FORBIDDEN-001 finding — a report sets
/// `signed_ttl` but its `visibility` is `public` or `private`. The
/// TTL knob only pairs with `visibility:signed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report carrying the forbidden TTL.
    pub report: String,
    /// The visibility token in effect (`"public"` or `"private"`).
    pub visibility: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-SIGNED-TTL-FORBIDDEN-001";

    /// Render the "signed_ttl on non-signed visibility" message,
    /// naming the report and the actual visibility token.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_signed_ttl_forbidden_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    ///     visibility: "public".into(),
    /// };
    /// assert!(f.message().contains("public"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` has `signed_ttl` but `visibility:{}`. `signed_ttl` only \
             pairs with `visibility:signed`.",
            self.report, self.visibility
        )
    }
}

fn visibility_token(v: FileVisibility) -> &'static str {
    match v {
        FileVisibility::Public => "public",
        FileVisibility::Private => "private",
        FileVisibility::Signed => "signed",
    }
}

/// Walk every report in `feature` and emit a finding for each that
/// sets `signed_ttl` on a non-signed visibility.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_signed_ttl_forbidden_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a public report carrying signed_ttl");
/// let _ = check(&feature, Path::new("sales.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .reports
        .iter()
        .filter(|r| !matches!(r.visibility, FileVisibility::Signed) && r.signed_ttl.is_some())
        .map(|r| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            report: r.name.clone(),
            visibility: visibility_token(r.visibility).to_owned(),
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
    fn public_with_ttl_fires() {
        let feature = mk_feature(vec![mk_report(FileVisibility::Public, Some("1h"))]);
        assert_eq!(check(&feature, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn signed_with_ttl_does_not_fire() {
        let feature = mk_feature(vec![mk_report(FileVisibility::Signed, Some("1h"))]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
