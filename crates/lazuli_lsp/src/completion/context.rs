//! Context-aware completion dispatch.
//!
//! [`context_aware_completions`] is the trunk that fans out to every
//! sibling completion provider in this crate. It chains, in priority
//! order:
//!
//! 1. `conventions [...]` closed-catalog completion
//!    (`crate::conventions_list_completions`).
//! 2. IR Route-Guards / Lifecycle-Gate completions
//!    (`crate::route_guard_completions` /
//!    `crate::lifecycle_gate_completions`).
//! 3. `@<ns>.<name>` prefix completion
//!    (`crate::namespace_prefix_completions`).
//! 4. `rate_limit "...per <axis>"` axis catalog
//!    ([`rate_limit_axis_completions`]).
//! 5. `rate_limit "..." in <env>` env catalog
//!    (`crate::rate_limit_env_completions`).
//! 6. `error_page <status>` / `audience <name>` completion
//!    (`crate::error_page_value_completions`).
//! 7. Auth-refresh rotation completions
//!    (`crate::auth_refresh_completions`).
//! 8. Error-vocab 6-trigger-position completions
//!    (`crate::error_vocab_completions`).
//! 9. Indent-aware kind-child fallback driven by
//!    [`KIND_CHILD_COMPLETIONS`] + [`EFFECT_VERBS`] +
//!    [`block_kind_at`].
//!
//! Three closed catalogs live alongside: [`KIND_CHILD_COMPLETIONS`]
//! (the per-block-kind closed children offered as completion),
//! [`EFFECT_VERBS`] (the four canonical command effect verbs), and
//! [`RATE_LIMIT_AXES`] (the four closed rate-limit axes).
//!
//! Two `conventions [...]` helpers live here too because they sit on
//! the same conventions-list cursor surface as the dispatch trunk:
//! [`convention_bundle_hover`] (rich hover for `crud` / `me`) and
//! [`is_inside_conventions_list`] (cursor-position gate).
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::leading_spaces`] / [`crate::line_prefix_at_position`] /
//!   [`crate::byte_index_for_utf16_position`] — indent/cursor probes.
//! - [`crate::keyword_description`] — keyword detail strings.
//! - The 9 sibling completion providers chained above.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::{
    auth_refresh_completions, byte_index_for_utf16_position, conventions_list_completions,
    error_page_value_completions, error_vocab_completions, keyword_description, leading_spaces,
    lifecycle_gate_completions, line_prefix_at_position, merge_completion_items,
    namespace_prefix_completions, rate_limit_env_completions, route_guard_completions,
};

/// Children offered as completion when the cursor sits on an indented
/// blank line inside a known kind block. The first slice element is
/// the kind keyword (matched by `block_kind_at`), the second is the
/// closed-catalog children offered as `CompletionItem`s. Required and
/// optional children are merged so authors / LLMs see the full
/// vocabulary; the per-token hover (via `keyword_description`) tells
/// them which are required.
///
/// The kind matcher is forgiving: any line whose first non-space token
/// starts with one of these prefixes (e.g. `command capture_lead`)
/// counts as the matching block.
pub(crate) const KIND_CHILD_COMPLETIONS: &[(&str, &[&str])] = &[
    (
        "command",
        &[
            "input",
            "route",
            "policy",
            "rate_limit",
            "audit",
            "validate",
            "let",
            "creates",
            "updates",
            "deletes",
            "returns",
            "handler",
            "emits",
            "invalidates",
            "approval",
            "tests",
            "previously",
            "deprecated",
            "gate",
            "idempotency",
            "write_window",
        ],
    ),
    (
        "query.list",
        &[
            "params",
            "filters",
            "search",
            "order",
            "paginate",
            "cache",
            "policy",
            "modifier",
            "scope",
            "rate_limit",
            "audit",
        ],
    ),
    (
        "query.lookup",
        &["params", "key", "policy", "cache", "scope", "audit"],
    ),
    (
        "query.sql",
        &[
            "returns", "sql", "params", "scope", "policy", "cache", "audit",
        ],
    ),
    (
        "query.view",
        &["policy", "returns", "source", "params", "scope"],
    ),
    // CL.C.3 — feature-level `cache <name>` profile children. The
    // inline (per-query) cache shape reuses these same keywords; the
    // context-aware completion uses the block kind, not the shape.
    (
        "cache",
        &[
            "key",
            "ttl",
            "namespace",
            "tags",
            "stale_while_revalidate",
            "coalesce",
            "sliding",
        ],
    ),
    (
        "api",
        &[
            "method",
            "path",
            "input",
            "output",
            "policy",
            "handler",
            "rate_limit",
            "audit",
            "route",
            "deprecated",
        ],
    ),
    ("deprecated", &["since", "replacement", "sunset"]),
    (
        "agent",
        &[
            "input",
            "context",
            "policy",
            "rate_limit",
            "output",
            "model",
            "prompt",
            "tools",
            "safety",
            "evals",
            "temperature",
            "top_p",
            "max_tokens",
            "seed",
            "expose",
            "audit",
        ],
    ),
    ("error_page", &["template", "audience"]),
    (
        "tenant_migration",
        &[
            "target",
            "axis",
            "idempotency",
            "timeout",
            "retry",
            "handler",
        ],
    ),
];

/// Closed catalog of effect verbs offered as completion when the
/// cursor is positioned where an effect would go inside a `command`.
/// `returns` is the non-mutating sibling shipped on commands like the
/// `smoke-hello` fixture's `greet` command.
pub(crate) const EFFECT_VERBS: &[&str] = &["creates", "updates", "deletes", "returns"];

/// Closed catalog of `rate_limit` axis tokens for the
/// `"<N> per <window> per <axis>"` grammar. Surfaced inside double
/// quotes after `per` so authors / LLMs see the closed set instead of
/// guessing tenant-style words.
pub(crate) const RATE_LIMIT_AXES: &[&str] = &["ip", "user", "org", "tenant"];

/// Detect which kind block the cursor is in by walking the source
/// backwards from `position.line` looking for the closest header line
/// at the parent indent. Returns the kind keyword (e.g. `command`,
/// `query.list`, `api`) or `None` when the cursor is at the top
/// level / outside a recognised block.
///
/// We treat header-line indent as "parent" if it's strictly less than
/// the cursor line's indent. The first kind keyword we find at that
/// shallower indent wins. This handles the canonical Lazurite layout
/// where `command capture_lead` sits at indent 2 inside a feature and
/// its children at indent 4.
pub(crate) fn block_kind_at(source: &str, position: Position) -> Option<&'static str> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = position.line as usize;
    if cursor_line_idx >= lines.len() {
        // The completion handler can be called past EOL; treat as the
        // last real line.
    }
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);

    // Walk backwards looking for a header line at indent < cursor_indent.
    for idx in (0..cursor_line_idx).rev() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < cursor_indent {
            // Try each known kind prefix. Match the longest first to
            // distinguish `query.list` from a hypothetical `query`.
            let mut kinds: Vec<&str> = KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
            kinds.sort_by_key(|k| std::cmp::Reverse(k.len()));
            for kind in kinds {
                if trimmed == kind
                    || trimmed.starts_with(&format!("{kind} "))
                    || trimmed.starts_with(&format!("{kind}\t"))
                {
                    return Some(kind);
                }
            }
            // Found a shallower line but it isn't one of our kinds —
            // stop walking; we're inside a sibling/parent block we
            // don't have a rich completion for yet.
            return None;
        }
    }
    None
}

/// Context-aware completions covering the seven LSP-extended kinds.
/// Returns `None` to fall back to the flat keyword list; returns
/// `Some(items)` when a context is recognised. Three contexts handled:
///
/// 1. `@policy.` / `@validator.` / `@fn.` / `@hook.` / `@role.` /
///    `@scope.` / `@actor.` immediately before the cursor — offers
///    every declared name from the same source.
/// 2. Indented blank line inside a known kind block — offers the
///    closed-catalog children (`KIND_CHILD_COMPLETIONS`). When the
///    block is a `command` the effect verbs are tagged as
///    `ENUM_MEMBER` so editors render them distinctly.
/// 3. Inside a `rate_limit "..."` value after the second `per ` —
///    offers the closed axis catalog (`ip`/`user`/`org`/`tenant`).
pub(crate) fn context_aware_completions(
    source: &str,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);

    // `docs/proposals/ir-resource-conventions-crud.md` §4.4 — closed-
    // catalog completion inside `conventions [..]`. Runs first because
    // the trigger is narrow (cursor inside a single-line bracket list
    // after the `conventions` keyword) and the surface is single-
    // member today; ranking before other context checks keeps the
    // typo'd intermediate `co` keystroke from offering noisy matches.
    if let Some(items) = conventions_list_completions(before) {
        return Some(items);
    }

    // IR Route-Guards — narrow context completions for view policies,
    // redirect paths, `actor_query`, and `app.route_guard` defaults.
    // Runs before generic namespace completion so `policy @policy.` inside
    // a view can offer full `@policy.<name>` refs instead of bare names.
    let lifecycle_items = lifecycle_gate_completions(source, position);
    let route_guard_items = route_guard_completions(source, position);
    if lifecycle_items.is_some() || route_guard_items.is_some() {
        return Some(merge_completion_items(lifecycle_items, route_guard_items));
    }

    // 1. `@<ns>.` prefix completion.
    if let Some(items) = namespace_prefix_completions(source, before) {
        return Some(items);
    }

    // 3. `rate_limit "...per <axis>"` — checked before the indent
    // path so we don't drop into kind-child completion while the
    // cursor is inside a string literal on a `rate_limit` line.
    if let Some(items) = rate_limit_axis_completions(before) {
        return Some(items);
    }

    // IR Rate-Limit env-aware (`docs/proposals/ir-rate-limit-env-aware.md`
    // §11.3) — when the cursor sits inside the `in <env>` tail of a
    // `rate_limit "..." in <|>` line, surface the 5-entry closed env
    // catalog. Runs after `rate_limit_axis_completions` so the axis
    // inside-string slot still wins when the cursor is in the spec.
    if let Some(items) = rate_limit_env_completions(before) {
        return Some(items);
    }

    if let Some(items) = error_page_value_completions(source, position, before) {
        return Some(items);
    }

    // IR Auth Refresh Rotation — narrow completions for
    // `auth.sessions.access_ttl` and nested `rotation` children. This runs
    // before error-vocab and kind-child fallback because the triggers are
    // specific to the auth.sessions indent shape.
    if let Some(items) = auth_refresh_completions(source, position) {
        return Some(items);
    }

    // IR Error-Vocab — the 6 trigger positions from proposal §7.1. Runs
    // before the indent-aware kind-child fallback because the matches are
    // strictly narrower (`when_denied `, `<code> message `,
    // `expose client 4xx ...`, `default ` inside `errors`, blank line inside
    // `errors`).
    if let Some(items) = error_vocab_completions(source, position) {
        return Some(items);
    }

    // 2. Indent-aware kind child.
    // Only fire when the prefix is whitespace (cursor on a blank
    // indented line) or a partial child keyword. We don't want to
    // shadow `@cap.File(...)` value completion (handled earlier in
    // the dispatch chain) or general keyword completion mid-token in
    // an unrelated context.
    let trimmed_before = before.trim_start();
    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word = !trimmed_before.is_empty()
        && trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !(is_blank_indented || is_partial_word) {
        return None;
    }
    let kind = block_kind_at(source, position)?;
    let (_, children) = KIND_CHILD_COMPLETIONS.iter().find(|(k, _)| *k == kind)?;
    let mut items: Vec<CompletionItem> = children
        .iter()
        .map(|child| CompletionItem {
            label: (*child).to_owned(),
            kind: Some(if EFFECT_VERBS.contains(child) {
                CompletionItemKind::ENUM_MEMBER
            } else {
                CompletionItemKind::KEYWORD
            }),
            detail: keyword_description(child).map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect();
    // Sort effects to the top of `command` completions so authors see
    // the canonical write verbs at-a-glance.
    if kind == "command" {
        items.sort_by_key(|item| {
            (
                !EFFECT_VERBS.contains(&item.label.as_str()),
                item.label.clone(),
            )
        });
    }
    Some(items)
}

pub(crate) fn convention_bundle_hover(
    source: &str,
    position: Position,
    word: &str,
) -> Option<String> {
    let bundle = match word {
        "crud" | "me" => word,
        _ => return None,
    };
    if !is_inside_conventions_list(source, position) {
        return None;
    }

    let lines: &[&str] = match bundle {
        "crud" => &[
            "`conventions [crud]` synthesizes:",
            "- `query.list list_<resource_snake>s` - paginated list of rows",
            "- `query.lookup lookup_<resource_snake>` - lookup by route id",
            "- `command create_<resource_snake>`",
            "- `command update_<resource_snake>`",
            "- `command delete_<resource_snake>`",
            "",
            "When the author declares a query/command with one of these names, the synth silently skips that entry (author wins).",
        ],
        "me" => &[
            "`conventions [me]` synthesizes:",
            "- `query.lookup lookup_my_<resource_snake>` - lookup for the active actor",
            "",
            "When the author declares a query with this name, the synth silently skips that entry (author wins).",
        ],
        _ => return None,
    };
    Some(lines.join("\n"))
}

pub(crate) fn is_inside_conventions_list(source: &str, position: Position) -> bool {
    let Some(line) = source.lines().nth(position.line as usize) else {
        return false;
    };
    let trimmed = line.trim_start();
    if !trimmed.starts_with("conventions ") {
        return false;
    }

    let cursor = byte_index_for_utf16_position(line, position.character);
    let before = &line[..cursor.min(line.len())];
    let Some(conv_idx) = before.rfind("conventions") else {
        return false;
    };
    let after_kw_before_cursor = &before[conv_idx + "conventions".len()..];
    after_kw_before_cursor.contains('[') && !after_kw_before_cursor.contains(']')
}

/// Inside a `rate_limit "<N> per <window> per "` value, offer the
/// closed axis catalog. Returns `None` outside that context.
pub(crate) fn rate_limit_axis_completions(before_cursor: &str) -> Option<Vec<CompletionItem>> {
    // We must be inside an open double-quoted string on a line whose
    // first non-space token is `rate_limit`.
    let quote_open = before_cursor.rfind('"')?;
    let already_closed = before_cursor[quote_open + 1..].contains('"');
    if already_closed {
        return None;
    }
    let line_prefix = &before_cursor[..quote_open];
    let trimmed_prefix = line_prefix.trim_start();
    if !trimmed_prefix.starts_with("rate_limit") {
        return None;
    }
    let string_so_far = &before_cursor[quote_open + 1..];
    // We expect `<N> per <window> per ` before the cursor; require at
    // least two `per ` occurrences.
    let per_count = string_so_far.matches(" per ").count();
    let ends_with_per_space = string_so_far.ends_with(" per ")
        || string_so_far
            .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
            .ends_with(" per ");
    if per_count < 2 || !ends_with_per_space {
        return None;
    }
    Some(
        RATE_LIMIT_AXES
            .iter()
            .map(|axis| CompletionItem {
                label: (*axis).to_owned(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(match *axis {
                    "ip" => "Per source IP (rate-limit axis).".to_owned(),
                    "user" => "Per authenticated user (rate-limit axis).".to_owned(),
                    "org" => "Per tenant org (rate-limit axis).".to_owned(),
                    "tenant" => "Per generic tenant axis (rate-limit axis).".to_owned(),
                    _ => String::new(),
                }),
                ..CompletionItem::default()
            })
            .collect(),
    )
}
