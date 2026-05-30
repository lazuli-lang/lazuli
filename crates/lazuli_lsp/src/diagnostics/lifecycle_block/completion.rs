//! Completion items for the `lifecycle <field>` resource-child block
//! vocabulary (proposal §3).
//!
//! Three trigger contexts:
//!
//! 1. **Closed invariant catalog** — when authoring `invariant `,
//!    surfaces the three legal forms (`terminal_immutable`,
//!    `single ${state} per ${scope}`, `no_jump_more_than_one`).
//! 2. **Lifecycle-block children** — when the cursor sits at the
//!    lifecycle-block child indent (header_indent + 2), surfaces the
//!    canonical child keywords (`state`, `transition`, `invariant`,
//!    `invariant_handler`, `audit`, `timestamps`, `tests`).
//! 3. **Transition children** — when the cursor sits at the
//!    transition-block child indent inside a `transition <name>`,
//!    surfaces the legal transition children (`from`, `to`, `policy`,
//!    `audit`, `timestamps`, `emits`, `requires`, `tests`).
//!
//! The text-walk approach matches the existing LSP convention
//! (`route_guard_completions`, `lifecycle_gate_completions`) — gives
//! immediate editor help before the parser sees the new surface.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::{leading_spaces, line_prefix_at_position};

use super::context::{enclosing_lifecycle_block, enclosing_transition_block};

/// Public completion entry point for the `lifecycle <field>` block
/// surface. Returns the appropriate catalog for the cursor's context,
/// or `None` when the cursor sits outside any lifecycle-block trigger.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::lifecycle_block_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Outside any lifecycle block — None.
/// let result = lifecycle_block_completions(
///     "feature billing\n",
///     Position { line: 0, character: 0 },
/// );
/// assert!(result.is_none());
/// ```
pub fn lifecycle_block_completions(
    source: &str,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let block = enclosing_lifecycle_block(source, position)?;
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();
    let cursor_indent = leading_spaces(before);

    // Trigger 1: `invariant ` followed by a partial-word — offer the
    // closed catalog (proposal §3.4).
    if let Some(rest) = trimmed_before.strip_prefix("invariant ")
        && is_partial_identifier(rest)
    {
        return Some(invariant_catalog_completion_items());
    }

    // Trigger 3 takes precedence over trigger 2 — a `transition` child
    // sits deeper than the lifecycle child indent.
    if let Some(transition) = enclosing_transition_block(source, position, &block) {
        let transition_child_indent = transition.header_indent + 2;
        if is_partial_or_blank(before, trimmed_before) && cursor_indent == transition_child_indent {
            return Some(transition_child_completion_items());
        }
    }

    // Trigger 2: at the lifecycle-block child indent — offer the
    // canonical child keywords.
    let lifecycle_child_indent = block.header_indent + 2;
    if is_partial_or_blank(before, trimmed_before) && cursor_indent == lifecycle_child_indent {
        return Some(lifecycle_child_completion_items());
    }

    None
}

fn is_partial_identifier(rest: &str) -> bool {
    rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_partial_or_blank(before: &str, trimmed_before: &str) -> bool {
    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word = !trimmed_before.is_empty()
        && trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    is_blank_indented || is_partial_word
}

pub(crate) fn invariant_catalog_completion_items() -> Vec<CompletionItem> {
    vec![
        snippet_completion(
            "terminal_immutable",
            "terminal_immutable",
            "Forbid edits to a row once it enters a `terminal` state.",
        ),
        snippet_completion(
            "single <state> per <scope>",
            "single ${1:active} per ${2:organization}",
            "At most one row per scope may sit in the named state at a time.",
        ),
        snippet_completion(
            "no_jump_more_than_one",
            "no_jump_more_than_one",
            "Forbid transitions that skip more than one state. Requires linear lifecycle.",
        ),
    ]
}

pub(crate) fn lifecycle_child_completion_items() -> Vec<CompletionItem> {
    vec![
        snippet_completion(
            "state",
            "state ${1:name}${2: initial}",
            "Declare a state. Add `initial` or `terminal` modifier.",
        ),
        snippet_completion(
            "transition",
            "transition ${1:name}\n  from ${2:source_state}\n  to ${3:target_state}",
            "Declare a named transition with from/to states.",
        ),
        snippet_completion(
            "invariant",
            "invariant ${1:terminal_immutable}",
            "Declare an invariant from the closed catalog.",
        ),
        snippet_completion(
            "invariant_handler",
            "invariant_handler ${1:handler_name}",
            "Bind a custom handler invoked when an invariant fails.",
        ),
        snippet_completion(
            "audit",
            "audit",
            "Emit an audit-log entry on each transition.",
        ),
        snippet_completion(
            "timestamps",
            "timestamps",
            "Stamp each transition into per-state `<state>_at` columns.",
        ),
        snippet_completion(
            "tests",
            "tests\n  ${1:# scenarios}",
            "Inline test block scoped to this lifecycle.",
        ),
    ]
}

pub(crate) fn transition_child_completion_items() -> Vec<CompletionItem> {
    vec![
        snippet_completion(
            "from",
            "from ${1:source_state}",
            "Source state(s) the transition may fire from.",
        ),
        snippet_completion(
            "to",
            "to ${1:target_state}",
            "Target state the transition writes.",
        ),
        snippet_completion(
            "policy",
            "policy ${1:@policy.write}",
            "Policy that gates this transition.",
        ),
        snippet_completion(
            "audit",
            "audit",
            "Emit an audit-log entry when this transition fires.",
        ),
        snippet_completion(
            "timestamps",
            "timestamps",
            "Stamp this transition into the resource's per-state timestamp columns.",
        ),
        snippet_completion(
            "emits",
            "emits ${1:event_name}",
            "Domain event published on successful transition.",
        ),
        snippet_completion(
            "requires",
            "requires ${1:condition}",
            "Pre-condition predicate evaluated before the transition fires.",
        ),
        snippet_completion(
            "tests",
            "tests\n  ${1:# scenarios}",
            "Inline test block scoped to this transition.",
        ),
    ]
}

fn snippet_completion(label: &str, body: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(body.to_owned()),
        insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_completions_outside_lifecycle_block() {
        assert!(
            lifecycle_block_completions(
                "feature billing\n",
                Position {
                    line: 0,
                    character: 0,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn closed_invariant_catalog_offered_after_invariant_keyword() {
        // Position cursor right after `invariant ` on the partial-word
        // boundary — should surface the closed catalog.
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        invariant \n";
        let items = lifecycle_block_completions(
            source,
            Position {
                line: 6,
                character: 18,
            },
        )
        .expect("invariant catalog should fire");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"terminal_immutable"),
            "labels = {labels:?}"
        );
        assert!(labels.contains(&"no_jump_more_than_one"));
        assert!(labels.iter().any(|label| label.contains("single")));
    }

    #[test]
    fn lifecycle_child_keywords_offered_at_child_indent() {
        // Cursor sits at lifecycle-child indent (8 spaces here) on a
        // blank line — should offer state/transition/invariant/etc.
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        \n";
        let items = lifecycle_block_completions(
            source,
            Position {
                line: 4,
                character: 8,
            },
        )
        .expect("lifecycle children should fire");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"state"));
        assert!(labels.contains(&"transition"));
        assert!(labels.contains(&"invariant"));
    }

    #[test]
    fn transition_child_keywords_offered_inside_transition() {
        // Cursor at transition-child indent inside a transition block.
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        transition publish\n          \n";
        let items = lifecycle_block_completions(
            source,
            Position {
                line: 7,
                character: 10,
            },
        )
        .expect("transition children should fire");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"from"));
        assert!(labels.contains(&"to"));
        assert!(labels.contains(&"policy"));
        assert!(labels.contains(&"emits"));
    }
}
