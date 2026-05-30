//! PascalCase / lowerCamelCase helpers shared across `.lzx` emitters.
//!
//! Acronyms (`id`, `url`, `api`, ...) are preserved as fully-uppercase
//! segments to match the runtime emitter's casing conventions.

/// Convert a snake_case / kebab-case / mixed identifier to PascalCase.
/// Acronyms (`id`, `url`, `api`, ...) are preserved as fully-uppercase
/// segments to match the runtime emitter's casing conventions.
pub(crate) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(['_', '-', ' ']) {
        if word.is_empty() {
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
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    out
}

/// lowerCamelCase: same as `pascal_case` but the very first character
/// stays lowercase.
pub(crate) fn lower_camel(s: &str) -> String {
    let pascal = pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut out = String::with_capacity(pascal.len());
            for c in first.to_lowercase() {
                out.push(c);
            }
            out.push_str(chars.as_str());
            out
        }
    }
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

/// Split a PascalCase string into its component tokens. E.g.
/// `ListMineSlugs` → `["List", "Mine", "Slugs"]`. Used by the
/// resource-plural de-duplication logic in `idents`.
pub(crate) fn pascal_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, ch) in value.char_indices().skip(1) {
        if ch.is_ascii_uppercase() {
            tokens.push(value[start..index].to_owned());
            start = index;
        }
    }
    tokens.push(value[start..].to_owned());
    tokens.retain(|token| !token.is_empty());
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_handles_snake_kebab_and_acronyms() {
        assert_eq!(pascal_case("slug_list"), "SlugList");
        assert_eq!(pascal_case("workspace-admin"), "WorkspaceAdmin");
        assert_eq!(pascal_case("workspace_admin"), "WorkspaceAdmin");
        // `id` is in the acronym table; uppercased segments are kept.
        assert_eq!(pascal_case("by_id"), "ByID");
        assert_eq!(pascal_case("user_id"), "UserID");
        assert_eq!(pascal_case(""), "");
    }

    #[test]
    fn lower_camel_first_char_lowercases() {
        assert_eq!(lower_camel("admin"), "admin");
        assert_eq!(lower_camel("workspace-admin"), "workspaceAdmin");
        assert_eq!(lower_camel("Slug"), "slug");
    }
}
