//! Statement-keyword catalogs and typo diagnostics for nested
//! sub-block bodies that aren't full block headers themselves:
//! `command X` indent-4 statements, `query.*` indent-4 statements,
//! `audience <name>` indent-2 children.
//!
//! Same skeleton as `blocks.rs`, but scoped to indent-4 inside a
//! statement-bearing parent block.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::super::closest_kind;
use crate::{leading_spaces, simple_canonical_diagnostic};

/// Closed catalog of indent-4 statement keywords inside a `command X`
/// body. Mirrors `parse_command_decl`'s prefix dispatch table. The
/// effect targets (`creates Foo`, `updates Foo`, etc.) are statement
/// keywords; the resource name they target is filtered out by the
/// per-line capitalized-identifier check (a bare `Customer` line is a
/// stray token, not an unknown kind).
/// Sorted alphabetically.
pub(crate) const COMMAND_STATEMENT_KINDS: &[&str] = &[
    "approval",
    "audit",
    "calls",
    "creates",
    "deletes",
    "deprecated",
    "emits",
    "gate",
    "handler",
    "idempotency",
    "input",
    "invalidates",
    "let",
    "policy",
    "previously",
    "rate_limit",
    "retry",
    "returns",
    "route",
    "target",
    "tests",
    "timeout",
    "updates",
    "validate",
    "write_window",
];

/// Closed catalog of indent-4 statement keywords inside `query.list`,
/// `query.lookup`, `query.sql`, and `query.view` bodies. Mirrors the prefix-dispatch
/// arms in `parse_query_list_decl`, `parse_query_lookup_decl`, and
/// `parse_query_sql_decl`. Includes the union (a `query.lookup` body
/// will never use `cache`/`paginate`/`order` — but flagging those as
/// typos would cause false positives across query kinds, so we accept
/// them and let the parser emit the precise per-kind error).
/// Sorted alphabetically.
pub(crate) const QUERY_STATEMENT_KINDS: &[&str] = &[
    "cache", "filters", "gate", "modifier", "order", "paginate", "params", "policy", "returns",
    "scope", "search", "source", "sql",
];

/// Closed catalog of children inside `audience <name>` blocks.
/// Per `parse_lzx_audience_block`, ONLY `requires @scope.<name>` and
/// `view list|detail|create <name>` are valid. The `requires` lines
/// are filtered out by the leading-`@` skip; we catalog the bare kind
/// keywords here.
pub(crate) const AUDIENCE_BODY_KINDS: &[&str] = &["policy", "requires", "view"];

/// 2026-05-15 — Indent-4 statement keywords inside `command X` body.
/// A typo like `audt` silently drops audit metadata; `inalidates`
/// strips cache invalidation. Both regenerate clean Go that quietly
/// loses behavior — exactly the silent-failure mode this lint catches.
///
/// Skips lines that are NOT keyword-statements: assignments (`x = ...`),
/// effect targets (capitalized identifier like `Customer`), field-name
/// lines inside `input`/`output` sub-blocks (which carry `:`).
pub(crate) fn command_statement_unknown_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_command: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_command {
            if leading <= header_indent {
                current_command = None;
            }
        }

        if current_command.is_none() {
            if trimmed.starts_with("command ") {
                current_command = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_command.unwrap();
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        // Skip decorators, namespaced refs, key-value lines, assignments.
        // The `=` and `:` checks scan the WHOLE trimmed line because
        // command bodies host `<field> = <expr>` and `<field>: <Type>`
        // forms where the LHS identifier carries no punctuation.
        if first.starts_with('@')
            || first.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        // Skip effect-target "bare resource" lines like `Customer` (a
        // capitalized identifier on its own is not a statement keyword).
        if first
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            continue;
        }
        if COMMAND_STATEMENT_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, COMMAND_STATEMENT_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown command statement `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown command statement `{first}`. Valid statements: previously / route / input / policy / rate_limit / audit / approval / target / let / validate / creates / updates / deletes / returns / handler / emits / invalidates / calls / timeout / retry / idempotency / write_window / tests / deprecated / gate."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "command-statement-unknown",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Indent-4 statement keywords inside `query.list`,
/// `query.lookup`, `query.sql`, and `query.view` bodies. A typo like `paginat` silently
/// drops pagination; `cahce` drops the cache profile binding.
pub(crate) fn query_statement_unknown_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_query {
            if leading <= header_indent {
                current_query = None;
            }
        }

        if current_query.is_none() {
            if trimmed.starts_with("query.list ")
                || trimmed.starts_with("query.lookup ")
                || trimmed.starts_with("query.sql ")
                || trimmed.starts_with("query.view ")
            {
                current_query = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_query.unwrap();
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if QUERY_STATEMENT_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, QUERY_STATEMENT_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown query statement `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown query statement `{first}`. Valid statements: policy / params / filters / scope / modifier / search / cache / paginate / order / returns / sql / source / gate."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "query-statement-unknown",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Children of `audience <name>` blocks. Per the parser's
/// strict check (`parse_lzx_audience` line 962), ONLY `view <name>
/// <Component>` lines are accepted. A typo like `vieww list ItemList`
/// causes the parser to bail with a generic shape error; this lint
/// turns it into a precise typo suggestion.
pub(crate) fn audience_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_audience: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_audience {
            if leading <= header_indent {
                current_audience = None;
            }
        }

        if current_audience.is_none() {
            if trimmed.starts_with("audience ") {
                current_audience = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_audience.unwrap();
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if AUDIENCE_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, AUDIENCE_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown audience child `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown audience child `{first}`. Valid children: `view <name> <Component>` declarations (or `requires @scope.<name>`)."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "audience-unknown-kind",
            &message,
        ));
    }

    diagnostics
}
