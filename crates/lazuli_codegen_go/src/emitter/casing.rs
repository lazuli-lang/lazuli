//! Shared Go identifier casing helpers. Resources, records, enums,
//! commands, queries, auth blocks, jobs, webhooks, and notifications
//! all need to convert DSL names (PascalCase resources, snake_case
//! fields, kebab-case adapter refs) into Go identifiers
//! (`PascalCase` for exported types/vars, `lowerCamel` for unexported
//! values). Before this module each emitter carried its own
//! pascal/camel helpers — eight copies — and the
//! `lower_camel(<PascalCase>)` branch was implemented inconsistently
//! (some collapsed multi-word PascalCase to all-lowercase).
//!
//! This module is the single source of truth; every emitter
//! imports `casing::{pascal_case, lower_camel}`. Acronym preservation
//! (`api` → `API`, `id` → `ID`) lives here as a single closed table.

/// Convert a DSL identifier to Go `PascalCase`. Splits on `_`/`-` and
/// on case transitions inside the input, then uppercases the first
/// letter of each segment. Recognised acronyms (`id`, `url`, …)
/// uppercase entirely so `id_lookup` becomes `IDLookup` rather than
/// `IdLookup`.
pub fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in split_words(s) {
        if word.is_empty() {
            continue;
        }
        if is_acronym(&word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

/// Convert a DSL identifier to Go `lowerCamel`. Derives from
/// [`pascal_case`] so both snake_case (`customer_import_batch`) and
/// PascalCase (`CustomerImportBatch`) inputs land on the same
/// canonical `customerImportBatch` form. The leading word is always
/// fully lower-cased — even when it's an acronym — so `id_lookup`
/// becomes `idLookup` rather than `iDLookup`.
pub fn lower_camel(s: &str) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(&word.to_ascii_lowercase());
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

/// Closed-catalog acronym table. Identifiers matching one of these
/// (case-insensitive) preserve uppercase across pascal/camel
/// conversion. Add entries as new acronyms surface in the DSL — the
/// catalog is intentionally small to keep emitter output predictable.
pub fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

/// Split a mixed-case identifier into discrete words. Handles three
/// separator forms:
///
/// - explicit `_` / `-` (snake_case / kebab-case);
/// - lower-to-upper transitions (`fooBarBaz` → `foo`, `Bar`, `Baz`);
/// - run-of-uppercase to lowercase (`HTTPRequest` → `HTTP`, `Request`).
fn split_words(s: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    let flush = |current: &mut String, words: &mut Vec<String>| {
        if !current.is_empty() {
            words.push(std::mem::take(current));
        }
    };

    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            flush(&mut current, &mut words);
            continue;
        }
        if ch.is_ascii_uppercase() {
            let prev_lower = i > 0
                && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
            if !current.is_empty() && (prev_lower || next_lower) {
                // A new word starts here either because we just left a
                // lowercase/digit run (`fooBar`) or because we're at
                // the tail of an uppercase run followed by lowercase
                // (`HTTPRequest` — `HTTP` ends before `R`).
                if !next_lower {
                    // Mid-uppercase run: keep adding to current.
                    current.push(ch);
                    continue;
                }
                flush(&mut current, &mut words);
            }
        }
        current.push(ch);
    }
    flush(&mut current, &mut words);
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_passes_pascal_through() {
        assert_eq!(pascal_case("CustomerImportBatch"), "CustomerImportBatch");
    }

    #[test]
    fn pascal_capitalises_snake() {
        assert_eq!(pascal_case("customer_import_batch"), "CustomerImportBatch");
    }

    #[test]
    fn pascal_preserves_acronym() {
        assert_eq!(pascal_case("id_lookup"), "IDLookup");
    }

    #[test]
    fn lower_camel_pascal_input() {
        assert_eq!(
            lower_camel("CustomerImportBatch"),
            "customerImportBatch"
        );
    }

    #[test]
    fn lower_camel_snake_input() {
        assert_eq!(
            lower_camel("customer_import_batch"),
            "customerImportBatch"
        );
    }

    #[test]
    fn lower_camel_single_word() {
        assert_eq!(lower_camel("Customer"), "customer");
        assert_eq!(lower_camel("customer"), "customer");
    }

    #[test]
    fn lower_camel_empty_input() {
        assert_eq!(lower_camel(""), "");
    }

    #[test]
    fn lower_camel_leading_acronym_lowers() {
        // Leading word is always lower-cased, even acronyms.
        assert_eq!(lower_camel("id_lookup"), "idLookup");
    }
}
