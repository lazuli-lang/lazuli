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
    "reorder",
    "retry",
    "returns",
    "route",
    "target",
    "tests",
    "timeout",
    "triggers",
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

/// Closed catalog of children inside an `auth ... sessions` block.
/// Mirrors `parse_auth_sessions` in
/// `lazuli_syntax/src/parser/lzi/auth/sessions.rs`: the `resource` /
/// `ttl` / `refresh` / `access_ttl` scalars plus the two nested
/// sub-blocks `rotation` and `cookie` (the latter being the
/// `cookie-sessions-child`). The cookie sub-block's own attributes
/// (`name` / `same_site` / `secure` / `http_only` / `domain` / `path`)
/// live one indent deeper, so they are NOT cataloged here — they are
/// caught by [`SESSIONS_COOKIE_BODY_KINDS`].
/// Sorted alphabetically for diff hygiene.
pub(crate) const SESSIONS_BODY_KINDS: &[&str] = &[
    "access_ttl",
    "cookie",
    "refresh",
    "resource",
    "rotation",
    "ttl",
];

/// Closed catalog of children inside an `auth ... sessions ... cookie`
/// block — the six session-cookie transport attributes from
/// `parse_auth_sessions_cookie`
/// (`lazuli_syntax/src/parser/lzi/auth/sessions.rs:318`). The
/// vocabulary is shared with the app-level `app.cookie` profiles
/// (`Context::Cookie`), reused here under the `auth.sessions` parent
/// per `docs/proposals/cookie-sessions-child.md`.
/// Sorted alphabetically for diff hygiene.
pub(crate) const SESSIONS_COOKIE_BODY_KINDS: &[&str] =
    &["domain", "http_only", "name", "path", "same_site", "secure"];

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

        // `current_command.is_none()` continued the loop above, so this is Some.
        let Some((_header_indent, body_indent)) = current_command else {
            continue;
        };
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
                "unknown command statement `{first}`. Valid statements: previously / route / input / policy / rate_limit / audit / approval / target / let / validate / creates / updates / deletes / reorder / returns / handler / emits / triggers / invalidates / calls / timeout / retry / idempotency / write_window / tests / deprecated / gate."
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

        // `current_query.is_none()` continued the loop above, so this is Some.
        let Some((_header_indent, body_indent)) = current_query else {
            continue;
        };
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

        // `current_audience.is_none()` continued the loop above, so this is Some.
        let Some((_header_indent, body_indent)) = current_audience else {
            continue;
        };
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

/// `cookie-sessions-child` — children of an `auth ... sessions` block and
/// its nested `cookie` sub-block. A typo like `cokie` (vs `cookie`) or
/// `same_sight` (vs `same_site`) is silently dropped by
/// `parse_auth_sessions` / `parse_auth_sessions_cookie`, so the session
/// regenerates without the intended transport override and the squiggle
/// never fires. This producer turns either typo into a precise
/// suggestion.
///
/// The `sessions` block nests at a feature-relative indent (typically
/// `feature -> auth -> sessions`), so it is header-tracked rather than
/// pinned to a fixed column. Two depths are checked: the sessions body
/// (`resource` / `ttl` / `refresh` / `access_ttl` / `rotation` /
/// `cookie`) and — when a `cookie` sub-block opens — its attributes one
/// indent deeper. The `rotation` sub-block's own children live at the
/// same deeper indent but a different parent, so they are skipped while
/// the open block is `cookie` only.
///
/// Lines that are NOT bare kind keywords (decorators, typed `:` decls,
/// `=` assignments, `(`-calls) are skipped, mirroring the sibling
/// producers.
pub(crate) fn sessions_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // (header_indent, body_indent) of the currently open `sessions` block.
    let mut current_sessions: Option<(usize, usize)> = None;
    // body_indent of the currently open `cookie` sub-block (its children
    // sit at this indent). `None` when no cookie sub-block is open.
    let mut cookie_body_indent: Option<usize> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        // Close the sessions scope when indentation returns to (or above)
        // the header. Closing sessions also closes any open cookie block.
        if let Some((header_indent, _body_indent)) = current_sessions {
            if leading <= header_indent {
                current_sessions = None;
                cookie_body_indent = None;
            }
        }

        if current_sessions.is_none() {
            if trimmed == "sessions" || trimmed.starts_with("sessions ") {
                current_sessions = Some((leading, leading + 2));
            }
            continue;
        }

        let Some((_header_indent, body_indent)) = current_sessions else {
            continue;
        };

        // A line at (or above) the sessions body indent closes any open
        // cookie sub-block before we re-classify it.
        if let Some(c_indent) = cookie_body_indent {
            if leading < c_indent {
                cookie_body_indent = None;
            }
        }

        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        // Skip decorators, typed field decls, assignments, namespaced calls.
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }

        // Inside an open `cookie` sub-block: check its attributes.
        if let Some(c_indent) = cookie_body_indent {
            if leading == c_indent {
                if SESSIONS_COOKIE_BODY_KINDS.contains(&first) {
                    continue;
                }
                let suggestion = closest_kind(first, SESSIONS_COOKIE_BODY_KINDS, 2);
                let message = match suggestion {
                    Some(suggested) => format!(
                        "unknown `auth sessions cookie` attribute `{first}`. Did you mean `{suggested}`?"
                    ),
                    None => format!(
                        "unknown `auth sessions cookie` attribute `{first}`. Valid attributes: name / same_site / secure / http_only / domain / path."
                    ),
                };
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "sessions-cookie-unknown-kind",
                    &message,
                ));
            }
            // Deeper-than-cookie-body lines are ignored.
            continue;
        }

        // At the sessions body indent: check the sessions child catalog.
        if leading != body_indent {
            continue;
        }
        // Opening the `cookie` sub-block: record its child indent and accept it.
        if first == "cookie" {
            cookie_body_indent = Some(body_indent + 2);
            continue;
        }
        if SESSIONS_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, SESSIONS_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown `auth sessions` child `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown `auth sessions` child `{first}`. Valid children: resource / ttl / refresh / access_ttl / rotation / cookie."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "sessions-unknown-kind",
            &message,
        ));
    }

    diagnostics
}
