//! Code actions for the IR Error-Vocab contract (proposal
//! `docs/proposals/ir-error-messages-vocab.md` §7.4).
//!
//! Three quickfixes are exposed:
//!
//! 1. **Scaffold `errors` block with all 12 codes** — fires on a
//!    `feature <name>` header line when the feature has no `errors`
//!    block yet. Emits a complete `errors` body plus 8 stub `key`
//!    entries in the feature's `translation` block (creating that
//!    block if missing).
//! 2. **Add `when_denied @translation.<stub>` (per-policy)** — fires on
//!    a `policies.<category>:` line that has no `when_denied` child.
//!    Inserts the `when_denied` line at the right child indent and
//!    wires it to a stub translation key.
//! 3. **Add `when_denied @translation.<stub>` (per-command)** — fires
//!    on a `command.policy @policy.<name>` line with no `when_denied`
//!    child. Same shape as #2 but the stub key follows the
//!    `<feature>_<command>_denied` pattern when the surrounding command
//!    name is recoverable.
//!
//! All actions are pure text edits with a single `WorkspaceEdit`. The
//! module deliberately stays at the text/indent layer — error-vocab
//! resolution against the lowered IR happens elsewhere (doctor, runtime)
//! and is not duplicated here.
//!
//! ## See also
//! * `lib.rs::feature_has_errors_block` / `feature_has_translation_block`
//!   were inlined into this module.
//! * `crate::catalogs::ERROR_VOCAB_CODES` — the closed catalog of 8 stub
//!   codes that drive the scaffold text.
//! * `crate::source_scan::enclosing_feature_name` — feature-name
//!   discovery used by the per-policy and per-command actions.
//! * `code_actions/auth_refresh.rs` — sister scaffold for the auth
//!   refresh-rotation contract.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};

use crate::{
    ERROR_VOCAB_CODES, enclosing_feature_name, leading_spaces, position_at_line_start,
};

/// IR Error-Vocab code actions — three actions per proposal §7.4.
pub fn error_vocab_code_actions(
    source: &str,
    uri: &Url,
    position: Position,
) -> Vec<CodeActionOrCommand> {
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let line = source
        .lines()
        .nth(position.line as usize)
        .unwrap_or("")
        .to_owned();
    let trimmed = line.trim_start();
    let line_indent = leading_spaces(&line);

    // Action 1 — scaffold `errors` block + 8 stub translation keys. Fires
    // when the cursor is on a `feature <name>` header (indent 0) AND the
    // feature has no `errors` block yet.
    if line_indent == 0 {
        if let Some(rest) = trimmed.strip_prefix("feature ") {
            let feature_name = rest.split_whitespace().next().unwrap_or("");
            if !feature_name.is_empty() && !feature_has_errors_block(source, feature_name) {
                if let Some(action) = build_scaffold_errors_action(source, uri, feature_name) {
                    actions.push(action.into());
                }
            }
        }
    }

    // Action 2 — add `when_denied @translation.<stub>` to a
    // `policies.<category>:` line.
    if let Some(category) =
        policies_category_name(&line).filter(|_| in_policies_block(source, position))
    {
        if !has_when_denied_child(source, position.line as usize, line_indent) {
            if let Some(action) = build_add_when_denied_policies_action(
                source,
                uri,
                position.line as usize,
                line_indent,
                &category,
            ) {
                actions.push(action.into());
            }
        }
    }

    // Action 3 — add `when_denied @translation.<stub>` to a
    // `command.policy @policy.<name>` line.
    if trimmed.starts_with("policy @policy.")
        && !has_when_denied_child(source, position.line as usize, line_indent)
    {
        if let Some(action) =
            build_add_when_denied_command_action(source, uri, position.line as usize, line_indent)
        {
            actions.push(action.into());
        }
    }

    actions
}

/// Check whether the named feature already contains an `errors` block.
pub(crate) fn feature_has_errors_block(source: &str, feature_name: &str) -> bool {
    let mut in_feature = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false);
            continue;
        }
        if in_feature && indent == 2 && trimmed == "errors" {
            return true;
        }
    }
    false
}

/// Pull a `<name>:` category from a `policies` entry line.
pub(crate) fn policies_category_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let colon = trimmed.find(':')?;
    let name = trimmed[..colon].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let after = trimmed[colon + 1..].trim_start();
    if after.is_empty() {
        return None;
    }
    Some(name.to_owned())
}

/// Walk backwards looking for whether `cursor_line_idx` sits inside a
/// `policies` block.
pub(crate) fn in_policies_block(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < cursor_indent {
            return trimmed == "policies" || trimmed.starts_with("policies ");
        }
    }
    false
}

/// Look ahead from `line_idx` for the next non-empty line. Return true
/// when that line is indented deeper than `parent_indent` AND its trimmed
/// form starts with `when_denied`.
pub(crate) fn has_when_denied_child(source: &str, line_idx: usize, parent_indent: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for idx in (line_idx + 1)..lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= parent_indent {
            return false;
        }
        return trimmed.starts_with("when_denied");
    }
    false
}

/// Build the "Scaffold `errors` block with all 12 codes" code action.
pub(crate) fn build_scaffold_errors_action(
    source: &str,
    uri: &Url,
    feature_name: &str,
) -> Option<CodeAction> {
    let lines: Vec<&str> = source.lines().collect();
    let feature_header_line = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        leading_spaces(line) == 0
            && trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false)
    })?;
    let feature_end = (feature_header_line + 1..lines.len())
        .find(|&idx| {
            let line = lines[idx];
            let trimmed = line.trim_start();
            !trimmed.is_empty() && !trimmed.starts_with('#') && leading_spaces(line) == 0
        })
        .unwrap_or(lines.len());

    let mut insertion_line = feature_header_line + 1;
    let mut inside_policies = false;
    let mut policies_end_line: Option<usize> = None;
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(feature_end)
        .skip(feature_header_line + 1)
    {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 2 {
            if inside_policies {
                policies_end_line = Some(idx);
                inside_policies = false;
            }
            if trimmed == "policies" || trimmed.starts_with("policies ") {
                inside_policies = true;
                policies_end_line = None;
            }
        }
    }
    if inside_policies {
        policies_end_line = Some(feature_end);
    }
    if let Some(end) = policies_end_line {
        insertion_line = end;
    }

    let errors_block = build_errors_block_text(feature_name);
    let needs_translation_block = !feature_has_translation_block(source, feature_name);
    let translation_text = if needs_translation_block {
        build_translation_block_with_stubs(feature_name)
    } else {
        build_translation_stub_keys_only(feature_name)
    };

    let edit_range = position_at_line_start(insertion_line);
    let edits = vec![TextEdit {
        range: Range {
            start: edit_range,
            end: edit_range,
        },
        new_text: format!("{errors_block}\n{translation_text}\n"),
    }];

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };
    Some(CodeAction {
        title: format!("Scaffold `errors` block with all 12 codes ({feature_name})"),
        kind: Some(CodeActionKind::REFACTOR_REWRITE),
        diagnostics: None,
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Detect whether the named feature already has a `translation` block.
pub(crate) fn feature_has_translation_block(source: &str, feature_name: &str) -> bool {
    let mut in_feature = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false);
            continue;
        }
        if in_feature
            && indent == 2
            && (trimmed == "translation" || trimmed.starts_with("translation "))
        {
            return true;
        }
    }
    false
}

/// Canonical text for a complete scaffold of the `errors` block.
pub(crate) fn build_errors_block_text(feature_name: &str) -> String {
    [
        "  errors".to_owned(),
        "    default hide".to_owned(),
        "    expose client 4xx message, code".to_owned(),
        "    expose client 5xx code".to_owned(),
        format!("    policy_denied      message @translation.{feature_name}_policy_denied"),
        format!("    validation_failed  message @translation.{feature_name}_validation_failed"),
        format!("    tenant_mismatch    message @translation.{feature_name}_tenant_mismatch"),
        format!("    not_found          message @translation.{feature_name}_not_found"),
        format!("    rate_limited       message @translation.{feature_name}_rate_limited"),
        format!("    bad_request        message @translation.{feature_name}_bad_request"),
        format!("    method_not_allowed message @translation.{feature_name}_method_not_allowed"),
        format!("    integration_error  message @translation.{feature_name}_integration_error"),
    ]
    .join("\n")
}

/// Build a fresh `translation` block prepopulated with 8 stub keys.
pub(crate) fn build_translation_block_with_stubs(feature_name: &str) -> String {
    let mut lines: Vec<String> = vec![
        "  translation".to_owned(),
        format!("    catalog \"./i18n/{feature_name}.<locale>.json\""),
    ];
    for code in ERROR_VOCAB_CODES {
        lines.push(format!("    key {feature_name}_{code}"));
        lines.push("      en-US \"TODO: customize this message.\"".to_owned());
    }
    lines.join("\n")
}

/// Build only the 8 stub `key` lines (without a header).
pub(crate) fn build_translation_stub_keys_only(feature_name: &str) -> String {
    let mut lines: Vec<String> = vec![
        "  # error-vocab — add the 8 stub keys into the existing `translation` block:".to_owned(),
    ];
    for code in ERROR_VOCAB_CODES {
        lines.push(format!("  #   key {feature_name}_{code}"));
        lines.push("  #     en-US \"TODO: customize this message.\"".to_owned());
    }
    lines.join("\n")
}

/// Build the "Add `when_denied @translation.<stub>`" code action for a
/// `policies.<category>:` line.
pub(crate) fn build_add_when_denied_policies_action(
    source: &str,
    uri: &Url,
    line_idx: usize,
    parent_indent: usize,
    category: &str,
) -> Option<CodeAction> {
    let feature = enclosing_feature_name(
        source,
        Position {
            line: line_idx as u32,
            character: 0,
        },
    )?;
    let stub_key = format!("{feature}_{category}_denied");
    let child_indent = " ".repeat(parent_indent + 2);
    let new_line = format!("{child_indent}when_denied @translation.{stub_key}\n");
    let insertion = position_at_line_start(line_idx + 1);
    let edits = vec![TextEdit {
        range: Range {
            start: insertion,
            end: insertion,
        },
        new_text: new_line,
    }];
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };
    Some(CodeAction {
        title: format!("Add `when_denied @translation.{stub_key}` (per-policy default)"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Build the "Add `when_denied @translation.<stub>`" code action for a
/// `command.policy @policy.<name>` line.
pub(crate) fn build_add_when_denied_command_action(
    source: &str,
    uri: &Url,
    line_idx: usize,
    parent_indent: usize,
) -> Option<CodeAction> {
    let feature = enclosing_feature_name(
        source,
        Position {
            line: line_idx as u32,
            character: 0,
        },
    )?;
    let command_name =
        enclosing_command_name(source, line_idx).unwrap_or_else(|| "command".to_owned());
    let stub_key = format!("{feature}_{command_name}_denied");
    let child_indent = " ".repeat(parent_indent + 2);
    let new_line = format!("{child_indent}when_denied @translation.{stub_key}\n");
    let insertion = position_at_line_start(line_idx + 1);
    let edits = vec![TextEdit {
        range: Range {
            start: insertion,
            end: insertion,
        },
        new_text: new_line,
    }];
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };
    Some(CodeAction {
        title: format!("Add `when_denied @translation.{stub_key}` (per-command override)"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Walk backwards from `line_idx` to find the enclosing
/// `command <name>` / `query.* <name>` / `api <name>` and return its name.
pub(crate) fn enclosing_command_name(source: &str, line_idx: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let target_line = lines.get(line_idx).copied().unwrap_or("");
    let target_indent = leading_spaces(target_line);
    for idx in (0..line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent >= target_indent {
            continue;
        }
        let mut tokens = trimmed.split_whitespace();
        let kind = tokens.next().unwrap_or("");
        if matches!(
            kind,
            "command"
                | "query.list"
                | "query.lookup"
                | "query.sql"
                | "query.view"
                | "api"
                | "webhook"
                | "job"
                | "agent"
                | "workflow"
                | "channel"
        ) {
            let name = tokens.next().unwrap_or("");
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
        return None;
    }
    None
}
