//! Indent-based source scanners over `.lzi` documents.
//!
//! Best-effort lexical helpers that walk the source text by leading-
//! space indentation rather than via the parser. They power the LSP
//! features that need to answer "what feature does this position sit
//! in?" or "what queries / policy categories / translation keys does
//! this document declare?" before the IR is lowered.
//!
//! ## Why not the parser?
//!
//! The LSP runs these inside completion / hover / code-action paths
//! that fire on every keystroke. A full parse is wasteful and the
//! parser doesn't yet expose stable scope-walk APIs. The scanners are
//! lossy by design (they reject incomplete syntax silently) but they
//! always terminate, so they're safe in tight UI loops.
//!
//! ## ABI guarantee
//!
//! All four functions are re-exported from the crate root via `pub use
//! source_scan::*;` so external consumers (the canonical pilot VSCode extension,
//! `lazuli_cli::doctor`) keep importing them from the same path
//! (`lazuli_lsp::collect_query_refs`, etc.).
//!
//! ## Shared helper
//!
//! All four scanners call `crate::leading_spaces` (lifted to
//! `pub(crate)` in lib.rs for this extraction) to count the indent.
//! That helper stays in lib.rs because many other private scanners
//! across the LSP also use it.

use std::collections::HashSet;

use tower_lsp::lsp_types::Position;

/// Walk the source and return every `<feature>.query.<name>` reference
/// it declares, in document order. De-duplicated. Used by completion
/// providers that surface valid query refs (e.g. route-guard
/// `actor_query` completion).
///
/// Lossy by design: only feature-scoped indent-2 query declarations
/// register. Incomplete or malformed lines are silently skipped so the
/// caller never crashes on a half-typed identifier.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::collect_query_refs;
/// let refs = collect_query_refs("feature billing\n  query.lookup me\n");
/// assert!(refs.contains(&"billing.query.me".to_owned()));
/// ```
pub fn collect_query_refs(source: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();
    let mut current_feature: Option<String> = None;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = crate::leading_spaces(line);
        if indent == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            continue;
        }
        if indent != 2 {
            continue;
        }
        let Some(feature) = current_feature.as_deref() else {
            continue;
        };
        for prefix in ["query.list ", "query.lookup ", "query.sql ", "query.view "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    let query_ref = format!("{feature}.query.{name}");
                    if seen.insert(query_ref.clone()) {
                        refs.push(query_ref);
                    }
                }
            }
        }
    }
    refs
}

/// Walk the source and return the policy-category names declared
/// inside the named feature's `policies` block. When `feature_hint` is
/// `None`, returns categories from whatever feature the scanner is
/// currently inside (caller is responsible for narrowing).
///
/// Powers the route-guard `@policy.<name>` completion provider — the
/// category list is the closed completion set, scoped to the surrounding
/// feature.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::collect_policy_categories_for_feature;
/// let source = "feature billing\n  policies\n    admin: @user.is_admin\n";
/// let cats = collect_policy_categories_for_feature(source, Some("billing"));
/// assert!(cats.contains(&"admin".to_owned()));
/// ```
pub fn collect_policy_categories_for_feature(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut in_policies = false;
    let mut policies_indent = 0;

    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = crate::leading_spaces(line);
        if indent == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            in_policies = false;
            continue;
        }
        let feature_matches = match (feature_hint, current_feature.as_deref()) {
            (Some(expected), Some(current)) => expected == current,
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if !feature_matches {
            continue;
        }
        if trimmed == "policies" || trimmed.starts_with("policies ") {
            in_policies = true;
            policies_indent = indent;
            continue;
        }
        if in_policies {
            if indent <= policies_indent {
                in_policies = false;
                continue;
            }
            if let Some(colon) = trimmed.find(':') {
                let name = trimmed[..colon].trim();
                if !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && seen.insert(name.to_owned())
                {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names
}

/// Find the name of the `feature <name>` block enclosing `position`.
/// Returns `None` if the cursor sits above the first feature header or
/// inside a top-level block (`app`, `workspace`, `contract`, etc.).
///
/// Best-effort indent walk; safe in tight UI loops (completion / hover
/// / code-action paths).
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::enclosing_feature_name;
/// use tower_lsp::lsp_types::Position;
///
/// let source = "feature billing\n  query.lookup me\n";
/// let name = enclosing_feature_name(source, Position { line: 1, character: 4 });
/// assert_eq!(name, Some("billing".into()));
/// ```
pub fn enclosing_feature_name(source: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if crate::leading_spaces(line) == 0 {
            if let Some(rest) = trimmed.strip_prefix("feature ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    return Some(name.to_owned());
                }
                return None;
            }
            return None;
        }
    }
    None
}

/// Walk the source and return every `key <name>` declared inside the
/// named feature's `translation` block, in document order.
/// De-duplicated.
///
/// Used by the error-vocab completion provider to surface valid
/// `@translation.<key>` targets for `when_denied` and `errors.<code>
/// message` slots.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::collect_translation_keys_for_feature;
/// let source = "feature billing\n  translation\n    key invoice_paid\n";
/// let keys = collect_translation_keys_for_feature(source, "billing");
/// assert_eq!(keys, vec!["invoice_paid".to_owned()]);
/// ```
pub fn collect_translation_keys_for_feature(source: &str, feature_name: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut in_feature = false;
    let mut in_translation = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = crate::leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false);
            in_translation = false;
            continue;
        }
        if !in_feature {
            continue;
        }
        if indent == 2 {
            in_translation = trimmed == "translation" || trimmed.starts_with("translation ");
            continue;
        }
        if in_translation
            && indent == 4
            && let Some(rest) = trimmed.strip_prefix("key ")
        {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() && seen.insert(name.to_owned()) {
                keys.push(name.to_owned());
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_query_refs_picks_up_feature_scoped_queries() {
        let source = "\
feature accounts
  query.lookup me
  query.list active

feature billing
  query.sql month_summary
";
        let refs = collect_query_refs(source);
        assert!(refs.contains(&"accounts.query.me".to_owned()));
        assert!(refs.contains(&"accounts.query.active".to_owned()));
        assert!(refs.contains(&"billing.query.month_summary".to_owned()));
    }

    #[test]
    fn enclosing_feature_name_walks_back_to_header() {
        let source = "feature billing\n  query.lookup me\n    policy admin\n";
        let pos = Position {
            line: 2,
            character: 4,
        };
        assert_eq!(enclosing_feature_name(source, pos), Some("billing".into()));
    }

    #[test]
    fn enclosing_feature_name_returns_none_above_features() {
        assert_eq!(
            enclosing_feature_name(
                "# header\n",
                Position {
                    line: 0,
                    character: 0
                }
            ),
            None
        );
    }

    #[test]
    fn collect_policy_categories_filters_by_feature() {
        let source = "\
feature billing
  policies
    admin: @user.is_admin
    owner: @user.is_owner

feature support
  policies
    agent: @user.is_agent
";
        let billing = collect_policy_categories_for_feature(source, Some("billing"));
        assert!(billing.contains(&"admin".to_owned()));
        assert!(billing.contains(&"owner".to_owned()));
        assert!(!billing.contains(&"agent".to_owned()));
    }

    #[test]
    fn collect_translation_keys_returns_named_keys() {
        let source = "\
feature billing
  translation
    key invoice_paid
    key invoice_failed
";
        let keys = collect_translation_keys_for_feature(source, "billing");
        assert_eq!(keys, vec!["invoice_paid", "invoice_failed"]);
    }
}
