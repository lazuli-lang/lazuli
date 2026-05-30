//! `lazuli doctor --self` entrypoint.
//!
//! Runs the framework-internal hygiene rules (file size, undocumented
//! public items, missing examples, test pairing) against the Lazuli
//! framework workspace itself. Wired from `doctor_command_with_options`
//! when `--self` is passed; emits text or JSON via the same
//! `--format` / `--fail-on` knobs as the main command.
//!
//! Extracted from `doctor/mod.rs` as part of the rails-style R3-C cut
//! (CLI-entrypoint bodies live in named files, not in mod.rs).

use std::path::Path;

use anyhow::{Context, Result, bail};
use lazuli_doctor_run::entry_support::{
    resolve_error_handling_severity, resolve_internal_hygiene_severity,
};
use lazuli_doctor_run::{DoctorDiagnostic, DoctorSeverity};
use lazuli_manifest::lazurite_manifest;

use super::runtime_options::DoctorRuntimeOptions;
use super::{parse_doctor_format, parse_fail_on_specs};

pub(super) fn doctor_self_command(input: &Path, opts: &DoctorRuntimeOptions) -> Result<()> {
    use lazuli_doctor::internal_hygiene::preset::InternalHygienePreset;
    use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
    use lazuli_doctor::internal_hygiene::{
        file_size_001, no_example_001, test_pairing_001, undoc_pub_001,
    };

    let workspace_root = if input.as_os_str().is_empty() {
        std::env::current_dir().context("failed to determine current directory")?
    } else {
        input.to_path_buf()
    };

    let manifest = lazurite_manifest::load(&workspace_root).ok().flatten();
    let preset = manifest
        .as_ref()
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.internal_hygiene.as_ref())
        .and_then(|ih| ih.preset.as_deref())
        .and_then(InternalHygienePreset::parse);
    let error_handling_preset = manifest
        .as_ref()
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.error_handling.as_ref())
        .and_then(|eh| eh.preset.as_deref())
        .and_then(lazuli_doctor::error_handling::preset::ErrorHandlingPreset::parse);

    let files = walk_workspace_rust_sources(&workspace_root);
    if files.is_empty() {
        bail!(
            "{} contains no `crates/lazuli_*/src/` Rust sources; `--self` is only valid \
             from the Lazuli framework workspace root",
            workspace_root.display()
        );
    }

    let mut diagnostics: Vec<DoctorDiagnostic> = Vec::new();

    // INTERNAL-FILE-SIZE-001 — per-finding tier-aware default severity.
    for finding in file_size_001::check(&files) {
        let default = match finding.tier {
            file_size_001::Tier::Warn => DoctorSeverity::Warning,
            file_size_001::Tier::Error => DoctorSeverity::Error,
        };
        let severity =
            resolve_internal_hygiene_severity(default, file_size_001::Finding::CODE, preset);
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: file_size_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-UNDOC-PUB-001 — default Warning.
    for finding in undoc_pub_001::check(&files) {
        let severity = resolve_internal_hygiene_severity(
            DoctorSeverity::Warning,
            undoc_pub_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: undoc_pub_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-NO-EXAMPLE-001 — default Info (soft during W5 sweep).
    for finding in no_example_001::check(&files) {
        let severity = resolve_internal_hygiene_severity(
            DoctorSeverity::Info,
            no_example_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: no_example_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-TEST-PAIRING-001 — default Warning.
    for finding in test_pairing_001::check(&files) {
        let severity = resolve_internal_hygiene_severity(
            DoctorSeverity::Warning,
            test_pairing_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: test_pairing_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-PANIC-UNWRAP-001 — default Warning. Iron-hand 4th
    // dimension. Scans framework Rust source for panic-prone constructs
    // outside test contexts.
    for finding in lazuli_doctor::error_handling::panic_unwrap_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            lazuli_doctor::error_handling::panic_unwrap_001::Finding::CODE,
            error_handling_preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: lazuli_doctor::error_handling::panic_unwrap_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-ERROR-NAMING-001 — default Warning. Pub types deriving
    // `thiserror::Error` whose identifier doesn't end in `Error`.
    for finding in lazuli_doctor::error_handling::error_naming_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            lazuli_doctor::error_handling::error_naming_001::Finding::CODE,
            error_handling_preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: lazuli_doctor::error_handling::error_naming_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-ERROR-NON-EXHAUSTIVE-001 — default Warning. Pub error
    // enums deriving `thiserror::Error` without `#[non_exhaustive]`.
    for finding in lazuli_doctor::error_handling::error_non_exhaustive_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            lazuli_doctor::error_handling::error_non_exhaustive_001::Finding::CODE,
            error_handling_preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: lazuli_doctor::error_handling::error_non_exhaustive_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // INTERNAL-ERROR-VARIANT-DOC-001 — default Warning. Each variant
    // of a pub `thiserror::Error` enum must carry either a `///` doc
    // or a `#[error("...")]` attribute (preferably both).
    for finding in lazuli_doctor::error_handling::error_variant_doc_001::check(&files) {
        let severity = resolve_error_handling_severity(
            DoctorSeverity::Warning,
            lazuli_doctor::error_handling::error_variant_doc_001::Finding::CODE,
            error_handling_preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity,
            code: lazuli_doctor::error_handling::error_variant_doc_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    let has_error = diagnostics
        .iter()
        .any(|d| d.severity == DoctorSeverity::Error);

    // Emit in text or JSON per --format.
    let format = parse_doctor_format(opts.format.as_deref());
    if matches!(
        format,
        crate::doctor_report::DoctorFormat::Json | crate::doctor_report::DoctorFormat::Ndjson
    ) {
        // Reuse build_doctor_report logic by constructing a synthetic
        // DoctorPackage path — but for self-audit, we serialize a
        // lightweight payload directly with summary by_rule + findings.
        let mut by_severity = (0usize, 0usize, 0usize);
        let mut by_rule = std::collections::BTreeMap::<String, usize>::new();
        for d in &diagnostics {
            *by_rule.entry(d.code.clone()).or_insert(0) += 1;
            match d.severity {
                DoctorSeverity::Error => by_severity.0 += 1,
                DoctorSeverity::Warning => by_severity.1 += 1,
                _ => by_severity.2 += 1,
            }
        }
        let report = serde_json::json!({
            "schema_version": 1,
            "self_audit": true,
            "summary": {
                "errors": by_severity.0,
                "warnings": by_severity.1,
                "infos": by_severity.2,
                "by_category": by_category_counts(&diagnostics),
                "by_rule": by_rule,
            },
            "findings": diagnostics.iter().map(|d| serde_json::json!({
                "code": d.code,
                "severity": format!("{:?}", d.severity).to_lowercase(),
                "path": d.path.display().to_string(),
                "line": d.line,
                "message": d.message,
            })).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize self-audit DoctorReport JSON")?
        );
    } else {
        for d in &diagnostics {
            d.print();
        }
        println!(
            "\n{} self-audit: {} files scanned, {} errors, {} warnings, {} infos",
            workspace_root.display(),
            files.len(),
            diagnostics
                .iter()
                .filter(|d| d.severity == DoctorSeverity::Error)
                .count(),
            diagnostics
                .iter()
                .filter(|d| d.severity == DoctorSeverity::Warning)
                .count(),
            diagnostics
                .iter()
                .filter(|d| matches!(d.severity, DoctorSeverity::Info | DoctorSeverity::Hint))
                .count(),
        );
    }

    // --fail-on category:<name> gate (internal_hygiene + error_handling).
    let specs =
        parse_fail_on_specs(&opts.fail_on).map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
    let gate_fail = !specs.is_empty()
        && specs.iter().any(|s| match s {
            crate::doctor_report::FailOnSpec::Category(c) => diagnostics
                .iter()
                .any(|d| lazuli_doctor::RuleCategory::from_code_prefix(&d.code) == *c),
            crate::doctor_report::FailOnSpec::Rule(r) => diagnostics.iter().any(|d| &d.code == r),
            crate::doctor_report::FailOnSpec::Severity(_) => has_error,
        });

    if has_error || gate_fail {
        bail!(
            "{} failed Lazuli doctor self-audit (internal_hygiene + error_handling)",
            workspace_root.display()
        );
    }

    Ok(())
}

/// Bucket findings into a `{ category: count }` map for the JSON
/// summary. Each diagnostic's category is derived from its rule code
/// prefix via [`lazuli_doctor::RuleCategory::from_code_prefix`].
fn by_category_counts(diagnostics: &[DoctorDiagnostic]) -> serde_json::Value {
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for d in diagnostics {
        let cat = lazuli_doctor::RuleCategory::from_code_prefix(&d.code).as_str();
        *counts.entry(cat).or_insert(0) += 1;
    }
    serde_json::to_value(counts).unwrap_or_else(|_| serde_json::json!({}))
}
