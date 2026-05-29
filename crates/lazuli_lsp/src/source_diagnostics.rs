//! Small file-local source-walk diagnostics that live at the crate
//! root because they don't fit any one catalog family.
//!
//! * [`approval_contract_diagnostics`] — Cut A.9 file-local checks on
//!   `approval` blocks declared inside commands. Required children
//!   (an approver via `by` *or* `chain`, plus `timeout`, `then`) +
//!   closed-catalog `then` value (`deny` / `allow` / `escalate`).
//!   W4 GAP-06: `chain [@role.a, @role.b] [sequential]` satisfies the
//!   approver requirement when `by` is absent. Cross-feature role
//!   resolution (and chain non-empty / role-declared / timeout-shape)
//!   lives in doctor's IR-layer `approval_diagnostics`.
//! * [`reserved_trace_event_diagnostics`] — Cut A.8 file-local check
//!   that flags authored `event.trace <name>` declarations whose
//!   `<name>` is reserved by the IR's built-in trace event registry.
//! * [`anchor_whitelist_diagnostics`] — flags `extensible_by`
//!   declarations whose listed feature does not extend the
//!   `@anchor.*` site.
//!
//! All three are re-exported at the crate root via `lib.rs` so
//! `crate::dispatch::diagnostics_for_with_profile_inner` continues to
//! `extend(...)` them in place.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    extends_anchor, extensible_by_features, feature_name, leading_spaces,
    simple_canonical_diagnostic, view_anchor,
};

/// Cut A.9 — file-local checks on `approval` blocks declared inside
/// commands. Required children present (an approver via `by` *or*
/// `chain`, plus `timeout`, `then`), `then` value in the closed
/// catalog. W4 GAP-06: a non-empty `chain [@role.a, ...]` satisfies
/// the approver requirement when `by` is absent (parser accepts
/// either form, never both). Cross-feature role resolution + the
/// chain-order / non-empty / timeout-shape checks live in doctor's
/// IR-layer `approval_diagnostics`.
pub(crate) fn approval_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 4 && trimmed == "approval" {
            let header_line = i;
            let mut has_by = false;
            let mut by_nonempty = false;
            // W4 GAP-06 — `chain [@role.a, @role.b] [sequential]` is the
            // multi-approver form. A non-empty bracketed list satisfies the
            // approver requirement on its own; `by` becomes optional then.
            let mut has_chain = false;
            let mut chain_nonempty = false;
            let mut has_timeout = false;
            let mut timeout_nonempty = false;
            let mut has_then = false;
            let mut then_invalid: Option<String> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let body = lines[j];
                let body_trim = body.trim_start();
                if body_trim.is_empty() || body_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(body) <= 4 {
                    break;
                }
                if leading_spaces(body) == 6 {
                    if let Some(rest) = body_trim.strip_prefix("by ") {
                        has_by = true;
                        by_nonempty = rest.split(',').any(|s| !s.trim().is_empty());
                    } else if let Some(rest) = body_trim.strip_prefix("chain ") {
                        has_chain = true;
                        chain_nonempty = approval_chain_nonempty(rest.trim());
                    } else if let Some(rest) = body_trim.strip_prefix("timeout ") {
                        has_timeout = true;
                        timeout_nonempty = !rest.trim().is_empty();
                    } else if let Some(rest) = body_trim.strip_prefix("then ") {
                        has_then = true;
                        let value = rest.trim().to_owned();
                        if !matches!(value.as_str(), "deny" | "allow" | "escalate") {
                            then_invalid = Some(value);
                        }
                    }
                }
                j += 1;
            }

            let mut missing: Vec<&str> = Vec::new();
            // An approver comes from `by <role>` (non-empty) or a non-empty
            // `chain [...]`. Either satisfies the requirement; the parser
            // rejects both-at-once, so we don't re-check exclusivity here.
            let has_approver = (has_by && by_nonempty) || (has_chain && chain_nonempty);
            if !has_approver {
                missing.push("by");
            }
            if !has_timeout || !timeout_nonempty {
                missing.push("timeout");
            }
            if !has_then {
                missing.push("then");
            }
            if !missing.is_empty() {
                diagnostics.push(simple_canonical_diagnostic(
                    header_line,
                    line,
                    DiagnosticSeverity::ERROR,
                    "approval_contract_diagnostics",
                    &format!(
                        "`approval` block is missing required children: {}.",
                        missing.join(", "),
                    ),
                ));
            }
            if let Some(value) = then_invalid {
                diagnostics.push(simple_canonical_diagnostic(
                    header_line,
                    line,
                    DiagnosticSeverity::ERROR,
                    "approval_contract_diagnostics",
                    &format!(
                        "`approval then {value}` is invalid — closed catalog is `deny`, `allow`, or `escalate`."
                    ),
                ));
            }
            i = j;
            continue;
        }
        i += 1;
    }

    diagnostics
}

/// W4 GAP-06 — file-local shape check for an `approval chain` body:
/// `[@role.a, @role.b] [sequential]`. Returns `true` when the bracketed
/// list parses and holds at least one approver. Mirrors the parser's
/// `parse_approval_chain` so the file-local walker agrees with the IR;
/// the authoritative non-empty / role-declared checks run in doctor.
fn approval_chain_nonempty(body: &str) -> bool {
    let body = body.trim();
    let Some(close) = body.find(']') else {
        return false;
    };
    let Some(list) = body[..close].strip_prefix('[') else {
        return false;
    };
    list.split(',').any(|t| !t.trim().is_empty())
}

/// Cut A.8 — flag authored `event.trace <name>` declarations whose
/// `<name>` is reserved by the IR's built-in trace event registry.
/// File-local fast feedback that mirrors doctor's
/// `event_trace_reserved_name_diagnostics`.
pub(crate) fn reserved_trace_event_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("event.trace ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lazuli_ir::is_reserved_trace_event_name(name) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "event_trace_reserved_name_diagnostics",
                    &format!(
                        "`event.trace {name}` is reserved by the IR as a built-in trace event; the runtime emits it automatically. Authoring this declaration is rejected — subscribe via `job ... trigger event.trace {name}` instead."
                    ),
                ));
            }
        }
    }
    diagnostics
}

#[derive(Debug)]
pub(crate) struct AnchorWhitelistEntry {
    anchor: String,
    feature: String,
    line_index: usize,
    line: String,
}

pub(crate) fn anchor_whitelist_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut whitelisted = Vec::new();
    let mut extensions = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_view_anchor: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_view_anchor = None;
            }
            2 => {
                current_view_anchor = None;
                if let Some(anchor) = extends_anchor(trimmed) {
                    if let Some(feature) = current_feature.as_deref() {
                        extensions.insert((anchor.to_owned(), feature.to_owned()));
                    }
                }
            }
            4 => {
                current_view_anchor = view_anchor(trimmed).map(str::to_owned);
            }
            6 => {
                let Some(anchor) = current_view_anchor.as_deref() else {
                    continue;
                };

                for feature in extensible_by_features(trimmed) {
                    whitelisted.push(AnchorWhitelistEntry {
                        anchor: anchor.to_owned(),
                        feature,
                        line_index,
                        line: line.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    for entry in whitelisted {
        if !extensions.contains(&(entry.anchor.clone(), entry.feature.clone())) {
            diagnostics.push(simple_canonical_diagnostic(
                entry.line_index,
                &entry.line,
                DiagnosticSeverity::WARNING,
                "anchor-whitelist-unused",
                &format!(
                    "`extensible_by` lists feature `{}`, but that feature does not extend `@anchor.{}`.",
                    entry.feature, entry.anchor
                ),
            ));
        }
    }

    diagnostics
}
