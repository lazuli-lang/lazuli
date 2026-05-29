//! Approval primitive aggregator.
//!
//! Owns the four `approval_*` diagnostics that cross-check the
//! `approval` block on commands:
//!
//! * `approval_missing_children` — file-local check: the
//!   text-walker captures the presence of an `approval` block plus
//!   its missing children (`timeout`, `then`, `by`, `requires` /
//!   `approvers`).
//! * `approval_diagnostics` — IR-driven checks: timeout shape
//!   (`approval_timeout_unparseable` / `_too_short` / `_too_long`),
//!   `then` catalog (`approval_then_invalid`), role resolution
//!   (`approval_role_unresolved`), and approver-resource catalog
//!   (`approval_resource_unknown` / `approval_resource_missing_field`).
//!
//! Supporting:
//!
//! * `ApprovalBlockPresence` struct — minimal text-walker output
//!   used by `approval_missing_children_diagnostics`. Kept here
//!   because the in-process unit test path (`package_from_sources`)
//!   does not feed sources through `lazuli_lsp::diagnostics_for_source`.
//! * `collect_approval_block_presence` — text walker that captures
//!   the `approval` block's existence + missing children.
//! * `approval_timeout_well_formed` — shape validator for the
//!   `timeout` child, factored out so `approval_diagnostics` can
//!   short-circuit on malformed timeouts without re-parsing them.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::doctor::scanners::leading_spaces;
use crate::doctor::{
    DoctorDiagnostic, DoctorFile, DoctorSeverity, ResourceFact, Tier3FeatureFacts,
};

/// Phase L Tier 4b — minimal text-walker output that captures the
/// existence of an `approval` block plus its missing children. Other
/// approval cross-checks (`timeout` shape, `then` catalog, `by` role
/// resolution) read from IR `Command.approval` via `Tier3FeatureFacts`.
///
/// The walker is retained because the parser canonical-indent slice
/// rejects malformed `approval` blocks with a parse error — which
/// short-circuits the feature lift, so `Command.approval` never reaches
/// the IR for those sources. The LSP file-local pass
/// (`approval_contract_diagnostics` in `crates/lazuli_lsp/src/lib.rs`)
/// emits the same diagnostic when invoked via `lazuli doctor`; this
/// walker covers the in-process unit test path
/// (`package_from_sources`), which does not feed sources through
/// `lazuli_lsp::diagnostics_for_source`.
#[derive(Debug, Clone)]
pub(crate) struct ApprovalBlockPresence {
    pub(crate) feature: String,
    pub(crate) command: String,
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) missing_children: Vec<&'static str>,
}

pub(crate) fn collect_approval_block_presence(
    file: &DoctorFile,
    out: &mut Vec<ApprovalBlockPresence>,
) {
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
        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            let name = trimmed
                .strip_prefix("command ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let feature_name = feature.clone().unwrap_or_default();

            // Find the `approval` child (indent 4) inside this command body.
            let mut j = i + 1;
            let mut approval_at: Option<usize> = None;
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
                if leading_spaces(inner) == 4 && inner_trim == "approval" {
                    approval_at = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(approval_at) = approval_at {
                let mut has_by = false;
                let mut has_timeout = false;
                let mut has_then = false;
                let mut k = approval_at + 1;
                while k < lines.len() {
                    let body = lines[k];
                    let body_trim = body.trim_start();
                    if body_trim.is_empty() || body_trim.starts_with('#') {
                        k += 1;
                        continue;
                    }
                    if leading_spaces(body) <= 4 {
                        break;
                    }
                    if leading_spaces(body) == 6 {
                        if body_trim.starts_with("by ") {
                            has_by = true;
                        } else if body_trim.starts_with("timeout ") {
                            has_timeout = true;
                        } else if body_trim.starts_with("then ") {
                            has_then = true;
                        }
                    }
                    k += 1;
                }
                let mut missing: Vec<&'static str> = Vec::new();
                if !has_by {
                    missing.push("by");
                }
                if !has_timeout {
                    missing.push("timeout");
                }
                if !has_then {
                    missing.push("then");
                }
                if !missing.is_empty() {
                    out.push(ApprovalBlockPresence {
                        feature: feature_name,
                        command: name,
                        path: file.path.clone(),
                        line: approval_at + 1,
                        missing_children: missing,
                    });
                }
                i = k;
                continue;
            }
        }
        i += 1;
    }
}

/// `approval_contract_diagnostics` (missing-children variant) — emitted
/// from the text-pattern walker above because parse-error approval
/// blocks never reach the IR.
pub(crate) fn approval_missing_children_diagnostics(
    presences: &[ApprovalBlockPresence],
) -> Vec<DoctorDiagnostic> {
    presences
        .iter()
        .map(|p| DoctorDiagnostic {
            path: p.path.clone(),
            line: p.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "approval_contract_diagnostics".to_owned(),
            message: format!(
                "command `{}.{}` declares `approval` but is missing required children: {}.",
                p.feature,
                p.command,
                p.missing_children.join(", "),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

/// Doctor-side diagnostics for the `approval` primitive. Three
/// dedicated ids plus the write-tool guard extension; the latter
/// reaches inside `agent_tool_write_unguarded_diagnostics` so write
/// tools whose target command carries `approval` no longer require
/// the agent's `safety` validator.
///
/// Phase L Tier 4b — reads `Command.approval` from `Tier3FeatureFacts`
/// (populated by `lower_feature_skeleton`). The text-walking
/// `CommandApprovalFact` collector retired; a minimal
/// `ApprovalBlockPresence` walker survives for the
/// missing-required-children variant. Two drift facts the retirement
/// repaired:
///
/// 1. The text-walker treated `by` as a `,`-split list. The parser
///    only accepts a single `by` declaration; the lifted IR carries
///    `by: String`. The doctor now validates the single role atom.
/// 2. The text-walker reported a closed catalog of `deny | proceed`
///    for `then`. The parser already enforces `deny | allow | escalate`
///    at parse time, lowering to `ApprovalThen` (enum-fechado). The
///    redundant doctor-side catalog check retired.
pub(crate) fn approval_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
    known_roles: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for feature in tier3_facts {
        for command in &feature.commands {
            let Some(approval) = &command.approval else {
                continue;
            };
            let line = feature
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or(feature.feature_line);

            // Timeout shape (when authored).
            if let Some(timeout) = approval.timeout.as_deref() {
                if !approval_timeout_well_formed(timeout) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "approval_timeout_invalid_diagnostics".to_owned(),
                        message: format!(
                            "command `{}.{}` declares `approval timeout {:?}` which is not a recognised duration shape (e.g. `\"24h\"`, `\"30 minutes\"`, `\"7d\"`).",
                            feature.feature, command.name, timeout,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // W4 GAP-06 — APPROVAL-CHAIN-ORDER-001. Validate the ordered
            // approver chain: it must be non-empty, and every entry must be a
            // declared `@role.<name>` (closed catalog). Single-approver `by`
            // forms lift to a 1-element chain via `approvers()`, so this one
            // walk covers both shapes.
            let approvers = approval.approvers();
            if approvers.is_empty() {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "APPROVAL-CHAIN-ORDER-001".to_owned(),
                    message: format!(
                        "command `{}.{}` approval `chain` is empty; declare at least one `@role.<name>` approver.",
                        feature.feature, command.name,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            }
            for role_ref in &approvers {
                let Some(suffix) = role_ref.strip_prefix("@role.") else {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "APPROVAL-CHAIN-ORDER-001".to_owned(),
                        message: format!(
                            "command `{}.{}` approval approver `{role_ref}` is not a `@role.<name>` reference; approvers are roles, not scopes.",
                            feature.feature, command.name,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                    continue;
                };
                if !known_roles.contains(suffix) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "APPROVAL-CHAIN-ORDER-001".to_owned(),
                        message: format!(
                            "command `{}.{}` approval approver `@role.{suffix}` references a role that no `policies` block or `app.lzi` `policy_for` declares.",
                            feature.feature, command.name,
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

/// Shape-check a duration string. Accepts `<digits> <unit>` or
/// `<digits><unit>` where unit ∈ s, m, h, d, w, second(s),
/// minute(s), hour(s), day(s), week(s). The runtime adapter parses
/// the canonical form; this layer rejects obviously malformed input.
pub(crate) fn approval_timeout_well_formed(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut chars = trimmed.chars();
    let mut saw_digit = false;
    let mut unit_start = 0;
    for (i, c) in chars.by_ref().enumerate() {
        if c.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        unit_start = i;
        break;
    }
    if !saw_digit {
        return false;
    }
    let unit = trimmed[unit_start..].trim();
    matches!(
        unit,
        "s" | "m"
            | "h"
            | "d"
            | "w"
            | "second"
            | "seconds"
            | "minute"
            | "minutes"
            | "hour"
            | "hours"
            | "day"
            | "days"
            | "week"
            | "weeks"
    )
}
