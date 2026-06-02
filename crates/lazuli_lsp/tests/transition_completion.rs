//! Cell LSP-TRANSITION-COMPLETION (audit gap #1) — integration coverage
//! for completing declared `transition <name>` at a `triggers transition
//! <name>` command slot.
//!
//! Exercises the public `triggers_transition_completions` entry point the
//! `completion` dispatch trunk calls, against canonical `feature` sources.

use lazuli_lsp::triggers_transition_completions;
use tower_lsp::lsp_types::Position;

fn position_for_offset(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    Position { line, character }
}

fn labels_after(source: &str, needle: &str) -> Vec<String> {
    let offset = source
        .rfind(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    triggers_transition_completions(source, position_for_offset(source, offset))
        .unwrap_or_else(|| panic!("expected transition completions after `{needle}`"))
        .into_iter()
        .map(|item| item.label)
        .collect()
}

const SOURCE: &str = "\
feature billing
  domain
    resource Invoice
      lifecycle status
        state draft initial
        state issued
        transition issue
          from draft
          to issued
        transition void
          from issued
          to voided
  command issue_invoice
    triggers transition \n";

#[test]
fn inline_triggers_slot_offers_declared_transitions() {
    let labels = labels_after(SOURCE, "triggers transition ");
    assert!(labels.contains(&"issue".to_owned()), "labels = {labels:?}");
    assert!(labels.contains(&"void".to_owned()), "labels = {labels:?}");
    assert_eq!(labels.len(), 2, "only the two declared transitions");
}

#[test]
fn no_completion_off_the_slot() {
    // Cursor on the feature header line — must not fire.
    let pos = position_for_offset(SOURCE, "feature bil".len());
    assert!(triggers_transition_completions(SOURCE, pos).is_none());
}

#[test]
fn block_form_triggers_child_offers_transitions() {
    let source = "\
feature billing
  domain
    resource Invoice
      lifecycle status
        transition issue
        transition void
  command issue_invoice
    triggers
      transition \n";
    let labels = labels_after(source, "      transition ");
    assert!(labels.contains(&"issue".to_owned()), "labels = {labels:?}");
    assert!(labels.contains(&"void".to_owned()), "labels = {labels:?}");
}
