//! Completion provider for `input.<field>` references inside a
//! `command` block (Cell A4).
//!
//! Surfaces both `command.route` slots (typed `route <name>: <Type>`
//! lines) and `command.input` slots (short `input a, b, c` form and
//! the typed `input` block) so authors don't hit "field not found" at
//! codegen time when they reach for a route parameter inside
//! `effect.bindings` / `effect.where`.
//!
//! **Ordering**: route params are surfaced *first* (sort_text `0_*`)
//! so the IDE puts them at the top of the completion list — that
//! matches the struct-field order Cell A1/A2 used at the IR layer and
//! gives authors an at-a-glance hint that the `id` they want lives on
//! `route`, not `input`. Each item carries a detail label that
//! distinguishes route-param vs input-field at-a-glance.
//!
//! Trigger: the line prefix ends with `input.<optional-partial>` and
//! the cursor sits inside a `command` block (any indent depth —
//! covers both the `name = input.id` binding shape at indent 6 and
//! any future `where (id = input.id)` expression on the command body
//! at indent 4).
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::leading_spaces`] / [`crate::line_prefix_at_position`]
//!   for indent and cursor-prefix discipline.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::{leading_spaces, line_prefix_at_position};

/// Surfaces both `command.route` slots (typed `route <name>: <Type>` lines)
/// and `command.input` slots (short `input a, b, c` form and the typed
/// `input` block) so authors don't hit "field not found" at codegen time
/// when they reach for a route parameter inside `effect.bindings` /
/// `effect.where`.
///
/// **Ordering**: route params are surfaced *first* (sort_text `0_*`) so
/// the IDE puts them at the top of the completion list — that matches
/// the struct-field order Cell A1/A2 used at the IR layer and gives
/// authors an at-a-glance hint that the `id` they want lives on `route`,
/// not `input`. Each item carries a detail label that distinguishes
/// route-param vs input-field at-a-glance.
///
/// Trigger: the line prefix ends with `input.<optional-partial>` and the
/// cursor sits inside a `command` block (any indent depth — covers both
/// the `name = input.id` binding shape at indent 6 and any future
/// `where (id = input.id)` expression on the command body at indent 4).
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::input_field_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Cursor not at an `input.<...>` trigger — None.
/// let result = input_field_completions(
///     "feature billing\n",
///     Position { line: 0, character: 6 },
/// );
/// assert!(result.is_none());
/// ```
pub fn input_field_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trigger = input_dot_trigger(before)?;
    let _ = trigger; // accepted; partial identifier text not needed for filtering — client filters.

    let (route_params, input_fields) = collect_command_input_and_route_params(source, position)?;

    if route_params.is_empty() && input_fields.is_empty() {
        return None;
    }

    let mut items: Vec<CompletionItem> = Vec::new();

    for (idx, name) in route_params.iter().enumerate() {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some("route param".to_owned()),
            sort_text: Some(format!("0_{:03}_{name}", idx)),
            ..CompletionItem::default()
        });
    }

    for (idx, name) in input_fields.iter().enumerate() {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some("input field".to_owned()),
            sort_text: Some(format!("1_{:03}_{name}", idx)),
            ..CompletionItem::default()
        });
    }

    Some(items)
}

/// Returns `Some(partial_identifier_text)` when the line prefix ends with
/// `input.<partial>` (partial may be empty). Returns `None` otherwise.
pub(crate) fn input_dot_trigger(before: &str) -> Option<&str> {
    let dot = before.rfind("input.")?;
    let after_dot = &before[dot + "input.".len()..];
    if !after_dot
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    // The `input.` token must be at a word boundary — guard against a
    // suffix match inside a longer identifier like `payload_input.x`.
    let prefix = &before[..dot];
    if let Some(last) = prefix.chars().last() {
        if last.is_ascii_alphanumeric() || last == '_' {
            return None;
        }
    }
    Some(after_dot)
}

/// Walk source backward from `position` to locate the enclosing
/// `command <name>` header, then walk forward gathering its `route` slot
/// names (in source order) and `input` field names (short + typed). Stops
/// at the next sibling/parent block.
pub(crate) fn collect_command_input_and_route_params(
    source: &str,
    position: Position,
) -> Option<(Vec<String>, Vec<String>)> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line = (position.line as usize).min(lines.len().saturating_sub(1));

    let mut command_start: Option<usize> = None;
    let mut command_indent: usize = 0;
    for idx in (0..=cursor_line).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("command ")
            || trimmed == "command"
            || trimmed.starts_with("command\t")
        {
            command_start = Some(idx);
            command_indent = leading_spaces(line);
            break;
        }
    }
    let start = command_start?;
    let child_indent = command_indent + 2;
    let body_indent = command_indent + 4;

    let mut route_params: Vec<String> = Vec::new();
    let mut input_fields: Vec<String> = Vec::new();
    let mut in_typed_input_block = false;

    for line in lines.iter().skip(start + 1) {
        let trimmed = line.trim_start();
        let indent = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Reached the next sibling/parent block — stop scanning.
        if indent <= command_indent {
            break;
        }

        if indent == child_indent {
            in_typed_input_block = false;

            if let Some(rest) = trimmed.strip_prefix("route ") {
                // Accept both `route id: ID` (named slot) and a bare
                // `route` (unlikely but safe to skip).
                let name = rest
                    .split(|c: char| c == ':' || c.is_ascii_whitespace())
                    .find(|t| !t.is_empty())
                    .unwrap_or("");
                if !name.is_empty() && route_params.iter().all(|n| n != name) {
                    route_params.push(name.to_owned());
                }
                continue;
            }

            if trimmed == "input" {
                in_typed_input_block = true;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("input ") {
                for field in rest.split(',').map(str::trim) {
                    if field.is_empty() {
                        continue;
                    }
                    // Reject typed-pair shape `name: Type` at this slot —
                    // doctor already warns; we just skip.
                    if field.contains(':') {
                        continue;
                    }
                    if field
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        && input_fields.iter().all(|n| n != field)
                    {
                        input_fields.push(field.to_owned());
                    }
                }
                continue;
            }

            continue;
        }

        if in_typed_input_block && indent == body_indent {
            // `<name>: <Type>` declarations.
            if let Some((name, _)) = trimmed.split_once(':') {
                let name = name.trim();
                if !name.is_empty()
                    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && input_fields.iter().all(|n| n != name)
                {
                    input_fields.push(name.to_owned());
                }
            }
        }
    }

    Some((route_params, input_fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_trigger_detects_partial() {
        assert_eq!(input_dot_trigger("name = input.i"), Some("i"));
        assert_eq!(input_dot_trigger("name = input."), Some(""));
    }

    #[test]
    fn dot_trigger_rejects_non_input_dot() {
        assert!(input_dot_trigger("name = thing.id").is_none());
        // Word boundary guard — `payload_input.x` should NOT trigger.
        assert!(input_dot_trigger("foo = payload_input.x").is_none());
    }

    #[test]
    fn completions_return_none_outside_command_block() {
        let result = input_field_completions(
            "feature billing\n",
            Position { line: 0, character: 6 },
        );
        assert!(result.is_none());
    }
}
