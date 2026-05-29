//! `error-page-*` aggregator — closed-status catalog, duplicate
//! detection, and template-resolution checks.

use std::collections::BTreeSet;
use std::path::Path;

use lazuli_ir as ir;

use crate::doctor::parsers::error_page_catalog_display;
use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity};

pub(crate) fn error_page_contract_diagnostics(app: &DoctorAppManifest) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    let app_dir = app.path.parent().unwrap_or_else(|| Path::new("."));

    for page in &app.manifest.error_pages {
        let line = error_page_line(app, page.status);
        if !ir::ERROR_PAGE_STATUS_CATALOG.contains(&page.status) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "error-page-contract".to_owned(),
                message: format!(
                    "`error_page {}` is outside the closed status catalog: {}.",
                    page.status,
                    error_page_catalog_display()
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        if !seen.insert(page.status) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "error-page-duplicate".to_owned(),
                message: format!(
                    "`error_page {}` is declared more than once in the app manifest.",
                    page.status
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        if page.template.trim().is_empty() {
            continue;
        }
        let template_path = app_dir.join(&page.template);
        if !template_path.exists() {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "error-page-template-missing".to_owned(),
                message: format!(
                    "`error_page {}` template `{}` does not resolve relative to `{}`.",
                    page.status,
                    page.template,
                    app.path.display()
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

pub(crate) fn error_page_line(app: &DoctorAppManifest, status: u16) -> usize {
    let needle = format!("error_page {status}");
    app.source
        .lines()
        .position(|line| line.trim_start() == needle)
        .map(|index| index + 1)
        .unwrap_or(1)
}
