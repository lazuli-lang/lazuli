//! Audit aggregator — emits `audit emit_to`, `event.trace level`, and
//! health-probe path diagnostics, plus the `resource_policy_and_command_
//! audit_hints` cross-check that flags commands writing to resources
//! without matching audit configuration.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::doctor::parsers::{catalog_list, is_lzi_path};
use crate::doctor::scanners::leading_spaces;
use crate::doctor::{
    DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, ResourceFact,
    Tier3FeatureFacts,
};

/// Mirrors `log/slog` level discipline; kept in sync with
/// `aggregators::observability::LOG_LEVEL_CATALOG`.
const TRACE_LEVEL_CATALOG: &[&str] = &["debug", "info", "warn", "error"];
const RESERVED_AUDIT_STREAMS: &[&str] = &["audit_log", "audit_stream"];

/// Public entrypoint — audit emit_to / event.trace level / health probe
/// cross-checks.
pub(crate) fn diagnostics(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    audit_event_health_diagnostics(files, app, tier3_facts)
}

/// Public entrypoint — resource/policy/command audit-hint cross-check.
pub(crate) fn resource_policy_hints(
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    resource_policy_and_command_audit_hints(tier3_facts, feature_resources)
}

/// Phase L Tier 4b — find the `emit_to <target>` line inside the body
/// of a construct whose header is at `header_line` (1-indexed). Returns
/// `(line_1_indexed, column_1_indexed)`. Used by the IR-driven
/// `audit emit_to` walker to anchor diagnostics at the exact source
/// location even when the IR side only carries the construct header.
pub(super) fn locate_emit_to_line(
    path: &Path,
    files: &[DoctorFile],
    header_line: usize,
    target: &str,
) -> Option<(usize, usize)> {
    let file = files.iter().find(|f| f.path == path)?;
    let lines: Vec<&str> = file.source.lines().collect();
    if header_line == 0 || header_line > lines.len() {
        return None;
    }
    let header_indent = leading_spaces(lines[header_line - 1]);
    let needle = format!("emit_to {target}");
    for (offset, line) in lines.iter().enumerate().skip(header_line) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        // Stop at sibling or higher-level construct.
        if indent <= header_indent {
            return None;
        }
        if trimmed == needle || trimmed.starts_with(&needle) {
            return Some((offset + 1, indent + 1));
        }
    }
    None
}

pub(super) fn audit_event_health_diagnostics(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Phase L Tier 4b — build the feature → event-group lookup from
    // both IR (`tier3_facts`) and text-walk (for features that don't
    // lower through the canonical-indent slice). IR takes precedence
    // when a feature appears in both.
    let mut feature_event_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for fact in tier3_facts {
        let entry = feature_event_groups
            .entry(fact.feature.clone())
            .or_default();
        for group in &fact.event_groups {
            // `EventGroup.pattern` is the whole `<name> *` or `<glob>`
            // pattern as authored. `emit_to` references the first
            // whitespace token (the group's name), matching the
            // historical text-walker behaviour.
            if let Some(name) = group.pattern.split_whitespace().next() {
                entry.insert(name.to_owned());
            }
        }
    }
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let mut current_feature: Option<String> = None;
        for line in file.source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if leading == 0 {
                current_feature = trimmed
                    .strip_prefix("feature ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                continue;
            }
            if let Some(feature) = current_feature.as_ref() {
                if let Some(rest) = trimmed.strip_prefix("event_group ") {
                    if let Some(name) = rest.split_whitespace().next() {
                        feature_event_groups
                            .entry(feature.clone())
                            .or_default()
                            .insert(name.to_owned());
                    }
                }
            }
        }
    }

    // Phase L Tier 4b — IR-driven `audit emit_to` resolution for
    // commands. Walks `Command.audit.emit_to` directly; anchors the
    // diagnostic at the `emit_to <target>` line inside the command
    // body by scanning the source range starting at the command
    // header. Retires the text-walker branch for command bodies.
    let mut command_audit_keys: BTreeSet<(PathBuf, usize)> = BTreeSet::new();
    for fact in tier3_facts {
        for command in &fact.commands {
            let Some(audit) = command.audit.as_ref() else {
                continue;
            };
            let Some(target) = audit.emit_to.as_deref() else {
                continue;
            };
            let allowed_set = feature_event_groups.get(&fact.feature);
            let resolved = RESERVED_AUDIT_STREAMS.contains(&target)
                || allowed_set.is_some_and(|set| set.contains(target));
            let Some(header_line) = fact.command_lines.get(&command.name).copied() else {
                continue;
            };
            let Some((line, column)) = locate_emit_to_line(&fact.path, files, header_line, target)
            else {
                continue;
            };
            command_audit_keys.insert((fact.path.clone(), line));
            if resolved {
                continue;
            }
            let mut allowed: Vec<String> = RESERVED_AUDIT_STREAMS
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            if let Some(set) = allowed_set {
                allowed.extend(set.iter().cloned());
            }
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line,
                column,
                severity: DoctorSeverity::Error,
                code: "audit_emit_to_unknown_diagnostics".to_owned(),
                message: format!(
                    "`audit emit_to {target}` does not resolve. Allowed: {}.",
                    allowed
                        .iter()
                        .map(|s| format!("`{s}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // `audit ... emit_to <X>` text-walker for constructs whose IR
    // does not yet carry `audit` (webhook, job, poller, lifecycle
    // transition). Command bodies are skipped — the IR walker above
    // owns them. Detected duplicates against `command_audit_keys`
    // are suppressed defensively.
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut current_feature: Option<String> = None;
        let mut audit_pending: Option<(usize, usize)> = None; // (line_index, indent of audit)
        let mut in_command: Option<usize> = None; // indent of `command <name>` header
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if leading == 0 {
                current_feature = trimmed
                    .strip_prefix("feature ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                audit_pending = None;
                in_command = None;
                continue;
            }
            // Track `command <name>` headers so we can skip their
            // bodies — the IR walker handles command audit emit_to.
            if let Some(command_indent) = in_command {
                if leading <= command_indent {
                    in_command = None;
                }
            }
            if trimmed.starts_with("command ") {
                in_command = Some(leading);
                audit_pending = None;
                continue;
            }
            if in_command.is_some() {
                continue;
            }
            // Track audit headers as `audit <fields...>` or bare `audit`
            // at indent 4 or 6 (webhook/job/poller bodies).
            if trimmed == "audit" || trimmed.starts_with("audit ") {
                audit_pending = Some((i, leading));
                continue;
            }
            if let Some((_, audit_indent)) = audit_pending {
                if leading <= audit_indent {
                    audit_pending = None;
                } else if leading == audit_indent + 2 {
                    if let Some(rest) = trimmed.strip_prefix("emit_to ") {
                        let target = rest.trim();
                        let resolved = if RESERVED_AUDIT_STREAMS.contains(&target) {
                            true
                        } else if let Some(feature) = current_feature.as_ref() {
                            feature_event_groups
                                .get(feature)
                                .is_some_and(|set| set.contains(target))
                        } else {
                            false
                        };
                        if !resolved && !command_audit_keys.contains(&(file.path.clone(), i + 1)) {
                            let mut allowed: Vec<String> = RESERVED_AUDIT_STREAMS
                                .iter()
                                .map(|s| (*s).to_owned())
                                .collect();
                            if let Some(feature) = current_feature.as_ref() {
                                if let Some(set) = feature_event_groups.get(feature) {
                                    allowed.extend(set.iter().cloned());
                                }
                            }
                            diagnostics.push(DoctorDiagnostic {
                                path: file.path.clone(),
                                line: i + 1,
                                column: leading + 1,
                                severity: DoctorSeverity::Error,
                                code: "audit_emit_to_unknown_diagnostics".to_owned(),
                                message: format!(
                                    "`audit emit_to {target}` does not resolve. Allowed: {}.",
                                    allowed
                                        .iter()
                                        .map(|s| format!("`{s}`"))
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                        }
                        audit_pending = None;
                    }
                }
            }
        }
    }

    // `event.trace <name> level <X>` + domain-event `level` rejection.
    // Both are text-walked because the canonical-indent slice does not
    // yet lower events (Phase L row 24).
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut pending_event: Option<(usize, bool, usize)> = None; // (start_line, is_trace, indent)
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let _ = rest;
                pending_event = Some((i, true, leading));
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("event ") {
                let _ = rest;
                pending_event = Some((i, false, leading));
                continue;
            }
            if let Some((_, is_trace, event_indent)) = pending_event {
                if leading <= event_indent {
                    pending_event = None;
                } else if let Some(level_rest) = trimmed.strip_prefix("level ") {
                    let level = level_rest.trim();
                    if is_trace {
                        if !TRACE_LEVEL_CATALOG.contains(&level) {
                            diagnostics.push(DoctorDiagnostic {
                                path: file.path.clone(),
                                line: i + 1,
                                column: leading + 1,
                                severity: DoctorSeverity::Error,
                                code: "event_trace_level_invalid_diagnostics".to_owned(),
                                message: format!(
                                    "`event.trace ... level {level}` is not in the closed catalog. Allowed values: {}.",
                                    catalog_list(TRACE_LEVEL_CATALOG),
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                        }
                    } else {
                        diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: i + 1,
                            column: leading + 1,
                            severity: DoctorSeverity::Error,
                            code: "event_trace_level_on_domain_event_diagnostics".to_owned(),
                            message: "`level` is only valid on `event.trace`, not on a domain `event`. Move the slot to a `event.trace` block or remove the `level` line.".to_owned(),
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
    }

    // Health probe paths from `app.runtime <unit>.{healthcheck,readiness}`.
    // We trust the parser (`parse_app_manifest`) to populate the IR;
    // doctor only validates shape ("/foo") here.
    if let Some(manifest) = app {
        for unit in &manifest.manifest.runtime {
            for (slot, value) in [
                ("healthcheck", unit.healthcheck.as_deref()),
                ("readiness", unit.readiness.as_deref()),
            ] {
                let Some(path) = value else {
                    continue;
                };
                if !path.starts_with('/') || path.contains(char::is_whitespace) {
                    diagnostics.push(DoctorDiagnostic {
                        path: manifest.path.clone(),
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "health_probe_path_invalid_diagnostics".to_owned(),
                        message: format!(
                            "`app.runtime unit {unit_name} {slot} {path:?}` must be a path starting with `/` and containing no whitespace.",
                            unit_name = unit.name,
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

mod policy_hints;

pub(super) use policy_hints::{
    is_write_effect_command, resource_policy_and_command_audit_hints, write_effect_resource,
};
