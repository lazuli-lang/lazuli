//! `@<ns>.<name>` prefix completion provider.
//!
//! Owns the file-local namespace-name surface for the seven closed
//! reference namespaces ([`namespace_prefix_completions`]) and the
//! companion declaration-scanner used to enumerate names from the
//! current document ([`collect_namespace_names`]).
//!
//! Supported namespaces:
//!
//! - `@policy.<name>` — keys inside `policies` blocks
//!   (e.g. `create: @role.admin` contributes `create`).
//! - `@validator.<name>` / `@fn.<name>` / `@hook.<name>` —
//!   `<keyword> <name>` lines inside `extensions` blocks.
//! - `@role.<name>` / `@scope.<name>` / `@actor.<name>` — atom
//!   tokens used anywhere in the document (declared catalogs live
//!   in `app.lzi`; the LSP file-locally surfaces what the document
//!   already references).
//!
//! Cheap text-based scan; matches the existing LSP convention
//! elsewhere in this crate.
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::leading_spaces`] — indent-aware block scanning.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use crate::leading_spaces;

/// Scan `source` for declared names under the namespace pointed to by
/// the immediately-preceding `@<ns>.` marker before the cursor. Returns
/// `None` when the cursor isn't sitting on such a marker.
///
/// Supported namespaces and how their names are collected from text:
/// - `@policy.<name>` → keys on lines inside a `policies` block
///   (e.g. `create: @role.admin` contributes `create`).
/// - `@validator.<name>` / `@fn.<name>` / `@hook.<name>` →
///   `<keyword> <name>` lines inside an `extensions` block.
/// - `@role.<name>` / `@scope.<name>` / `@actor.<name>` → atom
///   tokens used anywhere in the policies block (declared catalogs
///   live in `app.lzi` policy blocks; the LSP file-locally surfaces
///   what appears in this document).
pub(crate) fn namespace_prefix_completions(
    source: &str,
    before_cursor: &str,
) -> Option<Vec<CompletionItem>> {
    // The token under construction is the run of word chars at the
    // end of `before_cursor`; everything before it should end with
    // `@<ns>.`.
    let token_start = before_cursor
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &before_cursor[..token_start];
    let trimmed_prefix = prefix.trim_end();
    let ns = if trimmed_prefix.ends_with("@policy.") {
        "policy"
    } else if trimmed_prefix.ends_with("@validator.") {
        "validator"
    } else if trimmed_prefix.ends_with("@fn.") {
        "fn"
    } else if trimmed_prefix.ends_with("@hook.") {
        "hook"
    } else if trimmed_prefix.ends_with("@role.") {
        "role"
    } else if trimmed_prefix.ends_with("@scope.") {
        "scope"
    } else if trimmed_prefix.ends_with("@actor.") {
        "actor"
    } else {
        return None;
    };

    let names = collect_namespace_names(source, ns);
    if names.is_empty() {
        return Some(Vec::new());
    }
    Some(
        names
            .into_iter()
            .map(|name| CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::REFERENCE),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Collect declared names for a closed-namespace prefix by scanning
/// the document. Cheap text-based scan rather than a full IR walk —
/// matches the existing LSP convention used elsewhere in this crate.
pub(crate) fn collect_namespace_names(source: &str, ns: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let lines: Vec<&str> = source.lines().collect();

    match ns {
        "policy" => {
            // Walk `policies` blocks: each child line of form
            // `<name>: <atoms>` declares one feature-local category.
            let mut inside_policies = false;
            let mut block_indent: usize = 0;
            for line in &lines {
                let trimmed = line.trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                let indent = leading_spaces(line);
                if trimmed == "policies" || trimmed.starts_with("policies ") {
                    inside_policies = true;
                    block_indent = indent;
                    continue;
                }
                if inside_policies {
                    if indent <= block_indent {
                        inside_policies = false;
                        continue;
                    }
                    // Child line: `<name>: ...` or `policy_for ...`.
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
        }
        "validator" | "fn" | "hook" => {
            // Walk `extensions` blocks: child lines of form
            // `<kind> <name>: ...` declare an extension.
            let mut inside_ext = false;
            let mut block_indent: usize = 0;
            for line in &lines {
                let trimmed = line.trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                let indent = leading_spaces(line);
                if trimmed == "extensions" {
                    inside_ext = true;
                    block_indent = indent;
                    continue;
                }
                if inside_ext {
                    if indent <= block_indent {
                        inside_ext = false;
                        continue;
                    }
                    let kind_prefix = format!("{ns} ");
                    if let Some(rest) = trimmed.strip_prefix(&kind_prefix) {
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                }
            }
        }
        "role" | "scope" | "actor" => {
            // Scan every `@<ns>.<name>` token already used in the
            // document. This is best-effort surfacing rather than a
            // declaration index.
            let needle = format!("@{ns}.");
            for line in &lines {
                let mut rest: &str = line;
                while let Some(idx) = rest.find(&needle) {
                    let after = &rest[idx + needle.len()..];
                    let name_len = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .map(|c| c.len_utf8())
                        .sum::<usize>();
                    if name_len > 0 {
                        let name = after[..name_len].to_owned();
                        if seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                    if name_len == 0 {
                        // No more tokens on this line — break to
                        // avoid an infinite loop on bare `@ns.`.
                        break;
                    }
                    rest = &after[name_len..];
                }
            }
        }
        _ => {}
    }
    names
}
