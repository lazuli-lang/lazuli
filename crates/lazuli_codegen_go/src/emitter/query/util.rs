//! Cell E4 — utility helpers for the `Query` emitter sub-tree. These
//! are leaf functions: lower-camel / pascal-case / pluralization /
//! string escaping / banner painting / token splitting used by every
//! sibling (`list.rs`, `lookup.rs`, `sql.rs`, `header.rs`, `args.rs`,
//! `filters.rs`).
//!
//! Nothing here depends on Lazuli Go runtime mechanics — these are pure
//! string transforms. The only IR touchpoint is `resource_for_query`,
//! which scores resources by name overlap to attach a `Query` to its
//! semantic resource even when several are declared on the same
//! feature.

use lazuli_ir::{Feature, Query, Resource, TypeRef};

use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};

/// Score-based resolution from `Query.name` -> `Resource`. The synth
/// commonly attaches a single resource per feature, in which case the
/// scorer is a no-op. When multiple resources live on the feature the
/// scorer matches identifier tokens (with `plural()` tolerance) so
/// `list_hosts` -> `Host` and `lookup_user_session` -> `UserSession`.
pub(in crate::emitter) fn resource_for_query<'a>(
    feature: &'a Feature,
    query_name: &str,
) -> Option<&'a Resource> {
    let mut resources: Vec<&Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    if resources.len() <= 1 {
        return resources.into_iter().next();
    }

    let query_tokens = split_ident_tokens(query_name);
    resources
        .into_iter()
        .map(|resource| {
            let tokens = split_ident_tokens(&resource.name);
            let last = tokens.last().cloned().unwrap_or_default();
            let mut score = 0usize;
            for token in &tokens {
                if query_tokens
                    .iter()
                    .any(|q| q == token || q == &plural(token))
                {
                    score += 10;
                }
            }
            if !last.is_empty()
                && query_tokens
                    .iter()
                    .any(|q| q == &last || q == &plural(&last))
            {
                score += 50;
            }
            (score, resource)
        })
        .max_by(|(score_a, a), (score_b, b)| score_a.cmp(score_b).then_with(|| b.name.cmp(&a.name)))
        .map(|(_, resource)| resource)
}

/// Determinism rank for `(name, kind)` sort: `query.list` first, then
/// `query.lookup`, then `query.view`, then `query.sql`. Two queries
/// with the same name and different kinds is illegal upstream but the
/// emitter still needs a total order for `sort_by`.
pub(super) fn query_kind_rank(query: &Query) -> u8 {
    match query {
        Query::List(_) => 0,
        Query::Lookup(_) => 1,
        Query::Sql(q) if q.sql_kind == lazuli_ir::SqlQueryKind::View => 2,
        Query::Sql(_) => 3,
        // query.compose: W5 — sorts last for now; the determinism rank only
        // needs a total order, refined when compose codegen lands.
        Query::Compose(_) => 4,
    }
}

/// Lower-camel of a (possibly snake_case) identifier — delegates to
/// the shared `casing` module so query.gen.go stays byte-equivalent
/// with `command.gen.go` for matching identifier shapes.
pub(super) fn lower_camel(s: &str) -> String {
    super::super::casing::lower_camel(s)
}

/// Pascal-case of a (possibly snake_case) identifier — same
/// delegation as `lower_camel`.
pub(super) fn pascal_case(s: &str) -> String {
    super::super::casing::pascal_case(s)
}

/// Snake-case from PascalCase. Used by the owner-scope FK lift to
/// derive the related table name (`Host` -> `host`,
/// `UserSession` -> `user_session`).
pub(super) fn pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Naive PascalCase pluralizer. `Host` -> `Hosts`, `Story` -> `Stories`,
/// `Address` -> `Addresses`. Sufficient for the synth's list-args
/// naming convention (`ListHostsArgs`).
pub(super) fn plural_pascal(s: &str) -> String {
    if let Some(stem) = s.strip_suffix('y') {
        return format!("{stem}ies");
    }
    if s.ends_with('s') {
        format!("{s}es")
    } else {
        format!("{s}s")
    }
}

/// Naive lowercase pluralizer. Used by `resource_for_query` scoring
/// only — never emitted into Go source.
pub(super) fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ies")
    } else if word.ends_with('s') {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

/// Project a `TypeRef` back to its human-facing return-name as it
/// appears in the section banner / Returns string. `Many(T)` becomes
/// `T[]`; fully qualified types render `feature.Name`.
pub(super) fn return_name(type_ref: &TypeRef, ctx: &TypeCtx<'_>) -> String {
    match type_ref {
        TypeRef::Many(inner) => format!("{}[]", return_name(inner, ctx)),
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => match qname.feature.as_deref() {
            Some(feature) => format!("{}.{}", feature, qname.name),
            None => qname.name.clone(),
        },
        other => {
            let (go, _import) = types::go_type_for(other, ctx);
            go
        }
    }
}

/// Paint a `// ---- / // Query: name / // ----` banner before every
/// emitted query value. Keeps `query.gen.go` skimmable.
pub(super) fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

/// Local helper used only by `resource_for_query` scoring: split an
/// identifier into lowercase tokens. The shared `casing::split_words`
/// preserves case (`fooBar` -> `["foo", "Bar"]`); the scorer wants
/// lowercase tokens for direct equality + plural matching.
pub(super) fn split_ident_tokens(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower_or_digit = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.to_ascii_lowercase());
                current.clear();
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !current.is_empty() {
            words.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        words.push(current.to_ascii_lowercase());
    }
    words
}

/// Escape a raw string for embedding inside a Go `"..."` literal.
/// Only `\` and `"` need replacing in the shapes Lazuli emits;
/// control characters do not appear in any user-facing string fed
/// to this helper.
pub(super) fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}
