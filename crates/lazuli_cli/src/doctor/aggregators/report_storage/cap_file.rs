//! Five `@cap.File(...)` checks against the lifted file-capability
//! facts:
//!
//! * `cap_file_visibility_undeclared`
//! * `cap_file_visibility_signed_ttl_mismatch`
//! * `cap_file_mime_family_unknown`
//! * `cap_file_size_unit_invalid`
//! * `cap_file_accept_input_output_mismatch`
//!
//! The MIME catalog (`KNOWN_MIME_FAMILIES`) is the closed top-level
//! IANA family list; subtype `*` and family `*` are accepted at the
//! shape level and emitted under the wildcard match.
//!
//! Lifted from the parent `report_storage` god-file in the rails-style
//! split.

use std::collections::BTreeMap;

use crate::doctor::parsers::{format_accept_list, format_visibility, mime_sets_intersect};
use crate::doctor::{
    DoctorDiagnostic, DoctorSeverity, FileCapabilityBinding, FileCapabilityFact, OperationalFacts,
};

/// IANA top-level MIME families recognised by Lazuli's `@cap.File(accept:)`
/// closed catalog. Subtype `*` and family `*` are also accepted at the
/// shape level, but emitted under the wildcard match.
const KNOWN_MIME_FAMILIES: &[&str] = &[
    "text",
    "image",
    "application",
    "audio",
    "video",
    "font",
    "*",
];

pub(crate) fn cap_file_storage_diagnostics(
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // (1) cap_file_visibility_undeclared — api output without `visibility:`.
    // (3) cap_file_visibility_signed_ttl_mismatch — visibility/signed_ttl
    //     coherence (api outputs and resource fields both).
    // (5) cap_file_mime_family_unknown — MIME family outside the IANA
    //     closed catalog.
    for fact in &operational.file_capability_facts {
        if matches!(fact.binding, FileCapabilityBinding::ApiOutput { .. })
            && fact.capability.visibility.is_none()
        {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_visibility_undeclared".to_owned(),
                message: format!(
                    "api `{}` output declares `@cap.File(...)` without `visibility:`; ambiguous visibility on a file URL is a security contract gap. Declare `visibility:` as `public`, `private`, or `signed`.",
                    match &fact.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    }
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        match (
            fact.capability.visibility,
            fact.capability.signed_ttl.as_deref(),
        ) {
            (Some(lazuli_ir::FileVisibility::Signed), None) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message:
                        "`@cap.File(visibility:signed)` requires `signed_ttl:<duration>` (e.g. `1h`); signed URLs without a TTL contract leak forever."
                            .to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            (Some(other), Some(_)) if !matches!(other, lazuli_ir::FileVisibility::Signed) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message: format!(
                        "`@cap.File(visibility:{})` forbids `signed_ttl`; `signed_ttl` only applies when `visibility:signed`.",
                        format_visibility(other),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            _ => {}
        }

        for mime in &fact.capability.accept {
            if !KNOWN_MIME_FAMILIES.contains(&mime.family.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Warning,
                    code: "cap_file_mime_family_unknown".to_owned(),
                    message: format!(
                        "`@cap.File(accept:{}/{}` uses unknown MIME family `{}`; known families: {}.",
                        mime.family,
                        mime.subtype,
                        mime.family,
                        KNOWN_MIME_FAMILIES.join(", "),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // (4) cap_file_size_unit_invalid — typed promotion. The IR rejects
    //     unknown units at parse time (the analyzer falls through to
    //     `UserDefined`), so any line that matched `@cap.File(...)`
    //     literally but did NOT produce a typed `FileCapability` fact
    //     is the candidate. We re-walk operational.file_capabilities
    //     (the text-pattern facts) and cross-reference; sites that
    //     have NO typed fact for the same path:line are typing
    //     failures — promote with a typed error.
    for text_fact in &operational.file_capabilities {
        let has_typed = operational
            .file_capability_facts
            .iter()
            .any(|tf| tf.path == text_fact.path && tf.line == text_fact.line);
        if !has_typed {
            diagnostics.push(DoctorDiagnostic {
                path: text_fact.path.clone(),
                line: text_fact.line,
                column: text_fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_size_unit_invalid".to_owned(),
                message:
                    "`@cap.File(max_size:<literal>)` must use a positive integer with unit `kb`, `mb`, or `gb`; the surrounding `@cap.File(...)` shape did not lower to typed IR."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // (2) cap_file_accept_input_output_mismatch — per-feature, pair
    //     resource-field `@cap.File` inputs with api-output `@cap.File`
    //     outputs and require the accept sets to intersect.
    let mut by_feature: BTreeMap<&str, (Vec<&FileCapabilityFact>, Vec<&FileCapabilityFact>)> =
        BTreeMap::new();
    for fact in &operational.file_capability_facts {
        let entry = by_feature.entry(fact.feature.as_str()).or_default();
        match fact.binding {
            FileCapabilityBinding::ResourceField { .. } => entry.0.push(fact),
            FileCapabilityBinding::ApiOutput { .. } => entry.1.push(fact),
        }
    }
    for (_, (inputs, outputs)) in by_feature {
        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }
        for output in &outputs {
            for input in &inputs {
                if !mime_sets_intersect(&output.capability.accept, &input.capability.accept) {
                    let api_name = match &output.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    };
                    let (resource_name, field_name) = match &input.binding {
                        FileCapabilityBinding::ResourceField { resource, field } => {
                            (resource.as_str(), field.as_str())
                        }
                        _ => ("<unknown>", "<unknown>"),
                    };
                    diagnostics.push(DoctorDiagnostic {
                        path: output.path.clone(),
                        line: output.line,
                        column: output.column,
                        severity: DoctorSeverity::Error,
                        code: "cap_file_accept_input_output_mismatch".to_owned(),
                        message: format!(
                            "api `{api_name}` output declares `@cap.File(accept:{})` but resource `{resource_name}.{field_name}` declares `@cap.File(accept:{})`; accept lists must intersect for the round-trip to be valid.",
                            format_accept_list(&output.capability.accept),
                            format_accept_list(&input.capability.accept),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
    }

    diagnostics
}
