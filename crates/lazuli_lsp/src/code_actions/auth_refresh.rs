//! Code actions for the auth refresh-rotation contract (proposal
//! `docs/proposals/auth-refresh-rotation.md`).
//!
//! When the cursor sits inside an `auth.sessions` block that lacks a
//! `rotation` child, this module offers two quickfixes:
//!
//! * **Promote single-token to rotation** — fires on the `sessions` slot
//!   itself; expands the block to the framework-default rotation triplet
//!   (`refresh_ttl`, `grace`, `theft_detection_action`) plus an
//!   `access_ttl` default when one isn't already declared.
//! * **Scaffold rotation block** — fires on a `rotation` header that has
//!   no children; injects the same triplet at the right indent.
//!
//! Both actions are pure text edits: they synthesise lines at the correct
//! indent and emit a single `WorkspaceEdit`. No IR awareness — the LSP
//! deliberately stays text-shaped here because authors edit `sessions`
//! by hand far more often than they cross-validate it against the
//! lowered model.
//!
//! ## See also
//! * `lib.rs::enclosing_auth_sessions_block` / `enclosing_auth_rotation_block` —
//!   locate the surrounding block facts.
//! * `lib.rs::auth_sessions_has_child` / `auth_rotation_has_children` —
//!   guard against double-scaffolding.
//! * `lib.rs::is_rotation_line` — token detector that drives the
//!   `Scaffold rotation block` path.
//! * `code_actions/error_vocab.rs` — sister `errors` block scaffold.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::{
    AuthRotationBlock, AuthSessionsBlock, auth_rotation_has_children, auth_sessions_has_child,
    enclosing_auth_rotation_block, enclosing_auth_sessions_block, is_rotation_line,
    position_at_line_start,
};

/// Code actions for the auth refresh-rotation contract. Returns the
/// applicable quickfixes for the cursor position — `Promote
/// single-token to rotation` (on a `sessions` slot lacking `rotation`)
/// and `Scaffold rotation block` (on a `rotation` header with no
/// children). Returns an empty vector outside auth blocks.
///
/// Pure text edits — no IR awareness. See module docs for the rationale.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::auth_refresh_code_actions;
/// use tower_lsp::lsp_types::{Position, Url};
///
/// let uri = Url::parse("file:///example.lzi").unwrap();
/// // Cursor on a feature header — no auth context, no actions.
/// let actions = auth_refresh_code_actions(
///     "feature billing\n",
///     &uri,
///     Position { line: 0, character: 0 },
/// );
/// assert!(actions.is_empty());
/// ```
pub fn auth_refresh_code_actions(
    source: &str,
    uri: &Url,
    position: Position,
) -> Vec<CodeActionOrCommand> {
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let trimmed = line.trim_start();

    if is_rotation_line(trimmed)
        && let Some(rotation) = enclosing_auth_rotation_block(source, position)
        && !auth_rotation_has_children(source, rotation)
        && let Some(action) = build_scaffold_rotation_block_action(source, uri, rotation)
    {
        actions.push(action.into());
    }

    if let Some(sessions) = enclosing_auth_sessions_block(source, position)
        && !auth_sessions_has_child(source, sessions, "rotation")
        && let Some(action) = build_promote_single_token_to_rotation_action(source, uri, sessions)
    {
        actions.push(action.into());
    }

    actions
}

pub(crate) fn build_promote_single_token_to_rotation_action(
    source: &str,
    uri: &Url,
    sessions: AuthSessionsBlock,
) -> Option<CodeAction> {
    let include_access_ttl = !auth_sessions_has_child(source, sessions, "access_ttl");
    let new_text = build_rotation_defaults_text(sessions.indent, include_access_ttl);
    let insertion = position_at_line_start(sessions.end_line);
    let edits = vec![TextEdit {
        range: Range {
            start: insertion,
            end: insertion,
        },
        new_text,
    }];
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(CodeAction {
        title: "Promote single-token to rotation".to_owned(),
        kind: Some(CodeActionKind::REFACTOR_REWRITE),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

pub(crate) fn build_scaffold_rotation_block_action(
    _source: &str,
    uri: &Url,
    rotation: AuthRotationBlock,
) -> Option<CodeAction> {
    let insertion = position_at_line_start(rotation.line_idx + 1);
    let edits = vec![TextEdit {
        range: Range {
            start: insertion,
            end: insertion,
        },
        new_text: build_rotation_inner_defaults_text(rotation.indent),
    }];
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(CodeAction {
        title: "Scaffold rotation block".to_owned(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

pub(crate) fn build_rotation_defaults_text(
    sessions_indent: usize,
    include_access_ttl: bool,
) -> String {
    let session_child_indent = " ".repeat(sessions_indent + 2);
    let mut lines: Vec<String> = Vec::new();
    if include_access_ttl {
        lines.push(format!(
            "{session_child_indent}access_ttl \"15 minutes\" # framework default: short-lived access"
        ));
    }
    lines.push(format!("{session_child_indent}rotation"));
    lines.push(
        build_rotation_inner_defaults_text(sessions_indent + 2)
            .trim_end()
            .to_owned(),
    );
    format!("{}\n", lines.join("\n"))
}

pub(crate) fn build_rotation_inner_defaults_text(rotation_indent: usize) -> String {
    let child_indent = " ".repeat(rotation_indent + 2);
    [
        format!("{child_indent}refresh_ttl \"30 days\" # framework default: long-lived refresh"),
        format!("{child_indent}grace \"30 seconds\" # framework default: two-tab race window"),
        format!(
            "{child_indent}theft_detection_action revoke_session_family # framework default: revoke this session family"
        ),
    ]
    .join("\n")
        + "\n"
}
