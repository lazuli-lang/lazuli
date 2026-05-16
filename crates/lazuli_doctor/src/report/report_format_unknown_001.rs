//! REPORT-FORMAT-UNKNOWN-001 — `formats` token outside the closed `{csv, xlsx}` catalog.
//!
//! Operates against the AST `formats` list because the IR drops unknown
//! tokens during lowering (the parser preserves them; analyzer maps only
//! catalog entries). Doctor scans the AST text directly.

use std::path::{Path, PathBuf};

use lazuli_syntax::ReportDecl;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub unknown_format: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-FORMAT-UNKNOWN-001";

    pub fn message(&self) -> String {
        format!(
            "report `{}` declares `formats {}` which is outside the closed catalog `csv | xlsx`.",
            self.report, self.unknown_format
        )
    }
}

pub fn check(feature: &str, reports: &[ReportDecl], path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in reports {
        for token in &r.formats {
            if token != "csv" && token != "xlsx" {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.to_owned(),
                    report: r.name.clone(),
                    unknown_format: token.clone(),
                });
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_syntax::Span;

    fn mk_report(name: &str, formats: Vec<&str>) -> ReportDecl {
        ReportDecl {
            name: name.into(),
            source: "customer.query.list".into(),
            columns: vec![],
            formats: formats.into_iter().map(str::to_owned).collect(),
            storage: None,
            visibility: None,
            signed_ttl: None,
            filename: None,
            policy: None,
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span: Span::new(0, 1),
        }
    }

    #[test]
    fn unknown_format_fires() {
        let reports = vec![mk_report("r", vec!["csv", "pdf"])];
        let findings = check("customer", &reports, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unknown_format, "pdf");
    }

    #[test]
    fn known_formats_do_not_fire() {
        let reports = vec![mk_report("r", vec!["csv", "xlsx"])];
        assert!(check("customer", &reports, Path::new("f.lzi")).is_empty());
    }
}
