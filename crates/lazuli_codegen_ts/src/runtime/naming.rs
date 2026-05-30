//! Identifier-case helpers shared across the runtime TS emitter.
//!
//! Owns three primitives — `pascal_case`, `lower_camel`, `field_kind_ts` —
//! plus the `is_acronym` table that keeps `id` / `url` / `api` etc. all-caps
//! across both casings. The runtime emitter, query naming heuristics, and
//! lifecycle action maps all consume these; the case helpers also have to
//! agree with `runtime/ts/lazuli/src/case-mapper.ts` so the wire boundary
//! round-trips snake_case ↔ camelCase exactly.
//!
//! `lower_camel_export` is the public re-export entry point: the CLI zod
//! emitter consumes it via `lazuli_codegen_ts::lower_camel_export` so SDK
//! and zod surfaces share a single casing source of truth.

use lazuli_codegen_spec::FieldKind;

/// Convert an identifier (snake_case / kebab-case / single word) to
/// camelCase. First word stays lowercase; subsequent words title-case
/// the first letter only. `org_id` → `orgId`, `api_url` → `apiUrl`,
/// `created_at` → `createdAt`. Idiomatic for TypeScript field names;
/// matches the runtime `case-mapper.ts` `snake_to_camel` so the wire
/// boundary round-trips exactly.
///
/// Re-exported via `lazuli_codegen_ts::lower_camel_export` for the CLI
/// zod emitter (single source of truth for SDK/zod casing alignment).
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::lower_camel_export;
/// assert_eq!(lower_camel_export("org_id"), "orgId");
/// assert_eq!(lower_camel_export("created_at"), "createdAt");
/// ```
pub fn lower_camel_export(s: &str) -> String {
    lower_camel(s)
}

pub(super) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(['_', '-']) {
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
        out.push_str(chars.as_str());
    }
    out
}

pub(super) fn lower_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for word in s.split(['_', '-']) {
        if word.is_empty() {
            continue;
        }
        if first {
            out.push_str(&word.to_ascii_lowercase());
            first = false;
            continue;
        }
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            for u in c.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    out
}

pub(super) fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

pub(super) fn field_kind_ts(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text | FieldKind::Email => "string",
        FieldKind::Integer => "ID",
        FieldKind::Boolean => "boolean",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_camel_export_handles_snake_case_identifiers() {
        assert_eq!(lower_camel_export("org_id"), "orgId");
        assert_eq!(lower_camel_export("created_at"), "createdAt");
    }

    #[test]
    fn pascal_case_uppercases_known_acronyms() {
        assert_eq!(pascal_case("api_url"), "APIURL");
        assert_eq!(pascal_case("user_id"), "UserID");
    }

    #[test]
    fn field_kind_ts_maps_known_kinds() {
        assert_eq!(field_kind_ts(FieldKind::Text), "string");
        assert_eq!(field_kind_ts(FieldKind::Integer), "ID");
        assert_eq!(field_kind_ts(FieldKind::Boolean), "boolean");
    }
}
