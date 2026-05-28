//! REPORT-FORMAT-UNKNOWN-001 — `formats` token outside the closed `{csv, xlsx}` catalog.
//!
//! Operates against the AST `formats` list because the IR drops unknown
//! tokens during lowering (the parser preserves them; analyzer maps only
//! catalog entries). Doctor scans the AST text directly.

use std::path::{Path, PathBuf};

use lazuli_syntax::ReportDecl;

/// One REPORT-FORMAT-UNKNOWN-001 finding — a report declares a
/// `formats` token outside the closed `{csv, xlsx}` catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the report was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Report carrying the unknown format token.
    pub report: String,
    /// Verbatim token authored in the `formats` list.
    pub unknown_format: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "REPORT-FORMAT-UNKNOWN-001";

    /// Render the "report declares unknown format" message naming the
    /// report and the offending token. The closed catalog is named in
    /// the text so the author sees the allowed set inline.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::report::report_format_unknown_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("sales.lzi"),
    ///     feature: "sales".into(),
    ///     report: "weekly_sales".into(),
    ///     unknown_format: "pdf".into(),
    /// };
    /// assert!(f.message().contains("pdf"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "report `{}` declares `formats {}` which is outside the closed catalog `csv | xlsx`.",
            self.report, self.unknown_format
        )
    }
}

/// Scan AST report declarations directly (the IR drops unknown tokens
/// at lowering) and emit a finding for each `formats` entry outside
/// the closed `csv | xlsx` catalog.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::report::report_format_unknown_001::check;
/// use lazuli_syntax::ReportDecl;
///
/// let reports: Vec<ReportDecl> = vec![];
/// let _ = check("sales", &reports, Path::new("sales.lzi"));
/// ```
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
            input: vec![],
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
