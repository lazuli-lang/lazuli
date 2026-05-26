//! Canonical `DoctorReport` assembly + MCP JSON surface.
//!
//! Both functions consume the diagnostic list produced by
//! [`super::DoctorPackage::diagnostics`] and lift it into a shape an
//! agent can read — `build_doctor_report` returns the typed Wave 2 +
//! Wave 6 envelope, `doctor_diagnostics_json` flattens it for the MCP
//! `tools/call doctor` handler (which does its own envelope wrapping).

use std::path::Path;

use anyhow::Result;
use lazuli_lsp::SecurityProfile;

use super::{DoctorDiagnostic, DoctorPackage, DoctorSeverity};

/// Build a canonical `DoctorReport` from `DoctorDiagnostic` list +
/// optional coverage. Wave 2 (JSON schema) + Wave 6 (coverage).
pub(super) fn build_doctor_report(
    diagnostics: &[DoctorDiagnostic],
    want_coverage: bool,
    package: &DoctorPackage,
) -> crate::doctor_report::DoctorReport {
    use crate::doctor_report::{
        DoctorReport, DoctorSummary, FindingBuilder, Severity as JsonSeverity, classify_result,
    };
    use std::collections::BTreeMap;
    let mut findings = Vec::with_capacity(diagnostics.len());
    let mut summary = DoctorSummary {
        errors: 0,
        warnings: 0,
        infos: 0,
        by_category: BTreeMap::new(),
        by_feature: BTreeMap::new(),
        by_rule: BTreeMap::new(),
    };
    for d in diagnostics {
        let sev = match d.severity {
            DoctorSeverity::Error => {
                summary.errors += 1;
                JsonSeverity::Error
            }
            DoctorSeverity::Warning => {
                summary.warnings += 1;
                JsonSeverity::Warning
            }
            DoctorSeverity::Info => {
                summary.infos += 1;
                JsonSeverity::Info
            }
            DoctorSeverity::Hint => {
                summary.infos += 1;
                JsonSeverity::Hint
            }
        };
        let category = d.category.unwrap_or_else(|| {
            lazuli_doctor::rule_category::RuleCategory::from_code_prefix(&d.code)
        });
        *summary
            .by_category
            .entry(category.as_str().to_owned())
            .or_insert(0) += 1;
        if let Some(fname) = &d.feature_name {
            *summary.by_feature.entry(fname.clone()).or_insert(0) += 1;
        }
        *summary.by_rule.entry(d.code.clone()).or_insert(0) += 1;
        let construct_json = d
            .construct
            .as_ref()
            .map(|c| crate::doctor_report::ConstructJson {
                kind: c.kind.clone(),
                name: c.name.clone(),
                feature: d.feature_name.clone(),
                policy: None,
            });
        let fix_json = d.fix.as_ref().map(|f| crate::doctor_report::FixJson {
            action: f.action.clone(),
            preview: f.preview.clone(),
            auto_applicable: f.auto_applicable,
            cli: f.cli.clone(),
        });
        let finding = FindingBuilder {
            rule: d.code.clone(),
            category,
            severity: sev,
            path: d.path.clone(),
            line: d.line,
            column: d.column,
            message: d.message.clone(),
            construct: construct_json,
            fix: fix_json,
            feature_name: d.feature_name.clone(),
        };
        findings.push(finding.build());
    }
    let result = classify_result(&summary);
    let coverage = if want_coverage {
        Some(package.coverage_report())
    } else {
        None
    };
    DoctorReport {
        schema_version: 1,
        result: result.as_str().to_string(),
        summary,
        findings,
        coverage,
    }
}

/// MCP read surface — runs the same `DoctorPackage` pipeline as
/// `doctor_command` but returns a structured JSON array of
/// diagnostics instead of printing to stdout + bailing.
///
/// Wired by `crate::cmd_mcp` for the `tools/call doctor` MCP method.
/// Wire-thin: returns `serde_json::Value` to keep
/// `DoctorPackage` / `DoctorDiagnostic` / `DoctorSeverity` private to
/// this module (the MCP catalog is closed at the JSON surface, not
/// the type surface).
///
/// Companion: `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §6.
pub(crate) fn doctor_diagnostics_json(
    input: &Path,
    security_profile: SecurityProfile,
) -> Result<serde_json::Value> {
    let package = DoctorPackage::load(input, security_profile)?;
    let diagnostics = package.diagnostics();
    let payload: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "path": d.path.display().to_string(),
                "line": d.line,
                "column": d.column,
                "severity": match d.severity {
                    DoctorSeverity::Error => "error",
                    DoctorSeverity::Warning => "warning",
                    DoctorSeverity::Info => "info",
                    DoctorSeverity::Hint => "hint",
                },
                "code": d.code,
                "message": d.message,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(payload))
}
