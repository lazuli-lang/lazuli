//! Completion for the `triggers transition <name>` command slot.
//!
//! Audit gap #1 (`docs/audits/overnight-2026-06-02/09-lsp.md`): the
//! command-side `triggers transition <name>` slot references the closed set
//! of `transition <name>` declarations inside a resource `lifecycle`
//! block, but no completion provider surfaced that set — so an author /
//! LLM invents a transition name and ships a silently-dangling `triggers`
//! ref, the exact silent-bug surface the audit flagged.
//!
//! This provider closes that loop: when the cursor sits in a `triggers
//! transition <|>` slot (the inline `command ... triggers transition X`
//! form *or* a `transition X` child of a `triggers` block), it offers
//! every `transition <name>` declared in a `lifecycle` block in the same
//! document.
//!
//! ## Why file-local
//!
//! Transitions live on a resource inside the same feature file; a command
//! that triggers one is in that same `.lzi`. So a file-local text-scan
//! (matching the established convention in `completion::namespace`) covers
//! the real authoring case without a workspace IR walk. Cross-file triggers
//! are not a supported surface today.
//!
//! ## Shared helpers
//! - [`crate::leading_spaces`] — indent-aware block scanning.
//! - [`crate::line_prefix_at_position`] — cursor-relative line slice.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::{leading_spaces, line_prefix_at_position};

/// Offer declared transition names when the cursor is in a `triggers
/// transition <name>` slot. Returns `None` when the cursor isn't in that
/// slot (so the dispatch trunk falls through to the next provider).
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::triggers_transition_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Not in a triggers slot — None.
/// let none = triggers_transition_completions(
///     "feature billing\n",
///     Position { line: 0, character: 0 },
/// );
/// assert!(none.is_none());
/// ```
pub fn triggers_transition_completions(
    source: &str,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);

    if !is_triggers_transition_slot(source, position, before) {
        return None;
    }

    let names = collect_transition_names(source);
    // Drop names already present on the current line so a comma-separated
    // `triggers transition a, <|>` doesn't re-offer `a`.
    let already: HashSet<&str> = before
        .rsplit([' ', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    Some(
        names
            .into_iter()
            .filter(|name| !already.contains(name.as_str()))
            .map(|name| CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("declared lifecycle transition".to_owned()),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Is the cursor positioned where a transition *name* goes in a `triggers
/// transition <name>` slot?
///
/// Two accepted shapes:
/// 1. **Inline** — the line (up to the cursor) contains `triggers
///    transition ` and the cursor sits in the name list after it
///    (`command ... triggers transition <|>` / `... transition a, <|>`).
/// 2. **Block child** — the current line is `transition <|>` *and* the
///    enclosing block header is a `triggers` block (the
///    `triggers` / `transition X` multi-line form).
fn is_triggers_transition_slot(source: &str, position: Position, before: &str) -> bool {
    let trimmed = before.trim_start();

    // Shape 1 — inline `... triggers transition <name list>`.
    if let Some(after) = last_triggers_transition_tail(trimmed) {
        // The cursor must be in the name region (no nested keyword started),
        // i.e. the tail is a partial name / blank / comma-separated list.
        if is_name_list_region(after) {
            return true;
        }
    }

    // Shape 2 — `transition <name>` child of a `triggers` block.
    if let Some(rest) = trimmed.strip_prefix("transition ").or_else(|| {
        if trimmed == "transition" {
            Some("")
        } else {
            None
        }
    }) && is_name_list_region(rest)
        && enclosing_block_is_triggers(source, position)
    {
        return true;
    }

    false
}

/// Return the substring after the *last* `triggers transition ` marker on
/// the (trimmed) line prefix, or `None` when the marker is absent.
fn last_triggers_transition_tail(trimmed_before: &str) -> Option<&str> {
    const MARKER: &str = "triggers transition ";
    trimmed_before
        .rfind(MARKER)
        .map(|idx| &trimmed_before[idx + MARKER.len()..])
}

/// `true` when `region` is a transition-name list region: empty, a partial
/// identifier, or a comma-separated list of identifiers (with optional
/// trailing whitespace). Rejects anything that introduced a different
/// token (so we don't fire inside an unrelated trailing keyword).
fn is_name_list_region(region: &str) -> bool {
    region
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ',' || c == ' ')
}

/// Walk backwards from the cursor line to the nearest shallower header and
/// check whether it is a `triggers` block. Mirrors `block_kind_at`'s
/// indent walk.
fn enclosing_block_is_triggers(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_idx = position.line as usize;
    let cursor_line = lines.get(cursor_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);

    for idx in (0..cursor_idx).rev() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) < cursor_indent {
            return trimmed == "triggers" || trimmed.starts_with("triggers ");
        }
    }
    false
}

/// Collect every `transition <name>` declared inside a `lifecycle` block in
/// the document. Cheap text-scan: track lifecycle-block scope by indent
/// (same convention as `collect_namespace_names`), and within it pick up
/// `transition <name>` header lines.
///
/// Excludes `triggers`-block `transition <name>` lines (those are
/// *references*, not declarations) by only scanning inside a `lifecycle`
/// block.
pub(crate) fn collect_transition_names(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut inside_lifecycle = false;
    let mut block_indent: usize = 0;

    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);

        if trimmed == "lifecycle" || trimmed.starts_with("lifecycle ") {
            inside_lifecycle = true;
            block_indent = indent;
            continue;
        }

        if inside_lifecycle {
            if indent <= block_indent {
                // Left the lifecycle block. This same line might *open*
                // another lifecycle block — re-check below.
                inside_lifecycle = false;
                if trimmed == "lifecycle" || trimmed.starts_with("lifecycle ") {
                    inside_lifecycle = true;
                    block_indent = indent;
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("transition ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() && seen.insert(name.clone()) {
                    names.push(name);
                }
            }
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "feature billing\n  domain\n    resource Invoice\n      lifecycle status\n        state draft initial\n        state issued\n        transition issue\n          from draft\n          to issued\n        transition void\n          from issued\n          to voided\n  command issue_invoice\n    triggers transition \n";

    #[test]
    fn collects_lifecycle_transition_names() {
        let names = collect_transition_names(FIXTURE);
        assert!(names.contains(&"issue".to_owned()), "names = {names:?}");
        assert!(names.contains(&"void".to_owned()), "names = {names:?}");
        assert_eq!(names.len(), 2, "only the two declared transitions");
    }

    #[test]
    fn inline_triggers_slot_offers_declared_transitions() {
        // Cursor right after `triggers transition ` on line 13 (0-based).
        let items = triggers_transition_completions(
            FIXTURE,
            Position {
                line: 13,
                character: 24,
            },
        )
        .expect("triggers transition slot should fire");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"issue"), "labels = {labels:?}");
        assert!(labels.contains(&"void"), "labels = {labels:?}");
    }

    #[test]
    fn comma_list_drops_already_named() {
        let source = "feature b\n  domain\n    resource Invoice\n      lifecycle status\n        transition issue\n        transition void\n  command c\n    triggers transition issue, \n";
        let items = triggers_transition_completions(
            source,
            Position {
                line: 7,
                character: 31,
            },
        )
        .expect("slot fires");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"void"), "labels = {labels:?}");
        assert!(
            !labels.contains(&"issue"),
            "already-named `issue` should be dropped: {labels:?}"
        );
    }

    #[test]
    fn block_form_transition_child_offers_transitions() {
        // The `triggers` block with `transition <name>` children form.
        let source = "feature b\n  domain\n    resource Invoice\n      lifecycle status\n        transition issue\n        transition void\n  command c\n    triggers\n      transition \n";
        let items = triggers_transition_completions(
            source,
            Position {
                line: 8,
                character: 17,
            },
        )
        .expect("block-form transition child should fire");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"issue"), "labels = {labels:?}");
        assert!(labels.contains(&"void"), "labels = {labels:?}");
    }

    #[test]
    fn no_fire_outside_triggers_slot() {
        // Cursor on a plain feature line.
        assert!(
            triggers_transition_completions(
                FIXTURE,
                Position {
                    line: 0,
                    character: 5,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn lifecycle_transition_decl_line_does_not_fire() {
        // A `transition <name>` line that is a *declaration* inside a
        // lifecycle block (not a triggers child) must NOT offer completion.
        let items = triggers_transition_completions(
            FIXTURE,
            Position {
                line: 6,
                character: 19,
            },
        );
        assert!(
            items.is_none(),
            "declaration site should not offer triggers completion"
        );
    }
}
