//! Block-context helpers for the `lifecycle <field>` resource-child
//! vocabulary (proposal §3).
//!
//! These are intentionally separate from `diagnostics::lifecycle::*`,
//! which targets the route-guard family (`requires_lifecycle X = …`,
//! `resume` arms, `on_lifecycle_pending`). This module is about the
//! **block proper** — the lifecycle declaration that lives as a child
//! of `resource <Name>` and declares `state` / `transition` /
//! `invariant` / `invariant_handler` children.
//!
//! The single public concern is "is the cursor sitting inside a
//! `lifecycle <field>` block?". Hover and completion providers both
//! gate their fires on that answer so unrelated tokens (e.g. a `state`
//! identifier in a documentation comment elsewhere) don't surface
//! lifecycle-block content.

use tower_lsp::lsp_types::Position;

use crate::leading_spaces;

/// Block fact for the `lifecycle <field>` resource child.
///
/// `header_line` is the 0-indexed source line containing
/// `lifecycle <field>`. `header_indent` is the leading-space count of
/// that header — children sit at `header_indent + 2`. `field_name` is
/// the discriminator field (e.g. `status` for `lifecycle status`).
#[derive(Debug, Clone)]
pub(crate) struct LifecycleBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) field_name: String,
    pub(crate) end_line: usize,
}

/// Returns the enclosing `lifecycle <field>` block when `position`
/// sits at-or-after the `lifecycle` header line AND at greater indent
/// than the header (i.e. inside the block body) — OR exactly on the
/// header line itself. Returns `None` outside any lifecycle block.
///
/// Walks upward from the cursor, skipping blank / comment lines, and
/// stops at the first less-or-equal-indent line that doesn't start
/// with `lifecycle ` — same shape as `enclosing_named_block` in the
/// route-guard module.
pub(crate) fn enclosing_lifecycle_block(
    source: &str,
    position: Position,
) -> Option<LifecycleBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);

    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if idx == cursor_line_idx {
            if let Some(rest) = trimmed.strip_prefix("lifecycle ") {
                if let Some(field) = lifecycle_block_field(rest) {
                    let end_line = find_lifecycle_block_end(&lines, idx, indent);
                    return Some(LifecycleBlock {
                        header_line: idx,
                        header_indent: indent,
                        field_name: field,
                        end_line,
                    });
                }
            }
            continue;
        }
        if indent >= cursor_indent {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("lifecycle ") {
            if let Some(field) = lifecycle_block_field(rest) {
                let end_line = find_lifecycle_block_end(&lines, idx, indent);
                return Some(LifecycleBlock {
                    header_line: idx,
                    header_indent: indent,
                    field_name: field,
                    end_line,
                });
            }
        }
        if indent == 0 {
            return None;
        }
    }
    None
}

/// Discriminator-field extractor. `lifecycle status of Foo` (the
/// inline shorthand surfaced by the route-guard `state.rs` collector)
/// is intentionally NOT recognised here — that shorthand is a route
/// concern, not the block-vocab concern this module targets.
fn lifecycle_block_field(rest: &str) -> Option<String> {
    let first = rest.split_whitespace().next()?;
    if first.is_empty() || !first.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    // The shorthand `lifecycle status of <Resource>` is parsed
    // elsewhere; the block-vocab applies only to the canonical
    // resource-child form `lifecycle <field>` with no `of ...` tail.
    let tail = rest[first.len()..].trim_start();
    if tail.starts_with("of ") {
        return None;
    }
    Some(first.to_owned())
}

fn find_lifecycle_block_end(lines: &[&str], header_line: usize, header_indent: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(header_line + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= header_indent {
            return idx;
        }
    }
    lines.len()
}

/// Returns the enclosing `transition <name>` block within the parent
/// `lifecycle` block, if any. Used by hover/completion to know we are
/// authoring a transition child (where `from` / `to` / `policy` /
/// `audit` / `timestamps` / `emits` / `requires` / `tests` are legal).
pub(crate) fn enclosing_transition_block(
    source: &str,
    position: Position,
    lifecycle: &LifecycleBlock,
) -> Option<TransitionBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let child_indent = lifecycle.header_indent + 2;

    for idx in (lifecycle.header_line + 1..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < child_indent {
            return None;
        }
        if indent == child_indent && trimmed.starts_with("transition ") {
            let name = trimmed
                .strip_prefix("transition ")
                .and_then(|rest| rest.split_whitespace().next())
                .unwrap_or("")
                .to_owned();
            return Some(TransitionBlock {
                header_line: idx,
                header_indent: indent,
                name,
            });
        }
    }
    None
}

#[derive(Debug, Clone)]
pub(crate) struct TransitionBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    #[allow(dead_code)]
    pub(crate) name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lifecycle_block_at_header_line() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n";
        let block = enclosing_lifecycle_block(
            &source,
            Position {
                line: 3,
                character: 6,
            },
        )
        .expect("lifecycle header line");
        assert_eq!(block.field_name, "status");
        assert_eq!(block.header_line, 3);
        assert_eq!(block.header_indent, 6);
    }

    #[test]
    fn detects_lifecycle_block_inside_body() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        invariant terminal_immutable\n";
        let block = enclosing_lifecycle_block(
            source,
            Position {
                line: 5,
                character: 8,
            },
        )
        .expect("inside body");
        assert_eq!(block.field_name, "status");
    }

    #[test]
    fn returns_none_outside_any_lifecycle() {
        let source =
            "feature billing\n  domain\n    resource Publication\n      field name: Text\n";
        assert!(
            enclosing_lifecycle_block(
                source,
                Position {
                    line: 3,
                    character: 6
                },
            )
            .is_none()
        );
    }

    #[test]
    fn ignores_inline_shorthand_lifecycle_status_of() {
        // `lifecycle status of Foo` is the route-guard shorthand
        // handled by diagnostics::lifecycle, not the block-vocab.
        let source = "feature billing\n  lifecycle status of Publication\n    draft\n";
        assert!(
            enclosing_lifecycle_block(
                source,
                Position {
                    line: 1,
                    character: 2
                },
            )
            .is_none()
        );
    }
}
