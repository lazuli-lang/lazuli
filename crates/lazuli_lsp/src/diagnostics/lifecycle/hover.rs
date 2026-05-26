//! Hover content for lifecycle-related tokens.
//!
//! `lifecycle_gate_hover` is the public entry point consumed by
//! `lib.rs::server`. It dispatches by token shape:
//!
//! * `→` / `->` inside a `resume` block → arrow hover.
//! * `*` inside a `resume` block → wildcard hover.
//! * `source query.lookup` after the `source` / `query.lookup` token →
//!   source-query hover.
//! * `requires_lifecycle` / `on_lifecycle_pending` / `resume` / `none` →
//!   matching constant hover.
//! * Anything else → resolved gate hover that prints the bound
//!   resource, gated state, pending resume, and declared states.
//!
//! The hover constants live here because every hover string is
//! produced by this module; placing them next door to the dispatcher
//! keeps the wording in one place.

use tower_lsp::lsp_types::Position;

use crate::{byte_index_for_utf16_position, enclosing_view_block};

use super::gate::{lifecycle_pending_resume_for_view, lifecycle_resource_for_name};
use super::resume::enclosing_lifecycle_resume_block;

pub(crate) const LIFECYCLE_REQUIRES_HOVER: &str = "Gate this view on the actor's `<Resource>.lifecycle_state`. Codegen emits a TanStack `beforeLoad` that fetches the source query and redirects via `@resume` on mismatch.";
pub(crate) const LIFECYCLE_PENDING_HOVER: &str = "Name of the `resume <name>` block to redirect through when `requires_lifecycle` doesn't match.";
pub(crate) const LIFECYCLE_RESUME_HOVER: &str = "Block declaring how to route a user whose lifecycle state of a particular resource is mid-flow.";
pub(crate) const LIFECYCLE_SOURCE_QUERY_HOVER: &str = "The lookup query that fetches the actor's row of the resource. Must return a single record OR not-found (404).";
pub(crate) const LIFECYCLE_NONE_HOVER: &str =
    "Arm matched when the source query returns 404 (the actor's row doesn't exist yet).";
pub(crate) const LIFECYCLE_WILDCARD_HOVER: &str = "Catch-all arm. Matches any state not explicitly listed. Required when `resume` arms don't cover every state in the lifecycle, OR for forward-compatibility.";
pub(crate) const LIFECYCLE_ARROW_HOVER: &str = "Arrow token mapping a lifecycle state arm to a target view in a `resume` block. Both Unicode `→` and ASCII `->` accepted.";

/// Public hover entry point for the IR Lifecycle Route-Gate contract.
/// Returns a Markdown string explaining the cursor's token in
/// context — arrow / wildcard arms inside a `resume` block, the
/// `source query.lookup` slot, the four keyword tokens
/// (`requires_lifecycle`, `on_lifecycle_pending`, `resume`, `none`),
/// or the resolved gate hover that prints the bound resource and
/// declared states. Returns `None` for any token outside the
/// lifecycle-gate surface.
pub fn lifecycle_gate_hover(
    source: &str,
    position: Position,
    word: Option<&str>,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    if lifecycle_hover_is_arrow(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`→` / `->`\n\n{LIFECYCLE_ARROW_HOVER}"));
    }

    if lifecycle_hover_is_wildcard(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`*`\n\n{LIFECYCLE_WILDCARD_HOVER}"));
    }

    let word = word?;
    if word == "source" || word == "query.lookup" {
        if line.trim_start().starts_with("source query.lookup ")
            && enclosing_lifecycle_resume_block(source, position).is_some()
        {
            return Some(format!(
                "`source query.lookup`\n\n{LIFECYCLE_SOURCE_QUERY_HOVER}"
            ));
        }
    }

    match word {
        "requires_lifecycle" if enclosing_view_block(source, position).is_some() => Some(format!(
            "`requires_lifecycle`\n\n{LIFECYCLE_REQUIRES_HOVER}"
        )),
        "on_lifecycle_pending" if enclosing_view_block(source, position).is_some() => Some(
            format!("`on_lifecycle_pending`\n\n{LIFECYCLE_PENDING_HOVER}"),
        ),
        "resume" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`resume`\n\n{LIFECYCLE_RESUME_HOVER}"))
        }
        "none" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`none`\n\n{LIFECYCLE_NONE_HOVER}"))
        }
        _ => lifecycle_resolved_gate_hover(source, position, word),
    }
}

pub(crate) fn lifecycle_hover_is_wildcard(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let bytes = line.as_bytes();
    (index < bytes.len() && bytes[index] == b'*') || (index > 0 && bytes[index - 1] == b'*')
}

pub(crate) fn lifecycle_hover_is_arrow(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let before = &line[..index.min(line.len())];
    let after = &line[index.min(line.len())..];
    before.ends_with("->")
        || after.starts_with("->")
        || before.ends_with('→')
        || after.starts_with('→')
}

pub(crate) fn lifecycle_resolved_gate_hover(
    source: &str,
    position: Position,
    word: &str,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("requires_lifecycle ")?;
    let (resource, state) = rest.split_once('=')?;
    let resource = resource.trim();
    let state = state.split_whitespace().next().unwrap_or("");
    if word != state || state.is_empty() {
        return None;
    }
    let view = enclosing_view_block(source, position)?;
    let resume_name =
        lifecycle_pending_resume_for_view(source, &view).unwrap_or_else(|| "<resume>".to_owned());
    let states = lifecycle_resource_for_name(source, view.feature_hint.as_deref(), resource)
        .map(|resource| resource.states.join(", "))
        .unwrap_or_else(|| "unresolved".to_owned());
    Some(format!(
        "currently the view requires `{resource}.lifecycle_state = {state}`. On mismatch, redirects via `resume {resume_name}`. Lifecycle states declared: `{states}`."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_hover_for_requires_lifecycle_in_view_block() {
        let source = "feature billing\n  view checkout\n    requires_lifecycle account = active\n";
        let hover = lifecycle_gate_hover(
            source,
            Position {
                line: 2,
                character: 4,
            },
            Some("requires_lifecycle"),
        );
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("requires_lifecycle"));
    }

    #[test]
    fn returns_none_for_unrelated_word() {
        let source = "feature billing\n  view checkout\n    title \"Checkout\"\n";
        assert!(lifecycle_gate_hover(
            source,
            Position { line: 2, character: 4 },
            Some("title")
        )
        .is_none());
    }
}
