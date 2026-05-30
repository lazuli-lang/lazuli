//! Cell LSP-1 — completion, hover, and code-action coverage for the IR
//! Error-Vocab surface. See `docs/proposals/ir-error-messages-vocab.md`
//! §7 for the spec and §11 Cell LSP-1 for the implementation scope.
//!
//! These tests drive the public LSP-side surface directly (the same way
//! `auth.rs` exercises the auth bucket) so they pin the contract:
//! - keyword descriptions + rich hovers for `when_denied`, `errors`,
//!   `message_key`, and the 8 closed-catalog error codes.
//! - resolved-text hover for a code, walking the same-feature
//!   `translation` chain.
//! - completion firing at the 6 trigger positions (proposal §7.1).
//! - the 3 code actions (proposal §7.4).
//!
//! Wave R10-C split this single-file crate into per-concern sub-modules to
//! keep every file ≤ 500 LOC.

use tower_lsp::lsp_types::Position;

mod catalog;
mod code_actions;
mod completion;
mod helpers_coverage;
mod hover;

pub(crate) fn sample_feature_with_errors_override() -> String {
    // Mirrors the proposal §2.E "Resolution example" feature, with one
    // per-code feature-level override (`policy_denied`) and one declared
    // translation key.
    [
        "feature account",
        "  policies",
        "    authenticated: @scope.authenticated",
        "      when_denied @translation.must_be_signed_in",
        "",
        "  errors",
        "    default hide",
        "    expose client 4xx message, code",
        "    expose client 5xx code",
        "    policy_denied      message @translation.account_signin_required",
        "",
        "  translation",
        "    catalog \"./i18n/account.<locale>.json\"",
        "    key account_signin_required",
        "      en-US \"Please sign in to choose your role.\"",
        "      pt-BR \"Para escolher seu papel, entre na sua conta primeiro.\"",
        "    key must_be_signed_in",
        "      en-US \"You need to sign in first.\"",
        "",
    ]
    .join("\n")
}

pub(crate) fn position_at(source: &str, needle: &str) -> Position {
    let line_idx = line_index_containing(source, needle);
    let line = source.lines().nth(line_idx).expect("line should exist");
    let column = line.find(needle).expect("needle should appear on the line");
    Position {
        line: line_idx as u32,
        character: column as u32,
    }
}

/// Return a cursor position one column past `needle` on the line where
/// `needle` first appears. Used to simulate the user having typed the
/// substring and pressed completion at the very end of it.
pub(crate) fn cursor_after(source: &str, needle: &str) -> Position {
    let line_idx = line_index_containing(source, needle);
    let line = source.lines().nth(line_idx).expect("line should exist");
    let column = line.find(needle).expect("needle should appear on the line");
    Position {
        line: line_idx as u32,
        character: (column + needle.len()) as u32,
    }
}

pub(crate) fn line_index_containing(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("source does not contain needle `{needle}`:\n{source}"))
}
