//! REPORT-FILENAME-TOKEN-UNKNOWN-001 — filename pattern uses `{token}`
//! outside the closed catalog (`format`, `ctx.now:<strftime>`,
//! `ctx.user.id`, `ctx.tenant.id`).
//!
//! Operates on `Report.filename.literal` and re-scans for placeholders
//! the analyzer dropped at lowering.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub unknown_token: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-FILENAME-TOKEN-UNKNOWN-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` filename pattern uses `{{{}}}` which is not in the closed catalog \
             `{{format}}`, `{{ctx.now:<strftime>}}`, `{{ctx.user.id}}`, `{{ctx.tenant.id}}`.",
            self.report, self.unknown_token
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in &feature.reports {
        let Some(pattern) = r.filename.as_ref() else {
            continue;
        };
        for raw in extract_placeholders(&pattern.literal) {
            if !is_known_token(&raw) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    report: r.name.clone(),
                    unknown_token: raw,
                });
            }
        }
    }
    findings
}

fn extract_placeholders(literal: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(close) = literal[i + 1..].find('}') {
                out.push(literal[i + 1..i + 1 + close].to_owned());
                i = i + 1 + close + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_known_token(raw: &str) -> bool {
    matches!(raw, "format" | "ctx.user.id" | "ctx.tenant.id")
        || raw.starts_with("ctx.now:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, Policies, PolicyRef, Report, ReportColumn, ReportColumnSource,
        ReportFilenamePattern, ReportFormat, ReportSource,
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
            span_ref: None,
        }
    }

    fn mk_report_with_filename(literal: &str) -> Report {
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
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".into()),
            filename: Some(ReportFilenamePattern {
                literal: literal.to_owned(),
                tokens: vec![],
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn unknown_token_fires() {
        let feature = mk_feature(vec![mk_report_with_filename("a_{bogus}.csv")]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unknown_token, "bogus");
    }

    #[test]
    fn known_tokens_do_not_fire() {
        let feature = mk_feature(vec![mk_report_with_filename(
            "a_{ctx.now:yyyymm}_{format}_{ctx.user.id}.csv",
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
