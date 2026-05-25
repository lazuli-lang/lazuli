//! Case-conversion helpers used by codegen scaffolding (TS, Go, file
//! names) and the `lazuli new` template engine.
//!
//! These functions were previously inlined in `main.rs` and re-used by
//! ~150 call sites in the TS emitter and the project scaffolder. Pulled
//! out so:
//!
//! - `commands::generate::ts::*` and `commands::new::*` (the heavy
//!   clusters carved out in Wave R3-D) can import them without going
//!   through a `pub(crate)` re-export in `main.rs`.
//! - `doctor::schema_rich_001` already used `crate::pascal_case` via a
//!   re-export; that surface is preserved through `pub use casing::*`
//!   in `main.rs`.
//!
//! Each converter is pure: no I/O, no allocation beyond the output
//! `String`. The Lazuli-specific quirks (e.g. `pascal_case` preserves
//! `id`/`url`/`uri`/`html`/`json`/`sql`/`ttl` as ALL-CAPS so a Lazuli
//! `customer_id` field emits as `CustomerID` in Go and `customerID`
//! in TypeScript) are encoded once here instead of being re-implemented
//! per emitter.

/// PascalCase converter that preserves a small allow-list of acronyms
/// in ALL-CAPS (`id`, `url`, `uri`, `html`, `json`, `sql`, `ttl`).
///
/// Used to translate Lazuli identifiers (snake_case in source) into
/// TypeScript interface names, Go struct field names, and React hook
/// suffixes. Leading-digit identifiers are prefixed with `App` so the
/// result is always a valid identifier in both languages.
pub(crate) fn pascal_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for word in value.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        if out.is_empty() && word.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            out.push_str("App");
        }
        if matches!(
            word.to_ascii_lowercase().as_str(),
            "id" | "url" | "uri" | "html" | "json" | "sql" | "ttl"
        ) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }

    out
}

/// lowerCamelCase via `pascal_case` then lowercased first character.
/// When the input is already a single identifier (no separators), the
/// lowercase-first-char rule applies directly so e.g. `CustomerID`
/// becomes `customerID` rather than `customer_id`.
pub(crate) fn lower_camel(s: &str) -> String {
    let pascal = if s.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
        pascal_case(s)
    } else {
        s.to_owned()
    };
    let mut chars = pascal.chars();
    let Some(first) = chars.next() else {
        return pascal;
    };
    let mut out = String::with_capacity(pascal.len());
    for c in first.to_lowercase() {
        out.push(c);
    }
    out.push_str(chars.as_str());
    out
}

/// Lower-kebab caser used by `default_go_module_name` and
/// `default_module_name` (project scaffold). Mirrors the small kebab
/// caser in `lazuli_codegen_go::to_kebab_case`; kept local to avoid
/// pulling the codegen-go internal helpers into the CLI surface.
pub(crate) fn to_kebab_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            out.push('-');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('-');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}

/// Lower-snake caser. Symmetric counterpart to `to_kebab_case`; used by
/// the `lazuli new` template renderer for module/file-name fields.
pub(crate) fn to_snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('_');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}
