//! IR-driven feature collectors.
//!
//! Each function here reads `lazuli_ir::Feature` (or a slice of
//! `Tier3FeatureFacts`) and projects it into one of the doctor's
//! cross-feature maps:
//!
//! * `populate_feature_symbols_from_ir` — fills
//!   `feature_symbols: BTreeMap<String, FeatureSymbols>` with
//!   `(command_name → CommandSymbolFact)` so cross-feature command
//!   resolution diagnostics can read the policy surface text.
//! * `populate_feature_resources_from_ir` — fills
//!   `BTreeMap<feature, BTreeMap<resource, ResourceFact>>` for the
//!   `auth_diagnostics` cross-feature lookup. Field anchors come from
//!   `ResourceField.span_ref`; the resource header anchor comes from
//!   `collect_construct_lines` on the source.
//!
//! Two text walkers remain on the `.lzi` surface (no typed shape in
//! the IR for these yet):
//!
//! * `collect_feature_adapters` — harvests `extensions adapter <name>:
//!   <Type> at "..."` declarations into `feature → adapter_names`. The
//!   type contract is checked elsewhere; only the local name is
//!   stored.
//! * `collect_feature_uses` — harvests `uses <feature>, ...` into
//!   `feature → used_feature_names`. Strips the optional `version v<N>`
//!   pin per Cross-feature contracts §5.4 before comma-splitting.
//!
//! Storage-bucket fact builders (`collect_file_capability_facts` and
//! its `extract_cap_file_field_line` helper) live here because they
//! drive the row-30 `@cap.File(...)` diagnostics off the same per-file
//! source walk the other feature collectors use.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::doctor::facts::lines::collect_construct_lines;
use crate::doctor::parsers::is_lzi_path;
use crate::doctor::scanners::leading_spaces;
use crate::doctor::{
    CommandSymbolFact, DoctorFile, FeatureSymbols, FileCapabilityBinding, FileCapabilityFact,
    OperationalFacts, ResourceFact, ResourceFieldFact, SymbolFact, Tier3FeatureFacts,
    line_col_for_offset, policy_ref_surface_text,
};

pub(crate) fn populate_feature_symbols_from_ir(
    tier3_facts: &[Tier3FeatureFacts],
    feature_symbols: &mut BTreeMap<String, FeatureSymbols>,
) {
    for fact in tier3_facts {
        let symbols = feature_symbols.entry(fact.feature.clone()).or_default();
        for command in &fact.commands {
            symbols.commands.insert(
                command.name.clone(),
                CommandSymbolFact {
                    base: SymbolFact::default(),
                    policy: policy_ref_surface_text(&command.policy),
                },
            );
        }
    }
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `collect_feature_resources` text-walker. Reads typed `Feature.resources`
/// and projects each `Resource` + its fields into the `ResourceFact` /
/// `ResourceFieldFact` shape consumed by `auth_diagnostics`. The
/// resource line anchor comes from `collect_construct_lines` on the
/// file source so cross-feature anchored diagnostics still point at the
/// `resource <Name>` header.
pub(crate) fn populate_feature_resources_from_ir(
    file_path: &Path,
    file_source: &str,
    feature: &lazuli_ir::Feature,
    out: &mut BTreeMap<String, BTreeMap<String, ResourceFact>>,
) {
    if feature.resources.is_empty() {
        return;
    }
    let resource_lines = collect_construct_lines(
        file_source,
        "resource ",
        feature.resources.iter().map(|r| r.name.as_str()).collect(),
    );
    let entry = out.entry(feature.name.clone()).or_default();
    for resource in &feature.resources {
        let line = resource_lines.get(&resource.name).copied().unwrap_or(0);
        let mut fields = BTreeMap::new();
        for field in &resource.fields {
            let field_line = field
                .span_ref
                .map(|span| line_col_for_offset(file_source, span.start).0)
                .unwrap_or(line);
            fields.insert(
                field.name.clone(),
                ResourceFieldFact {
                    type_ref: field.type_ref.clone(),
                    unique: field.unique,
                    line: field_line,
                },
            );
        }
        entry.insert(
            resource.name.clone(),
            ResourceFact {
                path: file_path.to_path_buf(),
                line,
                fields,
            },
        );
    }
}

/// Harvest each feature's `extensions adapter <name>: <Type> at "..."`
/// declarations. Only the local name is stored; the type contract is
/// checked elsewhere.
pub(crate) fn collect_feature_adapters(
    file: &DoctorFile,
    out: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if !is_lzi_path(&file.path) {
        return;
    }
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed == "extensions" {
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if let Some(rest) = inner_trim.strip_prefix("adapter ") {
                    let name_segment = rest.split([':', ' ']).next().unwrap_or("").trim();
                    if !name_segment.is_empty() {
                        if let Some(feature_name) = feature.as_ref() {
                            out.entry(feature_name.clone())
                                .or_default()
                                .insert(name_segment.to_owned());
                        }
                    }
                }
                j += 1;
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Harvest each feature's `uses <feature>, <feature>, ...` declarations.
/// Cross-feature resource resolution (e.g. `auth identity Customer.email`
/// in `customer_auth uses customer`) reads this map.
pub(crate) fn collect_feature_uses(
    file: &DoctorFile,
    out: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if !is_lzi_path(&file.path) {
        return;
    }
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            if let Some(rest) = trimmed.strip_prefix("uses ") {
                if let Some(feature_name) = feature.as_ref() {
                    // Cross-feature contracts §5.4 — strip the optional
                    // trailing `version v<N>` pin BEFORE comma-splitting.
                    // The pin applies to all entries on the line, but the
                    // legacy uses-map only tracks feature names.
                    let list_part = match rest.find(" version ") {
                        Some(idx) => &rest[..idx],
                        None => rest,
                    };
                    let entry = out.entry(feature_name.clone()).or_default();
                    for token in list_part.split(',') {
                        let name = token.trim();
                        if !name.is_empty() {
                            entry.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

// ============================================================================
// Row 30 — Storage bucket cycle fact builders
//
// Five typed `@cap.File(...)` diagnostics (under
// `aggregators::correctness`) run against
// `OperationalFacts.file_capability_facts`, populated here by
// `collect_file_capability_facts`. The accompanying
// `extract_cap_file_field_line` mirrors the inline `extract_cap_file_field`
// in `main.rs` so the doctor's dependency surface stays unchanged.
// ============================================================================

pub(crate) fn collect_file_capability_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    if !is_lzi_path(&file.path) {
        return;
    }

    let mut current_feature: Option<String> = None;
    let mut current_resource: Option<(String, usize)> = None;
    let mut current_api: Option<(String, usize)> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        let line_number = index + 1;
        let column = indent + 1;

        // Top-level feature header anchors all enclosed sites.
        if indent == 0 && trimmed.starts_with("feature ") {
            current_feature = trimmed.split_whitespace().nth(1).map(str::to_owned);
            current_resource = None;
            current_api = None;
            continue;
        }

        // Resource and api headers; close on any line that retreats to
        // the header indent or shallower (matching `inspect_storage_projection`).
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_api = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_resource = None;
            continue;
        }
        if let Some((_, header_indent)) = &current_resource {
            if indent <= *header_indent {
                current_resource = None;
            }
        }
        if let Some((_, header_indent)) = &current_api {
            if indent <= *header_indent {
                current_api = None;
            }
        }

        let Some(feature) = current_feature.as_deref() else {
            continue;
        };

        // Resource field shape: `<field>: @cap.File(...)`.
        if let Some((resource, _)) = &current_resource {
            if let Some((field_name, cap_text)) = extract_cap_file_field_line(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file_cap)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    operational.file_capability_facts.push(FileCapabilityFact {
                        path: file.path.clone(),
                        line: line_number,
                        column,
                        feature: feature.to_owned(),
                        binding: FileCapabilityBinding::ResourceField {
                            resource: resource.clone(),
                            field: field_name,
                        },
                        capability: file_cap,
                    });
                }
            }
        }

        // Api output shape: `output @cap.File(...)`.
        if let Some((api, _)) = &current_api {
            if let Some(rest) = trimmed.strip_prefix("output ") {
                let rest = rest.trim();
                if rest.starts_with("@cap.File(") {
                    if let Some(close) = rest.find(')') {
                        let cap_text = &rest[..=close];
                        if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(
                            file_cap,
                        )) = lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                        {
                            operational.file_capability_facts.push(FileCapabilityFact {
                                path: file.path.clone(),
                                line: line_number,
                                column,
                                feature: feature.to_owned(),
                                binding: FileCapabilityBinding::ApiOutput { api: api.clone() },
                                capability: file_cap,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a resource-field line.
/// Mirrors `crates/lazuli_cli/src/main.rs:extract_cap_file_field` but is
/// re-implemented here to keep the doctor crate's dependency surface
/// unchanged (no new pub item needed).
pub(crate) fn extract_cap_file_field_line(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}
