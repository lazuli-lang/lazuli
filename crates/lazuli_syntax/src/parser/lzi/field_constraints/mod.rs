//! Inline field-constraint parser. Shared between resource fields,
//! command/api input slots, and query params.
//!
//! L0 #3 §10 promoted the constraint vocabulary from a doctor-only
//! grep pass into a typed surface: every `<TypeRef> [decorators...]
//! [required|optional|unique] [<constraint>...] [= <default>] [derived
//! from <expr>]` tail is sliced here and surfaced as a
//! `FieldConstraintsDecl` next to the type text.
//!
//! Closed catalog (see `docs/canonical-semantics.md` §"Constraints"):
//!
//! ```text
//! min <int>
//! max <int>
//! length <int>
//! pattern "<regex>"
//! between <A> and <B>
//! in [<value>, <value>, ...]                     # quoted strings or bare ints
//! validate sanitize_html(<profile>)
//! validate utf8_safe
//! validate max_recursion:<u32>
//! validate max_size:<u64>
//! validator covers_pii[:<sub-tag>]
//! ```
//!
//! Combination-rule enforcement (e.g. min ≤ max, length conflicts with
//! between on integers) lives analyzer-side. The parser only checks
//! shape: integer parsing, unbalanced brackets, missing quoted-string
//! terminators, and duplicate keywords on the same field.
//!
//! Visibility: every cross-cluster consumer (command.rs, query.rs,
//! the resource parser) calls into this module through:
//!
//! - `split_resource_field_after` — full peel of constraints / default
//!   / derived / modifiers, called once per resource field line.
//! - `extract_field_constraints` — just the inline-constraint peel
//!   (constraints only), called from command input and query param
//!   slots which have their own default / modifier parsing.
//!
//! All other helpers stay private; the catalog is fully closed.

mod parsers;

#[cfg(test)]
mod tests;

use super::super::common::{SourceLine, find_token, line_error};
use super::super::error::ParseError;

use crate::ast::FieldConstraintsDecl;

use parsers::{
    ParsedValidateConstraint, parse_constraint_between, parse_constraint_in_list,
    parse_constraint_int, parse_constraint_string, parse_constraint_validate,
    parse_constraint_validator,
};

/// Split `<TypeRef> [decorators...] [required|optional|unique]
/// [<constraint>...] [= <default>] [derived from <expr>]` into
/// structured pieces. L0 #3 §10 adds the constraint axis between
/// modifiers and the default — but we peel from the right, so the
/// order doesn't matter on input. Constraint keywords (`min`, `max`,
/// `pattern`, `between`, `length`, `in`) are scanned via
/// `find_token` at depth 0 so they don't trip on parenthesised
/// decorator args.
pub(super) fn split_resource_field_after(
    line: &SourceLine<'_>,
    after: &str,
) -> Result<
    (
        String,
        String,
        Option<String>,
        Option<String>,
        FieldConstraintsDecl,
    ),
    ParseError,
> {
    let after = after.trim();

    // Pull out `derived from <expr>` (always at the end).
    let (head, derived_from) = if let Some(idx) = find_token(after, " derived from ") {
        let derived = after[idx + " derived from ".len()..].trim().to_owned();
        (after[..idx].trim_end().to_owned(), Some(derived))
    } else {
        (after.to_owned(), None)
    };

    // Pull out ` = <default>`.
    let (head, default) = if let Some(idx) = find_default_assignment(&head) {
        let value = head[idx + " = ".len()..].trim().to_owned();
        (head[..idx].trim_end().to_owned(), Some(value))
    } else {
        (head, None)
    };

    // Pull out inline constraints (closed catalog).
    let (head, constraints) = extract_field_constraints(line, &head)?;

    // Now split type (paren-aware) from trailing modifier tokens.
    let (type_text, modifiers_text) = split_type_and_modifiers(&head);
    Ok((
        type_text,
        modifiers_text,
        default,
        derived_from,
        constraints,
    ))
}

/// L0 #3 §10 — scan the field tail for inline constraint keywords.
/// Returns the head text with constraint segments removed plus a
/// populated `FieldConstraintsDecl`. Each keyword is recognised at
/// depth 0 (outside parens/brackets) and stripped from the head so
/// the remaining text walks cleanly through `split_type_and_modifiers`.
///
/// Catalog: `min N`, `max N`, `pattern "STRING"`, `between A and B`,
/// `length N`, `in [a, b, c]`, `validate sanitize_html(profile)`.
/// Combination rule enforcement happens
/// in the analyzer; the parser only captures presence + values and
/// reports basic shape errors (unparsable integer, missing bracket).
pub(super) fn extract_field_constraints(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(String, FieldConstraintsDecl), ParseError> {
    let mut head = text.to_owned();
    let mut constraints = FieldConstraintsDecl::default();
    // Loop until no more constraint keywords appear. Each iteration
    // peels at most one keyword off the right; ordering of multiple
    // constraints in the source is preserved by the iterative scan.
    loop {
        let scan = head.clone();
        if let Some((before, rest)) = find_constraint_keyword(&scan) {
            let rest = rest.trim_start();
            match before_keyword_after(&scan, &rest) {
                // `in [...]` — bracketed list.
                ConstraintKw::In => {
                    let (values, tail) = parse_constraint_in_list(line, rest)?;
                    if constraints.r#in.is_some() {
                        return Err(line_error(line, "duplicate `in` constraint on field"));
                    }
                    constraints.r#in = Some(values);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Min => {
                    let (n, tail) = parse_constraint_int(line, rest, "min")?;
                    if constraints.min.is_some() {
                        return Err(line_error(line, "duplicate `min` constraint on field"));
                    }
                    constraints.min = Some(n);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Max => {
                    let (n, tail) = parse_constraint_int(line, rest, "max")?;
                    if constraints.max.is_some() {
                        return Err(line_error(line, "duplicate `max` constraint on field"));
                    }
                    constraints.max = Some(n);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Length => {
                    let (n, tail) = parse_constraint_int(line, rest, "length")?;
                    if n < 0 {
                        return Err(line_error(
                            line,
                            "`length` constraint must be a non-negative integer",
                        ));
                    }
                    if constraints.length.is_some() {
                        return Err(line_error(line, "duplicate `length` constraint on field"));
                    }
                    constraints.length = Some(n as usize);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Pattern => {
                    let (pat, tail) = parse_constraint_string(line, rest, "pattern")?;
                    if constraints.pattern.is_some() {
                        return Err(line_error(line, "duplicate `pattern` constraint on field"));
                    }
                    constraints.pattern = Some(pat);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Between => {
                    let (lo, hi, tail) = parse_constraint_between(line, rest)?;
                    if constraints.between.is_some() {
                        return Err(line_error(line, "duplicate `between` constraint on field"));
                    }
                    constraints.between = Some((lo, hi));
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Validate => {
                    let (validated, tail) = parse_constraint_validate(line, rest)?;
                    match validated {
                        ParsedValidateConstraint::SanitizeHtml(profile) => {
                            if constraints.sanitize_html.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate sanitize_html` constraint on field",
                                ));
                            }
                            constraints.sanitize_html = Some(profile);
                        }
                        ParsedValidateConstraint::Utf8Safe => {
                            if constraints.utf8_safe.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate utf8_safe` constraint on field",
                                ));
                            }
                            constraints.utf8_safe = Some(true);
                        }
                        ParsedValidateConstraint::MaxRecursion(n) => {
                            if constraints.max_recursion.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate max_recursion` constraint on field",
                                ));
                            }
                            constraints.max_recursion = Some(n);
                        }
                        ParsedValidateConstraint::MaxSize(n) => {
                            if constraints.max_size.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate max_size` constraint on field",
                                ));
                            }
                            constraints.max_size = Some(n);
                        }
                    }
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Validator => {
                    let (validator, tail) = parse_constraint_validator(line, rest)?;
                    if constraints.covers_pii.is_some() {
                        return Err(line_error(
                            line,
                            "duplicate `validator covers_pii` constraint on field",
                        ));
                    }
                    constraints.covers_pii = Some(validator);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
            }
        } else {
            break;
        }
    }
    Ok((head, constraints))
}

#[derive(Debug, Clone, Copy)]
enum ConstraintKw {
    Min,
    Max,
    Pattern,
    Between,
    Length,
    In,
    Validate,
    Validator,
}

/// Find the first constraint keyword in `text` (at depth 0). Returns
/// `(before_keyword, after_keyword_including_kw_and_args)`. Returns
/// `None` when no recognized keyword is found.
fn find_constraint_keyword(text: &str) -> Option<(&str, &str)> {
    // Catalog of probes, each `(needle, kw)`. We scan once over the
    // text and pick the earliest occurrence so multiple constraints
    // peel left-to-right deterministically.
    let needles: &[(&str, ConstraintKw)] = &[
        (" min ", ConstraintKw::Min),
        (" max ", ConstraintKw::Max),
        (" pattern ", ConstraintKw::Pattern),
        (" between ", ConstraintKw::Between),
        (" length ", ConstraintKw::Length),
        (" in ", ConstraintKw::In),
        (" validate ", ConstraintKw::Validate),
        (" validator ", ConstraintKw::Validator),
    ];
    let mut best: Option<usize> = None;
    for (needle, _) in needles {
        if let Some(idx) = find_token(text, needle) {
            best = Some(best.map_or(idx, |b| b.min(idx)));
        }
    }
    let idx = best?;
    let before = &text[..idx];
    // Include the leading space so callers can identify which kw was
    // matched without re-scanning.
    let after = &text[idx + 1..];
    Some((before, after))
}

/// Pick the constraint kind from `after_keyword_text` (which starts
/// with the keyword token plus its args).
fn before_keyword_after(_full: &str, after_keyword_text: &str) -> ConstraintKw {
    if after_keyword_text.starts_with("min ") {
        ConstraintKw::Min
    } else if after_keyword_text.starts_with("max ") {
        ConstraintKw::Max
    } else if after_keyword_text.starts_with("pattern ") {
        ConstraintKw::Pattern
    } else if after_keyword_text.starts_with("between ") {
        ConstraintKw::Between
    } else if after_keyword_text.starts_with("length ") {
        ConstraintKw::Length
    } else if after_keyword_text.starts_with("in ") {
        ConstraintKw::In
    } else if after_keyword_text.starts_with("validate ") {
        ConstraintKw::Validate
    } else if after_keyword_text.starts_with("validator ") {
        ConstraintKw::Validator
    } else {
        // Should be unreachable because find_constraint_keyword
        // matched one of these; defensive default.
        ConstraintKw::Min
    }
}

fn split_type_and_modifiers(text: &str) -> (String, String) {
    // Walk from the right, peeling off ` required` / ` optional` / ` unique`
    // trailing modifiers. The type text (which may contain parenthesised
    // decorator args like `@cap.Encrypted(key:@key.tenant)`) stays
    // structurally untouched because the modifier suffixes are bare
    // identifiers that never occur inside the paren-balanced span.
    let mut head = text.to_owned();
    let mut modifiers = Vec::new();
    loop {
        let trimmed = head.trim_end();
        if trimmed.ends_with(" required") {
            modifiers.push("required");
            head = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            modifiers.push("optional");
            head = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else if trimmed.ends_with(" unique") {
            modifiers.push("unique");
            head = trimmed[..trimmed.len() - " unique".len()].to_owned();
        } else {
            head = trimmed.to_owned();
            break;
        }
    }
    (head, modifiers.join(" "))
}

/// Find ` = ` outside of parens/brackets. The default literal may itself
/// contain `=` (rare), but the fixture's default literals are simple
/// (`= lead`, `= 0`).
fn find_default_assignment(text: &str) -> Option<usize> {
    find_token(text, " = ")
}
