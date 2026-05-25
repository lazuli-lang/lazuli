//! Code actions for the IR Lifecycle Route-Gate contract (proposal
//! `docs/proposals/ir-lifecycle-route-gates.md`).
//!
//! Four quickfixes are exposed:
//!
//! 1. **Add lifecycle gate** — fires on a `view <name>` header line when
//!    the view doesn't already declare `requires_lifecycle`. Picks a
//!    resource candidate from the view's `source` / `submit` slots and
//!    inserts the `requires_lifecycle` + `on_lifecycle_pending @resume`
//!    pair at the view's body indent.
//! 2. **Remove stale arm** — fires on a `resume <name>` arm whose state
//!    is no longer declared in the source resource's `lifecycle.states`.
//!    Removes the offending line.
//! 3. **Add missing state arms** — fires inside a `resume <name>` block
//!    that doesn't cover every state. Appends `<state> → view <TODO>`
//!    stubs for every missing state.
//! 4. **Convert to wildcard** — fires inside a `resume <name>` block
//!    when 2+ states are missing. Inserts a `* → view <fallback>` catch-all
//!    so the gate is forward-compatible.
//!
//! All actions are pure text edits. Shared helpers (`enclosing_view_block`,
//! `enclosing_lifecycle_resume_block`, `lifecycle_missing_resume_states`,
//! `lifecycle_resume_for_resource`, `simple_edit_action`, etc.) come from
//! `lib.rs` because they're also used by `lifecycle_gate_completions` and
//! `lifecycle_gate_hover`.
//!
//! ## See also
//! * `lib.rs::LifecycleResumeBlock`, `LifecycleResumeArm`,
//!   `LifecycleGateCandidate` — facts structs shared with completions
//!   and hover.
//! * `lib.rs::simple_edit_action` — shared single-edit builder, also used
//!   by `code_actions::route_guard`.
//! * `code_actions/route_guard.rs` — sister scaffold for the route-guard
//!   contract.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Url,
};

use crate::{
    LifecycleResumeBlock, enclosing_lifecycle_resume_block, enclosing_view_block,
    lifecycle_gate_candidate_for_view, lifecycle_gate_insertion_line,
    lifecycle_missing_resume_states, lifecycle_resume_arm_insertion_line,
    lifecycle_resume_for_resource, lifecycle_stale_resume_arm_on_line, position_at_line_start,
    simple_edit_action, snake_case, view_has_requires_lifecycle,
};

pub fn lifecycle_gate_code_actions(
    source: &str,
    uri: &Url,
    position: Position,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let line = lines.get(line_idx).copied().unwrap_or("");
    let trimmed = line.trim_start();

    if trimmed.starts_with("view ") {
        if let Some(action) = build_add_lifecycle_gate_action(source, uri, line_idx) {
            actions.push(action.into());
        }
    }

    if let Some(resume) = enclosing_lifecycle_resume_block(source, position) {
        if let Some(action) =
            build_remove_stale_lifecycle_arm_action(source, uri, line_idx, &resume)
        {
            actions.push(action.into());
        }
        let missing = lifecycle_missing_resume_states(source, &resume);
        if !missing.is_empty() {
            if let Some(action) = build_add_missing_lifecycle_arms_action(uri, &resume, &missing) {
                actions.push(action.into());
            }
            if missing.len() >= 2 {
                if let Some(action) = build_convert_lifecycle_arms_to_wildcard_action(uri, &resume)
                {
                    actions.push(action.into());
                }
            }
        }
    }

    actions
}

pub(crate) fn build_add_missing_lifecycle_arms_action(
    uri: &Url,
    resume: &LifecycleResumeBlock,
    missing: &[String],
) -> Option<CodeAction> {
    let indent = " ".repeat(resume.header_indent + 2);
    let new_text = missing
        .iter()
        .map(|state| format!("{indent}{state} → view <TODO>\n"))
        .collect::<String>();
    if new_text.is_empty() {
        return None;
    }
    Some(simple_edit_action(
        uri,
        "Add missing state arms",
        CodeActionKind::QUICKFIX,
        vec![TextEdit {
            range: Range {
                start: position_at_line_start(lifecycle_resume_arm_insertion_line(resume)),
                end: position_at_line_start(lifecycle_resume_arm_insertion_line(resume)),
            },
            new_text,
        }],
        true,
    ))
}

pub(crate) fn build_convert_lifecycle_arms_to_wildcard_action(
    uri: &Url,
    resume: &LifecycleResumeBlock,
) -> Option<CodeAction> {
    if resume.arms.iter().any(|arm| arm.state == "*") {
        return None;
    }
    let indent = " ".repeat(resume.header_indent + 2);
    Some(simple_edit_action(
        uri,
        "Convert to wildcard",
        CodeActionKind::REFACTOR_REWRITE,
        vec![TextEdit {
            range: Range {
                start: position_at_line_start(lifecycle_resume_arm_insertion_line(resume)),
                end: position_at_line_start(lifecycle_resume_arm_insertion_line(resume)),
            },
            new_text: format!("{indent}* → view <fallback>\n"),
        }],
        false,
    ))
}

pub(crate) fn build_remove_stale_lifecycle_arm_action(
    source: &str,
    uri: &Url,
    line_idx: usize,
    resume: &LifecycleResumeBlock,
) -> Option<CodeAction> {
    let stale = lifecycle_stale_resume_arm_on_line(source, resume, line_idx)?;
    Some(simple_edit_action(
        uri,
        "Remove stale arm",
        CodeActionKind::QUICKFIX,
        vec![TextEdit {
            range: Range {
                start: position_at_line_start(stale.line),
                end: position_at_line_start(stale.line + 1),
            },
            new_text: String::new(),
        }],
        true,
    ))
}

pub(crate) fn build_add_lifecycle_gate_action(
    source: &str,
    uri: &Url,
    view_line_idx: usize,
) -> Option<CodeAction> {
    let view = enclosing_view_block(
        source,
        Position {
            line: view_line_idx as u32,
            character: 0,
        },
    )?;
    if view_has_requires_lifecycle(source, &view) {
        return None;
    }
    let candidate = lifecycle_gate_candidate_for_view(source, &view)?;
    let child_indent = " ".repeat(view.header_indent + 2);
    let resume =
        lifecycle_resume_for_resource(source, view.feature_hint.as_deref(), &candidate.resource)
            .unwrap_or_else(|| format!("{}_lifecycle", snake_case(&candidate.resource)));
    let new_text = format!(
        "{child_indent}requires_lifecycle {} = {}\n{child_indent}on_lifecycle_pending @resume {resume}\n",
        candidate.resource, candidate.state
    );
    let insertion_line = lifecycle_gate_insertion_line(source, &view);
    Some(simple_edit_action(
        uri,
        "Add lifecycle gate",
        CodeActionKind::QUICKFIX,
        vec![TextEdit {
            range: Range {
                start: position_at_line_start(insertion_line),
                end: position_at_line_start(insertion_line),
            },
            new_text,
        }],
        true,
    ))
}
