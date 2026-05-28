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

use crate::ast::{CrossFeatureTargetAst, OwnerAxisAst, ResourceFieldDecl, Span};

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
    let tail = field_constraints::split_resource_field_after(header, after)?;
    let raw_type_text = tail.type_text;
    let modifiers_text = tail.modifiers_text;
    let default = tail.default;
    let derived_from = tail.derived_from;
    let computed_date = tail.computed_date;
    let constraints = tail.constraints;
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

    // GAP-12 — peel `target @feature.<feature>.<Resource>` out of the
    // type tail into a typed `ResourceFieldDecl.cross_feature_target`.
    // The analyzer projects this onto `ir::Field.cross_feature_target`;
    // doctor cross-checks the feature against the declaring feature's
    // `uses` (Dependencies) and the resource against that feature.
    let (type_text, cross_feature_target) = extract_cross_feature_target(header, &type_text)?;

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
            computed_date,
            constraints,
            full_text,
            owner_axis,
            cross_feature_target,
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

/// GAP-12 — peel `target @feature.<feature>.<Resource>` off a resource
/// field's type text. Returns the cleaned type text plus the optional
/// cross-feature target payload.
///
/// Grammar: the literal keyword `target`, then `@feature.<feature>
/// .<Resource>` — exactly two dot-separated identifier segments after the
/// `@feature.` prefix. The annotation is only meaningful on `ID` fields;
/// the analyzer rejects it on non-`ID` types. Recognized as a bare
/// trailing clause (after the type token, e.g. `ID target
/// @feature.agency.Department`).
///
/// Errors:
/// - `target` not followed by `@feature.<feature>.<Resource>`.
/// - missing feature or resource segment.
/// - duplicate `target ...` on the same field.
fn extract_cross_feature_target(
    line: &SourceLine<'_>,
    type_text: &str,
) -> Result<(String, Option<CrossFeatureTargetAst>), ParseError> {
    // The clause is `target @feature.<feature>.<Resource>`. Locate the
    // `target` token at a word boundary (start-of-text or after space).
    let needle = "target ";
    let hit = if type_text.starts_with(needle) {
        Some(0usize)
    } else {
        type_text.find(" target ").map(|idx| idx + 1)
    };
    let Some(start) = hit else {
        return Ok((type_text.to_owned(), None));
    };
    let after = type_text[start + needle.len()..].trim_start();
    // The qualifier reference runs to the next whitespace (the clause is
    // a bare dotted path with no parens).
    let (reference, tail) = match after.find(char::is_whitespace) {
        Some(idx) => (&after[..idx], after[idx..].trim_start()),
        None => (after, ""),
    };
    let Some(qualified) = reference.strip_prefix("@feature.") else {
        return Err(line_error(
            line,
            "`target` requires `@feature.<feature>.<Resource>`",
        ));
    };
    let mut segments = qualified.split('.');
    let feature = segments.next().unwrap_or("").trim();
    let resource = segments.next().unwrap_or("").trim();
    if feature.is_empty() || resource.is_empty() || segments.next().is_some() {
        return Err(line_error(
            line,
            "`target @feature.<feature>.<Resource>` requires exactly a feature \
             and a resource segment",
        ));
    }
    // Reassemble the cleaned type text (everything before `target` plus
    // any trailing tokens after the reference).
    let before = type_text[..start].trim_end();
    let mut cleaned = before.to_owned();
    if !cleaned.is_empty() && !tail.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(tail);
    // A second `target` clause is a parse error.
    if cleaned.contains("target @feature.") {
        return Err(line_error(line, "duplicate `target` clause on field"));
    }
    Ok((
        cleaned.trim().to_owned(),
        Some(CrossFeatureTargetAst {
            feature: feature.to_owned(),
            resource: resource.to_owned(),
        }),
    ))
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

#[cfg(test)]
mod field_tests {
    use super::super::super::parse_feature_skeletons;

    #[test]
    fn parses_slug_field_decorator() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required
    title: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let r = &features[0].resources[0];
        assert_eq!(r.fields.len(), 2);
        // First field is the slug field; `@slug` peeled, type clean.
        assert_eq!(r.fields[0].name, "slug");
        assert!(r.fields[0].slug, "`@slug` should peel into Field.slug");
        assert!(r.fields[0].required);
        assert!(
            !r.fields[0].type_text.contains("@slug"),
            "@slug should be stripped from type_text; got: {}",
            r.fields[0].type_text
        );
        // Second field has no `@slug`.
        assert!(!r.fields[1].slug);
    }

    #[test]
    fn slug_decorator_coexists_with_unique_modifier() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required unique
";
        let features = parse_feature_skeletons(source).unwrap();
        let f = &features[0].resources[0].fields[0];
        assert!(f.slug);
        assert!(f.unique);
        assert!(f.required);
    }

    // -------------------------------------------------------------------
    // `ir-resource-conventions-owner-scope` Cell O1 — `@owner_axis(through: <ident>)`
    // -------------------------------------------------------------------

    #[test]
    fn parses_owner_axis_decorator_with_through_ident() {
        let source = "
feature catalog
  resource Property
    org: Org required
    host: Host required @owner_axis(through: user)
    name: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let property = &features[0].resources[0];
        let host_field = &property.fields[1];
        assert_eq!(host_field.name, "host");
        let axis = host_field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` should peel into ResourceFieldDecl.owner_axis");
        assert_eq!(axis.through_column, "user");
        assert!(
            !host_field.type_text.contains("@owner_axis"),
            "@owner_axis should be stripped from type_text; got: {}",
            host_field.type_text,
        );
        // The neighbouring fields stay axis-free.
        assert!(property.fields[0].owner_axis.is_none());
        assert!(property.fields[2].owner_axis.is_none());
    }

    #[test]
    fn owner_axis_rejects_string_literal_argument() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis(through: \"user\")
";
        let err = parse_feature_skeletons(source).expect_err(
            "string literal in @owner_axis(through: ...) must be a parse error per §7.1",
        );
        let message = format!("{err}");
        assert!(
            message.contains("requires a bare identifier"),
            "got: {message}",
        );
    }

    // -------------------------------------------------------------------
    // GAP-12 — `target @feature.<feature>.<Resource>` cross-feature FK
    // -------------------------------------------------------------------

    #[test]
    fn parses_cross_feature_target_on_id_field() {
        let source = "
feature agency
  uses department
  resource Agency
    name: Text required
    default_department_id: ID target @feature.department.Department
";
        let features = parse_feature_skeletons(source).unwrap();
        let agency = &features[0].resources[0];
        let fk = &agency.fields[1];
        assert_eq!(fk.name, "default_department_id");
        let target = fk
            .cross_feature_target
            .as_ref()
            .expect("`target @feature.department.Department` should peel into the typed slot");
        assert_eq!(target.feature, "department");
        assert_eq!(target.resource, "Department");
        // The `target ...` clause must be stripped from type_text.
        assert!(
            !fk.type_text.contains("target"),
            "target clause should be stripped from type_text; got: {}",
            fk.type_text
        );
        assert_eq!(fk.type_text.trim(), "ID");
    }

    #[test]
    fn cross_feature_target_requires_feature_and_resource() {
        let source = "
feature agency
  resource Agency
    dep_id: ID target @feature.department
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "single-segment `@feature.department` must be a parse error"
        );
    }

    // -------------------------------------------------------------------
    // W3 GAP-03 — `computed_date from <base> offset <offset>`
    // -------------------------------------------------------------------

    #[test]
    fn parses_computed_date_with_field_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    offset_days: Integer required
    due_date: Date computed_date from campaign_start offset offset_days
";
        let features = parse_feature_skeletons(source).unwrap();
        let campaign = &features[0].resources[0];
        let due = &campaign.fields[2];
        assert_eq!(due.name, "due_date");
        let cd = due
            .computed_date
            .as_ref()
            .expect("`computed_date from ... offset ...` should peel into the typed slot");
        assert_eq!(cd.base, ComputedDateBaseAst::Field("campaign_start".into()));
        assert_eq!(
            cd.offset,
            ComputedDateOffsetAst::Field("offset_days".into())
        );
        // `computed_date ...` clause must be stripped from type_text.
        assert_eq!(due.type_text.trim(), "Date");
        assert!(!due.type_text.contains("computed_date"));
        // Sibling fields carry no computed_date.
        assert!(campaign.fields[0].computed_date.is_none());
        assert!(campaign.fields[1].computed_date.is_none());
    }

    #[test]
    fn parses_computed_date_with_integer_literal_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start offset 30
";
        let features = parse_feature_skeletons(source).unwrap();
        let due = &features[0].resources[0].fields[1];
        let cd = due.computed_date.as_ref().expect("computed_date present");
        assert_eq!(cd.base, ComputedDateBaseAst::Field("campaign_start".into()));
        assert_eq!(cd.offset, ComputedDateOffsetAst::Literal(30));
        assert_eq!(due.type_text.trim(), "Date");
    }

    // -------------------------------------------------------------------
    // W4 GAP-08 — `schedule_rule from @fn.<name>(<arg>) offset <offset>`
    // -------------------------------------------------------------------

    #[test]
    fn parses_schedule_rule_with_fn_base_and_field_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature activity
  resource Activity
    offset_days: Integer required
    rule: Text required
    due_date: Date schedule_rule from @fn.activity_date_rule(input.rule) offset offset_days
";
        let features = parse_feature_skeletons(source).unwrap();
        let activity = &features[0].resources[0];
        let due = &activity.fields[2];
        assert_eq!(due.name, "due_date");
        let cd = due
            .computed_date
            .as_ref()
            .expect("`schedule_rule from @fn...(...) offset ...` should peel into the typed slot");
        assert_eq!(
            cd.base,
            ComputedDateBaseAst::Rule {
                rule: "input.rule".into(),
                fn_ref: "activity_date_rule".into(),
            }
        );
        assert_eq!(cd.offset, ComputedDateOffsetAst::Field("offset_days".into()));
        // The `schedule_rule ...` clause must be stripped from type_text.
        assert_eq!(due.type_text.trim(), "Date");
        assert!(!due.type_text.contains("schedule_rule"));
    }

    #[test]
    fn schedule_rule_missing_fn_base_is_a_parse_error() {
        let source = "
feature activity
  resource Activity
    due_date: Date schedule_rule from start_date offset 7
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`schedule_rule` requires an `@fn.<name>(<arg>)` base, not a bare field"
        );
    }

    #[test]
    fn schedule_rule_missing_offset_is_a_parse_error() {
        let source = "
feature activity
  resource Activity
    due_date: Date schedule_rule from @fn.pick_date(input.rule)
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`schedule_rule from @fn(...)` without `offset <offset>` must be a parse error"
        );
    }

    #[test]
    fn computed_date_missing_offset_keyword_is_a_parse_error() {
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`computed_date from <base>` without `offset <offset>` must be a parse error"
        );
    }

    #[test]
    fn computed_date_and_derived_from_are_mutually_exclusive() {
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start offset 30 derived from now()
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "combining `computed_date` and `derived from` on one field must be a parse error"
        );
    }

    #[test]
    fn owner_axis_without_arguments_is_a_parse_error() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis
";
        let err = parse_feature_skeletons(source)
            .expect_err("bare @owner_axis must be rejected — annotation requires (through: ...)");
        let message = format!("{err}");
        assert!(
            message.contains("`@owner_axis` requires `(through: <ident>)`"),
            "got: {message}",
        );
    }
}
