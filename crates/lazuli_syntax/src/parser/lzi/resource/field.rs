//! Resource field-line parser.
//!
//! A single resource field declaration is `<name>: <Type> [modifiers...]`
//! at child indent of the parent `resource <Name>`. Modifiers and
//! constraints sit at the tail of the line (parsed by the shared
//! `field_constraints` module), while three decorators are *peeled* off
//! the type-text before storage so the AST exposes typed flags instead
//! of strings:
//!
//! - `@slug` — bare-token boolean (CL.C.4). The slug column gets
//!   auto-uniqueness and analyzer-side URL handling.
//! - `@full_text` — bare-token boolean (Roadmap §1.5 — CL.C.2). Marks
//!   the column as a tsvector source for `fts on (...)`.
//! - `@owner_axis(through: <ident>)` — typed payload
//!   (`ir-resource-conventions-owner-scope` §7.1). Names the foreign-key
//!   column from which the ownership chain projects. Depth-aware so it
//!   doesn't trip on nested decorator args like
//!   `@cap.Encrypted(key:@key.tenant)`.
//!
//! ```text
//! resource Customer
//!   id: ID @key.tenant
//!   email: String @semantic.email required unique
//!   slug: String @slug
//!   search: tsvector @full_text
//!   org: Org @owner_axis(through: organization_id)
//! ```
//!
//! Optional `previously migrated <old>` grandchild lines are consumed
//! immediately after the field line and lowered onto
//! `ResourceFieldDecl.previously`.
//!
//! Visibility: `parse_resource_field_decl` is `pub(super)` so the
//! resource dispatcher (`resource/mod.rs`) and the typed-record parser
//! (`lzi/record.rs`) can both call it. The decorator peelers
//! (`extract_slug_decorator`, `extract_owner_axis_decorator`,
//! `extract_full_text_marker`) are file-private — they exist solely to
//! split this entry point into readable phases.

use super::super::super::common::{SourceLine, is_trivia, line_error, strip_inline_comment};
use super::super::super::error::ParseError;
use super::super::field_constraints;

use crate::ast::{OwnerAxisAst, ResourceFieldDecl, Span};

pub(in super::super) fn parse_resource_field_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(ResourceFieldDecl, usize), ParseError> {
    let header = &lines[start];
    let raw_trimmed = header.text.trim_start();
    let trimmed = strip_inline_comment(raw_trimmed).trim_end();
    let (name_part, after) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            header,
            "resource field must be `<name>: <Type> [modifiers...]`",
        )
    })?;
    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "resource field requires a name before `:`",
        ));
    }
    let after = after.trim();
    // Split the type text from trailing modifiers honouring parens.
    let (raw_type_text, modifiers_text, default, derived_from, constraints) =
        field_constraints::split_resource_field_after(header, after)?;
    let required = modifiers_text.contains("required");
    let optional = modifiers_text.contains("optional");
    let unique = modifiers_text.contains("unique");
    // CL.C.4 — `@slug` field decorator. Lives in the type/decorator
    // chain alongside `@semantic.X`/`@pii.X`. We peel it to a typed
    // `Field.slug` bool so codegen and doctor read it from the typed
    // slot without re-scanning `type_text`. Stripped from `type_text`
    // so `type_ref_from_*` does not see an unknown token.
    let (type_text, slug) = extract_slug_decorator(&raw_type_text);

    // Roadmap §1.5 (CL.C.2) — `@full_text` field decorator. Sits in
    // the type/decorator chain alongside `@slug`/`@semantic.X`/`@pii.X`.
    // We peel it to a typed `Field.full_text` bool. Detection is
    // depth-aware so it doesn't trip on parenthesised decorator args
    // (e.g. `@cap.Encrypted(key:@key.tenant)`).
    let (type_text, full_text) = extract_full_text_marker(header, &type_text)?;

    // `ir-resource-conventions-owner-scope` §7.1 — peel
    // `@owner_axis(through: <ident>)` out of the type text into a
    // typed `ResourceFieldDecl.owner_axis` slot. The analyzer
    // projects this onto `ir::Field.owner_axis`; the synth pass (O2)
    // builds the ownership-chain WHERE-clause predicate from it.
    let (type_text, owner_axis) = extract_owner_axis_decorator(header, &type_text)?;

    // Consume optional `previously migrated <old>` grandchild lines.
    let mut previously: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let inner = line.text.trim_start();
        if is_trivia(inner) {
            i += 1;
            continue;
        }
        if line.indent != grandchild_indent {
            break;
        }
        if let Some(rest) = inner.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            i += 1;
        } else {
            break;
        }
    }

    Ok((
        ResourceFieldDecl {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            unique,
            slug,
            default,
            derived_from,
            constraints,
            full_text,
            owner_axis,
            previously,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// CL.C.4 — peel the `@slug` decorator off the raw type text. Returns
/// the cleaned type text + a bool indicating presence. `@slug` is
/// recognized as a standalone bare token (no parens) anywhere in the
/// decorator chain; other `@*` decorators (`@semantic.X`, `@pii.X`,
/// `@cap.Encrypted(...)`) stay inside the type text.
fn extract_slug_decorator(text: &str) -> (String, bool) {
    let mut parts: Vec<&str> = text.split_whitespace().collect();
    let mut slug = false;
    parts.retain(|tok| {
        if *tok == "@slug" {
            slug = true;
            false
        } else {
            true
        }
    });
    (parts.join(" "), slug)
}

/// `ir-resource-conventions-owner-scope` §7.1 — peel
/// `@owner_axis(through: <ident>)` off a resource-field type text.
/// Returns the cleaned type text plus the optional axis payload.
///
/// Grammar:
/// - `@owner_axis(through: <ident>)` — keyword, open paren,
///   `through:`, bare identifier, close paren. Whitespace flexible.
/// - `<ident>` is a snake_case identifier; string literals (`"user"`)
///   are rejected with a parse error so authors don't accidentally
///   quote a column name into a heterogeneous shape.
/// - `@owner_axis` standalone (no parens) is a parse error.
/// - `@owner_axis()` with empty body is a parse error.
/// - Duplicate `@owner_axis(...)` on the same field is a parse error.
///
/// Detection is depth-aware (must sit at paren depth 0) so the marker
/// does not collide with paren-nested decorator args like
/// `@cap.Encrypted(key:@key.tenant)`.
fn extract_owner_axis_decorator(
    line: &SourceLine<'_>,
    type_text: &str,
) -> Result<(String, Option<OwnerAxisAst>), ParseError> {
    let bytes = type_text.as_bytes();
    const NEEDLE: &[u8] = b"@owner_axis";
    let mut depth = 0i32;
    let mut hit: Option<(usize, usize, String)> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if depth == 0 && i + NEEDLE.len() <= bytes.len() && &bytes[i..i + NEEDLE.len()] == NEEDLE {
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            let after_idx = i + NEEDLE.len();
            // The keyword must be followed (after optional whitespace)
            // by `(` — bare `@owner_axis` is rejected so authors don't
            // accidentally ship the annotation without an axis column.
            let mut j = after_idx;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if before_ok {
                if j >= bytes.len() || bytes[j] as char != '(' {
                    return Err(line_error(
                        line,
                        "`@owner_axis` requires `(through: <ident>)` — bare keyword is not allowed",
                    ));
                }
                // Find the balanced closing paren.
                let mut d = 0i32;
                let mut k = j;
                let mut closed: Option<usize> = None;
                while k < bytes.len() {
                    match bytes[k] as char {
                        '(' => d += 1,
                        ')' => {
                            d -= 1;
                            if d == 0 {
                                closed = Some(k);
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                let Some(close) = closed else {
                    return Err(line_error(
                        line,
                        "`@owner_axis(...)` is missing a closing `)`",
                    ));
                };
                let body = type_text[j + 1..close].trim();
                let through = parse_owner_axis_body(line, body)?;
                if hit.is_some() {
                    return Err(line_error(
                        line,
                        "duplicate `@owner_axis(...)` decorator on field",
                    ));
                }
                hit = Some((i, close + 1, through));
                i = close + 1;
                continue;
            }
        }
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    let Some((start, end, through_column)) = hit else {
        return Ok((type_text.to_owned(), None));
    };
    let before = type_text[..start].trim_end();
    let after = type_text[end..].trim_start();
    let mut cleaned = String::with_capacity(type_text.len());
    cleaned.push_str(before);
    if !before.is_empty() && !after.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(after);
    Ok((
        cleaned.trim().to_owned(),
        Some(OwnerAxisAst { through_column }),
    ))
}

/// Parse the body of `@owner_axis(<body>)`. Body must be exactly
/// `through: <ident>` per §7.1. String literals are rejected so the
/// authored shape stays homogenous with other identifier-valued slots
/// (`@slug`, `derived from`).
fn parse_owner_axis_body(line: &SourceLine<'_>, body: &str) -> Result<String, ParseError> {
    if body.is_empty() {
        return Err(line_error(
            line,
            "`@owner_axis()` requires `through: <ident>` — empty body is not allowed",
        ));
    }
    let (key, value) = body
        .split_once(':')
        .ok_or_else(|| line_error(line, "`@owner_axis(...)` body must be `through: <ident>`"))?;
    if key.trim() != "through" {
        return Err(line_error(
            line,
            "`@owner_axis(...)` only accepts the `through:` keyword argument",
        ));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(line_error(
            line,
            "`@owner_axis(through:)` is missing the column identifier",
        ));
    }
    if value.starts_with('"') || value.starts_with('\'') {
        return Err(line_error(
            line,
            "`@owner_axis(through: <ident>)` requires a bare identifier, not a string literal",
        ));
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || value
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
    {
        return Err(line_error(
            line,
            "`@owner_axis(through: <ident>)` identifier must match `[A-Za-z_][A-Za-z0-9_]*`",
        ));
    }
    Ok(value.to_owned())
}

/// Roadmap §1.5 (CL.C.2) — peel the `@full_text` decorator off the
/// type text. Returns the cleaned type text plus a boolean flag. The
/// marker is rejected if it appears more than once. Depth-aware so
/// paren-balanced decorator args (e.g. `@cap.Encrypted(key:@key.tenant)`)
/// are left alone.
fn extract_full_text_marker(
    line: &SourceLine<'_>,
    type_text: &str,
) -> Result<(String, bool), ParseError> {
    let bytes = type_text.as_bytes();
    let needle = b"@full_text";
    let mut depth = 0i32;
    let mut hit: Option<usize> = None;
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        let ch = bytes[i] as char;
        if ch == '(' || ch == '[' {
            depth += 1;
        } else if ch == ')' || ch == ']' {
            depth -= 1;
        }
        if depth == 0 && &bytes[i..i + needle.len()] == needle {
            // Boundary check: must be preceded by start/whitespace and
            // followed by end/whitespace so `@full_text_oops` doesn't
            // match.
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            let end = i + needle.len();
            let after_ok = end == bytes.len() || (bytes[end] as char).is_whitespace();
            if before_ok && after_ok {
                if hit.is_some() {
                    return Err(line_error(
                        line,
                        "duplicate `@full_text` decorator on field",
                    ));
                }
                hit = Some(i);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    let Some(start) = hit else {
        return Ok((type_text.to_owned(), false));
    };
    let end = start + needle.len();
    let mut cleaned = String::with_capacity(type_text.len() - needle.len());
    cleaned.push_str(type_text[..start].trim_end());
    let tail = type_text[end..].trim_start();
    if !cleaned.is_empty() && !tail.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(tail);
    Ok((cleaned.trim().to_owned(), true))
}
