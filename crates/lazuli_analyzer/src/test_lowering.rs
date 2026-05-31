//! Shared `tests { }` lowering — the single path that turns authored
//! test lines into [`ir::TestAssertion`]s for every construct that
//! carries a `tests { }` block.
//!
//! ## Why this module exists (Bug A)
//!
//! Before this, only the lifecycle slot lowered `tests { }` lines (via a
//! private `lower_tests`/`lower_test_line` pair in `lifecycle/mod.rs`).
//! `command` `tests` were dropped on the floor — `lower_command_decl`
//! hardcoded `tests: None`, so an authored
//! `command ... tests { allows as @role.admin }` never reached the IR.
//! The two consumers that read `ir::Command.tests` —
//! `lazuli_doctor::vocab::vocab_tests_missing_001` (counts a feature as
//! tested only when a substantive [`ir::TestBlock`] exists) and
//! `lazuli_doctor::coverage::spec_actor_matrix` (credits an
//! `AllowsAs` / `DeniesAs` row against the command's actor matrix) —
//! therefore never saw command test evidence: the
//! `VOCAB-TESTS-MISSING-001` waiver couldn't be lifted by a real command
//! test, and the actor matrix stayed `0/N`.
//!
//! Funnelling command + lifecycle through ONE lowering closes that gap
//! and keeps the authorable verb grammar in a single place.
//!
//! ## Closed assertion catalog
//!
//! The match arms mirror [`ir::TestAssertion`]. Lines the analyzer does
//! not recognise lower to nothing, so a block of only comments /
//! malformed lines yields `None` — which keeps the doctor's "substantive
//! block" accounting honest (an empty / garbage block is not coverage).
//!
//! The `allows when <pred>` / `denies when <pred>` forms carry a typed
//! closed [`ir::Predicate`] (a closed enum — `Comparison`/`Has`/`And`/`Or`,
//! no raw-text variant). Lowering them *faithfully* needs the closed
//! predicate parser, which is out of scope for this mechanical projection.
//!
//! UNTIL that parser lands (spec 0012, 2026-05-31): rather than DROP these
//! lines — which left an authored `tests` block reading empty and made
//! `VOCAB-TESTS-MISSING-001` false-fire on every feature that wrote only
//! `when`-predicate tests (pauta BT-03/BT-11; both pilots waived it) — they
//! lower to the sanctioned `TestAssertion::Raw` fallback. `Raw` preserves
//! the authored line verbatim so the doctor's `block_has_substance` check
//! counts it as real coverage, without fabricating a bogus typed
//! `Predicate`. The actor-matrix (`as` / `from`) forms still lower to their
//! typed variants and are the only rows `spec_actor_matrix` credits; `Raw`
//! rows carry no typed actor and are ignored there. Comments and malformed
//! lines still lower to nothing (an empty/garbage block is not coverage).

use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Lower a `tests { }` block's raw lines into an [`ir::TestBlock`].
///
/// Returns `None` when no line lowered to a substantive assertion, so the
/// absence of real coverage is represented as `None` (the invariant the
/// doctor's `block_has_substance` check relies on: a present `TestBlock`
/// always carries at least one assertion).
pub(crate) fn lower_test_block(lines: &[String], span: syntax::Span) -> Option<ir::TestBlock> {
    let assertions: Vec<_> = lines
        .iter()
        .filter_map(|line| lower_test_line(line))
        .collect();
    if assertions.is_empty() {
        return None;
    }
    Some(ir::TestBlock {
        assertions,
        span_ref: Some(ir::SpanRef {
            start: span.start,
            end: span.end,
        }),
    })
}

/// Lower a single authored test line into a typed [`ir::TestAssertion`].
/// The match arms are the closed catalog; anything else lowers to `None`.
pub(crate) fn lower_test_line(line: &str) -> Option<ir::TestAssertion> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        // Combined transition forms (most specific first).
        ["allows", "from", state, "as", actor] => Some(ir::TestAssertion::AllowsFromAs {
            state: (*state).to_owned(),
            actor: (*actor).to_owned(),
        }),
        ["denies", "from", state, "as", actor] => Some(ir::TestAssertion::DeniesFromAs {
            state: (*state).to_owned(),
            actor: (*actor).to_owned(),
        }),
        // State-edge forms (lifecycle transitions).
        ["allows", "from", state] => Some(ir::TestAssertion::AllowsFrom {
            state: (*state).to_owned(),
        }),
        ["denies", "from", state] => Some(ir::TestAssertion::DeniesFrom {
            state: (*state).to_owned(),
        }),
        // Actor-matrix forms (commands + transitions). These are the rows
        // `spec_actor_matrix` credits.
        ["allows", "as", actor] => Some(ir::TestAssertion::AllowsAs {
            actor: (*actor).to_owned(),
        }),
        ["denies", "as", actor] => Some(ir::TestAssertion::DeniesAs {
            actor: (*actor).to_owned(),
        }),
        // `allows when <pred>` / `denies when <pred>` — the typed
        // closed-`Predicate` parser is not yet wired (see module docs).
        // Rather than DROP the line (which left an authored `tests` block
        // reading empty and made VOCAB-TESTS-MISSING-001 false-fire —
        // spec 0012), lower to the sanctioned verbatim `Raw` fallback so
        // the line counts as real coverage without fabricating a bogus
        // typed `Predicate`. `Raw` rows carry no actor, so the actor
        // matrix ignores them.
        ["allows", "when", ..] | ["denies", "when", ..] => Some(ir::TestAssertion::Raw {
            line: line.trim().to_owned(),
        }),
        // Comments / malformed lines are not coverage.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> syntax::Span {
        syntax::Span { start: 0, end: 0 }
    }

    #[test]
    fn lowers_actor_allow_and_deny() {
        let block = lower_test_block(
            &[
                "allows as @role.admin".to_owned(),
                "denies as @role.guest".to_owned(),
            ],
            span(),
        )
        .expect("substantive block lowers to Some");
        assert_eq!(
            block.assertions,
            vec![
                ir::TestAssertion::AllowsAs {
                    actor: "@role.admin".to_owned()
                },
                ir::TestAssertion::DeniesAs {
                    actor: "@role.guest".to_owned()
                },
            ]
        );
    }

    #[test]
    fn lowers_state_forms() {
        let block = lower_test_block(
            &[
                "allows from draft".to_owned(),
                "allows from draft as @role.admin".to_owned(),
            ],
            span(),
        )
        .expect("substantive block");
        assert_eq!(
            block.assertions[0],
            ir::TestAssertion::AllowsFrom {
                state: "draft".to_owned()
            }
        );
        assert_eq!(
            block.assertions[1],
            ir::TestAssertion::AllowsFromAs {
                state: "draft".to_owned(),
                actor: "@role.admin".to_owned()
            }
        );
    }

    #[test]
    fn comments_and_malformed_lower_to_none() {
        assert!(
            lower_test_block(
                &["# just a comment".to_owned(), "garbage line".to_owned()],
                span()
            )
            .is_none()
        );
    }

    // ── spec 0012 (VOCAB-TESTS-MISSING-001 feed fix) ──────────────────────────

    /// `allows when <pred>` / `denies when <pred>` now lower to the
    /// sanctioned `Raw` fallback (non-empty) instead of being dropped.
    #[test]
    fn when_predicates_lower_to_raw_non_empty() {
        assert_eq!(
            lower_test_line("allows when input.amount > 0"),
            Some(ir::TestAssertion::Raw {
                line: "allows when input.amount > 0".to_owned()
            })
        );
        assert_eq!(
            lower_test_line("denies when input.email is_empty"),
            Some(ir::TestAssertion::Raw {
                line: "denies when input.email is_empty".to_owned()
            })
        );
    }

    /// Quiet-on-legit (gate): an inline `tests` block whose lines are all
    /// `when`-predicates lowers to a non-empty `TestBlock` — the exact
    /// pilot shape that used to read empty and false-fire the rule.
    #[test]
    fn tests_missing_quiet_on_lowered_inline_tests() {
        let block = lower_test_block(
            &[
                "allows when input.email contains \"@\"".to_owned(),
                "denies when input.email is_empty".to_owned(),
            ],
            span(),
        )
        .expect("a block of when-predicate lines must lower to a non-empty TestBlock");
        assert_eq!(block.assertions.len(), 2);
        assert!(
            block
                .assertions
                .iter()
                .all(|a| matches!(a, ir::TestAssertion::Raw { .. }))
        );
    }

    /// Mixed block: a typed `as` row plus a `when` row — both survive.
    #[test]
    fn mixed_typed_and_when_block_lowers_all_lines() {
        let block = lower_test_block(
            &[
                "allows as @role.member".to_owned(),
                "denies when input.note is_empty".to_owned(),
            ],
            span(),
        )
        .expect("mixed block lowers");
        assert_eq!(
            block.assertions[0],
            ir::TestAssertion::AllowsAs {
                actor: "@role.member".to_owned()
            }
        );
        assert_eq!(
            block.assertions[1],
            ir::TestAssertion::Raw {
                line: "denies when input.note is_empty".to_owned()
            }
        );
    }
}
