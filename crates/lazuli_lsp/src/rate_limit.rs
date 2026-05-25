//! `rate_limit "..." in <env>` env-list completion provider.
//!
//! Surfaces the closed env catalog (`production`, `staging`, `test`,
//! `dev`, `local`) when the cursor sits inside the `in <env>, ...`
//! tail of a `rate_limit "<spec>" in <|>` literal. Filters out envs
//! already listed in the same `in`-list so authors don't see
//! redundant offers.
//!
//! ## ABI guarantee
//!
//! `rate_limit_env_completions` is re-exported from the crate root
//! via `pub use rate_limit::*;` so external consumers keep importing
//! it from the same path (`lazuli_lsp::rate_limit_env_completions`).
//!
//! ## Cross-module references
//!
//! Three private helpers (`closing_quote_after_rate_limit`,
//! `split_trailing_token`, `env_completion_detail`) live here too —
//! they're consumed only by `rate_limit_env_completions`. The closed
//! env catalog itself (`RATE_LIMIT_ENV_CATALOG`) lives in
//! `catalogs.rs` and is referenced via `crate::RATE_LIMIT_ENV_CATALOG`.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

/// Inside a `rate_limit "<spec>" in <|>` (or `rate_limit "<spec>" in
/// <env>, <|>`) context, offer the closed env catalog. Returns `None`
/// outside that context.
///
/// Trigger conditions:
/// - The line's first non-space token is `rate_limit`.
/// - A complete double-quoted string literal sits between
///   `rate_limit` and the `in` keyword (so we're past the spec).
/// - The substring after the `in` keyword and before the cursor is
///   either empty, ends with `, `, or contains a partial identifier
///   word (so the author is mid-name).
///
/// Already-listed envs are filtered out so authors don't see
/// duplicate offers (`rate_limit "..." in dev, |` skips `dev`).
pub fn rate_limit_env_completions(before_cursor: &str) -> Option<Vec<CompletionItem>> {
    let trimmed = before_cursor.trim_start();
    if !trimmed.starts_with("rate_limit") {
        return None;
    }
    // Locate the closing quote of the spec string and the `in` keyword
    // that follows. We need both — without the closed string we're
    // still inside the spec (handled by `rate_limit_axis_completions`),
    // and without the `in` keyword the author hasn't reached the
    // env-qualifier slot yet.
    let close_quote = closing_quote_after_rate_limit(trimmed)?;
    let after_quote = &trimmed[close_quote + 1..];
    // Find the `in` keyword: token-aligned (preceded by whitespace,
    // followed by whitespace). Strip leading whitespace, then check.
    let after_quote_trimmed = after_quote.trim_start();
    let in_rel_start = after_quote.len() - after_quote_trimmed.len();
    if !after_quote_trimmed.starts_with("in ") && after_quote_trimmed != "in" {
        return None;
    }
    let after_in_start = in_rel_start + 2; // skip the literal "in"
    if after_in_start > after_quote.len() {
        return None;
    }
    let after_in = &after_quote[after_in_start..];
    // We accept exactly one separating space after `in`. The remainder
    // is the env-list authored so far.
    let env_list = after_in.trim_start_matches(' ');
    // Already-listed envs (comma-separated names; partial trailing
    // token is the in-flight prefix). We filter the catalog so the
    // author doesn't see redundant offers.
    let (committed, _partial) = split_trailing_token(env_list);
    let already: Vec<&str> = committed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let items: Vec<CompletionItem> = crate::RATE_LIMIT_ENV_CATALOG
        .iter()
        .filter(|env| !already.contains(env))
        .map(|env| CompletionItem {
            label: (*env).to_owned(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some(env_completion_detail(env).to_owned()),
            ..CompletionItem::default()
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(items)
}

/// Find the closing `"` of the spec string on a `rate_limit` line.
/// Returns the byte offset of the closing quote in `trimmed` (a string
/// that starts with `rate_limit`). Returns `None` if either quote is
/// missing.
fn closing_quote_after_rate_limit(trimmed: &str) -> Option<usize> {
    let first_quote = trimmed.find('"')?;
    let after_first = &trimmed[first_quote + 1..];
    let close_rel = after_first.find('"')?;
    Some(first_quote + 1 + close_rel)
}

/// Split a comma-separated env list at the trailing in-flight token.
/// Returns `(committed_part, partial_trailing_token)`.
/// `"dev, staging, t"` -> `("dev, staging,", "t")`.
/// `"dev, "` -> `("dev,", "")`.
/// `""` -> `("", "")`.
fn split_trailing_token(env_list: &str) -> (&str, &str) {
    match env_list.rfind(',') {
        Some(last_comma) => {
            let after = env_list[last_comma + 1..].trim_start();
            // Slice the input so committed includes the trailing comma.
            // We can't just split because `after` may have leading whitespace
            // that's not in the original; use char-offset arithmetic on the
            // original `env_list` to recover the trailing token.
            let trailing_start = env_list.len() - after.len();
            (&env_list[..last_comma + 1], &env_list[trailing_start..])
        }
        None => ("", env_list),
    }
}

fn env_completion_detail(env: &str) -> &'static str {
    match env {
        "production" => "Production deployment. `LAZULI_ENV=production`.",
        "staging" => "Pre-production mirror. `LAZULI_ENV=staging`.",
        "test" => "Automated test suite (CI + `pnpm test`). `LAZULI_ENV=test`.",
        "dev" => "Developer-machine `pnpm dev`. `LAZULI_ENV=dev`.",
        "local" => "Equivalent alias for `dev`. `LAZULI_ENV=local`.",
        _ => "",
    }
}
