//! Hover content for the `lifecycle <field>` resource-child block
//! vocabulary (proposal §3).
//!
//! Every hover fires only when the cursor sits inside an enclosing
//! `lifecycle <field>` block (per `enclosing_lifecycle_block` in
//! `context.rs`). This narrowness is what makes the surface safe to
//! share keywords (`state`, `from`, `to`, `audit`, `timestamps`) with
//! unrelated families elsewhere in the document: a stray `state`
//! identifier in a doc comment or test fixture does not fire these
//! hovers.
//!
//! Constants live alongside the dispatcher so the wording stays in one
//! place. Each constant carries a brief description plus a pointer back
//! to `docs/proposals/lifecycle-vocab.md §X` so editor users can drill
//! into the canonical reference.

use tower_lsp::lsp_types::Position;

use super::context::{enclosing_lifecycle_block, enclosing_transition_block};

// ── lifecycle-block top-level keywords (proposal §3.1–§3.4) ───────────────────

pub(crate) const LIFECYCLE_BLOCK_HEADER_HOVER: &str = "Declares the lifecycle state machine for this resource. The discriminator field carries the active state; codegen emits enum + transition guard + invariant checks. See `docs/proposals/lifecycle-vocab.md` §3.";

pub(crate) const LIFECYCLE_STATE_HOVER: &str = "Declare a state in the lifecycle. Suffix with `initial` to mark the entry state, or `terminal` to mark an absorbing state with no outgoing transitions. See `docs/proposals/lifecycle-vocab.md` §3.2.";

pub(crate) const LIFECYCLE_TRANSITION_HOVER: &str = "Declare a named transition between states. Children: `from`, `to`, `policy`, `audit`, `timestamps`, `emits`, `requires`, `tests`. Codegen emits a guarded command that enforces the from-set and writes the to-state. See `docs/proposals/lifecycle-vocab.md` §3.3.";

pub(crate) const LIFECYCLE_INVARIANT_HOVER: &str = "Declare an invariant from the closed catalog (`terminal_immutable`, `single <state> per <scope>`, `no_jump_more_than_one`). Codegen lowers each to a runtime assertion in the transition guard. See `docs/proposals/lifecycle-vocab.md` §3.4.";

pub(crate) const LIFECYCLE_INVARIANT_HANDLER_HOVER: &str = "Bind a custom handler invoked when an invariant fails. Receives the transition context and may surface a user-facing error or roll back. See `docs/proposals/lifecycle-vocab.md` §3.4.";

pub(crate) const LIFECYCLE_AUDIT_HOVER: &str = "Emit an audit-log entry on each transition. Codegen wires this into the resource audit stream with the actor, source state, target state, and timestamp. See `docs/proposals/lifecycle-vocab.md` §3.3.";

pub(crate) const LIFECYCLE_TIMESTAMPS_HOVER: &str = "Stamp each transition into a per-state `<state>_at: DateTime` column on the resource. Codegen materializes the columns and writes them inside the transition guard. See `docs/proposals/lifecycle-vocab.md` §3.3.";

pub(crate) const LIFECYCLE_TESTS_HOVER: &str = "Inline test block scoped to this lifecycle / transition. Runner enumerates the from→to assertions against the lowered state machine. See `docs/proposals/lifecycle-vocab.md` §3.5.";

// ── state modifiers (proposal §3.2) ────────────────────────────────────────────

pub(crate) const LIFECYCLE_INITIAL_HOVER: &str = "Marks a state as the entry state — newly created resources start here. Exactly one state per lifecycle may carry `initial`. See `docs/proposals/lifecycle-vocab.md` §3.2.";

pub(crate) const LIFECYCLE_TERMINAL_HOVER: &str = "Marks a state as absorbing — no outgoing transitions are allowed and the row becomes immutable when paired with `invariant terminal_immutable`. See `docs/proposals/lifecycle-vocab.md` §3.2.";

// ── transition children (proposal §3.3) ────────────────────────────────────────

pub(crate) const LIFECYCLE_FROM_HOVER: &str = "Source state(s) the transition may fire from. Multiple `from` lines fan-in; codegen emits an `IN (...)` guard. See `docs/proposals/lifecycle-vocab.md` §3.3.";

pub(crate) const LIFECYCLE_TO_HOVER: &str = "Target state the transition writes. Exactly one `to` per transition. See `docs/proposals/lifecycle-vocab.md` §3.3.";

// ── closed invariant-catalog vocabulary (proposal §3.4) ────────────────────────

pub(crate) const LIFECYCLE_TERMINAL_IMMUTABLE_HOVER: &str = "Closed catalog: forbids any column edit once the row enters a `terminal` state. Pairs with `state X terminal` to lock history. See `docs/proposals/lifecycle-vocab.md` §3.4.";

pub(crate) const LIFECYCLE_SINGLE_PER_HOVER: &str = "Closed catalog: at most one row per scope may sit in the named state at a time (e.g. `single active per organization`). Codegen emits a partial unique index. See `docs/proposals/lifecycle-vocab.md` §3.4.";

pub(crate) const LIFECYCLE_NO_JUMP_HOVER: &str = "Closed catalog: forbids transitions that skip more than one state in the declared order. Requires a linear (non-branching) lifecycle. See `docs/proposals/lifecycle-vocab.md` §3.4.";

/// Public hover entry point for the `lifecycle <field>` block surface.
///
/// Returns a Markdown hover when the cursor sits inside an enclosing
/// `lifecycle <field>` block AND the token matches one of the
/// vocabulary keywords. Returns `None` for any other context — so
/// stray `state` / `from` / `to` identifiers outside lifecycle blocks
/// don't surface block-vocab content.
///
/// The dispatcher prefers narrower contexts: invariant-catalog members
/// (`terminal_immutable`, `single`, `per`, `no_jump_more_than_one`)
/// fire only when the line starts with `invariant `; `from` / `to` /
/// `audit` / `timestamps` fire only inside an enclosing `transition`
/// block.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::lifecycle_block_hover;
/// use tower_lsp::lsp_types::Position;
///
/// // No enclosing lifecycle block — None.
/// let hover = lifecycle_block_hover(
///     "feature billing\n",
///     Position { line: 0, character: 0 },
///     Some("state"),
/// );
/// assert!(hover.is_none());
/// ```
pub fn lifecycle_block_hover(
    source: &str,
    position: Position,
    word: Option<&str>,
) -> Option<String> {
    let Some(block) = enclosing_lifecycle_block(source, position) else {
        return None;
    };
    let word = word?;
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let trimmed_line = line.trim_start();

    // The `lifecycle` keyword itself fires only on the header line —
    // anywhere else inside the block, the cursor sits on a child.
    if word == "lifecycle" && (position.line as usize) == block.header_line {
        return Some(format!(
            "`lifecycle {}`\n\n{}",
            block.field_name, LIFECYCLE_BLOCK_HEADER_HOVER
        ));
    }

    // Invariant catalog members fire when the line starts with
    // `invariant `; otherwise these tokens fall through to nothing.
    if trimmed_line.starts_with("invariant ") {
        match word {
            "terminal_immutable" => {
                return Some(format!(
                    "`terminal_immutable`\n\n{LIFECYCLE_TERMINAL_IMMUTABLE_HOVER}"
                ));
            }
            "single" | "per" => {
                return Some(format!(
                    "`single <state> per <scope>`\n\n{LIFECYCLE_SINGLE_PER_HOVER}"
                ));
            }
            "no_jump_more_than_one" => {
                return Some(format!(
                    "`no_jump_more_than_one`\n\n{LIFECYCLE_NO_JUMP_HOVER}"
                ));
            }
            _ => {}
        }
    }

    // Transition-child keywords fire only inside an enclosing
    // `transition` block. Outside one, the same tokens would be either
    // ambiguous or belong to another family.
    let in_transition = enclosing_transition_block(source, position, &block).is_some();

    match word {
        "state" => Some(format!("`state`\n\n{LIFECYCLE_STATE_HOVER}")),
        "transition" => Some(format!("`transition`\n\n{LIFECYCLE_TRANSITION_HOVER}")),
        "invariant" => Some(format!("`invariant`\n\n{LIFECYCLE_INVARIANT_HOVER}")),
        "invariant_handler" => Some(format!(
            "`invariant_handler`\n\n{LIFECYCLE_INVARIANT_HANDLER_HOVER}"
        )),
        "audit" => Some(format!("`audit`\n\n{LIFECYCLE_AUDIT_HOVER}")),
        "timestamps" => Some(format!("`timestamps`\n\n{LIFECYCLE_TIMESTAMPS_HOVER}")),
        "tests" => Some(format!("`tests`\n\n{LIFECYCLE_TESTS_HOVER}")),
        "initial" => Some(format!("`initial`\n\n{LIFECYCLE_INITIAL_HOVER}")),
        "terminal" => Some(format!("`terminal`\n\n{LIFECYCLE_TERMINAL_HOVER}")),
        "from" if in_transition => Some(format!("`from`\n\n{LIFECYCLE_FROM_HOVER}")),
        "to" if in_transition => Some(format!("`to`\n\n{LIFECYCLE_TO_HOVER}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_for_state_keyword_inside_lifecycle_block() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 4,
                character: 8,
            },
            Some("state"),
        );
        assert!(hover.is_some(), "state keyword should hover inside block");
        let text = hover.unwrap();
        assert!(text.contains("state"));
        assert!(text.contains("§3.2"));
    }

    #[test]
    fn hover_for_transition_keyword_inside_lifecycle_block() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        transition publish\n          from draft\n          to published\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 6,
                character: 8,
            },
            Some("transition"),
        );
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("transition"));
    }

    #[test]
    fn hover_for_closed_invariant_catalog_terminal_immutable() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        invariant terminal_immutable\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 6,
                character: 18,
            },
            Some("terminal_immutable"),
        );
        assert!(
            hover.is_some(),
            "terminal_immutable in `invariant` line should hover"
        );
        assert!(hover.unwrap().contains("terminal_immutable"));
    }

    #[test]
    fn hover_for_no_jump_more_than_one_invariant() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        invariant no_jump_more_than_one\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 6,
                character: 18,
            },
            Some("no_jump_more_than_one"),
        );
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("linear"));
    }

    #[test]
    fn from_keyword_hovers_only_inside_transition_block() {
        let source = "feature billing\n  domain\n    resource Publication\n      lifecycle status\n        state draft initial\n        state published terminal\n        transition publish\n          from draft\n          to published\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 7,
                character: 10,
            },
            Some("from"),
        );
        assert!(hover.is_some(), "from inside transition block should hover");
        assert!(hover.unwrap().contains("Source state"));
    }

    #[test]
    fn returns_none_outside_lifecycle_block() {
        let source =
            "feature billing\n  domain\n    resource Publication\n      field name: Text\n";
        let hover = lifecycle_block_hover(
            source,
            Position {
                line: 3,
                character: 6,
            },
            Some("state"),
        );
        assert!(hover.is_none());
    }
}
