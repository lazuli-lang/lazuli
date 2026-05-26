//! Inline-validator constraint lifting + the four `validate_constraint_*`
//! gates, extracted from `resource.rs` (Rails-style R9).
//!
//! These are called from `resource::lower_resource_field` and from
//! `command_decl` / `query` typed input slot lowering. The split keeps
//! `resource.rs` focused on the resource-decl + field projection while
//! the constraint mechanics — type-applicability, range invariants,
//! regex shape, combination conflicts, default-vs-constraint —
//! live here.
//!
//! Every fn here is `pub(crate)`; the public re-exports stay in
//! `resource.rs` so the crate's wide `use crate::resource::*` paths
//! continue to resolve unchanged.

use crate::{AnalyzeError, type_ref_from_syntax};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Project `syntax::FieldConstraintsDecl` onto the IR's
/// `ir::FieldConstraints`. Combination + default checks happen
/// separately; closed-catalog validate profiles are checked here.
pub(crate) fn lift_field_constraints(
    field: &str,
    decl: &syntax::FieldConstraintsDecl,
) -> Result<ir::FieldConstraints, AnalyzeError> {
    Ok(ir::FieldConstraints {
        min: decl.min,
        max: decl.max,
        pattern: decl.pattern.clone(),
        between: decl.between,
        length: decl.length,
        r#in: decl.r#in.clone(),
        sanitize_html: match decl.sanitize_html.as_deref() {
            Some(profile) => Some(lower_sanitize_html_profile(field, profile)?),
            None => None,
        },
        utf8_safe: decl.utf8_safe,
        max_recursion: decl.max_recursion,
        max_size: decl.max_size,
        covers_pii: decl.covers_pii.clone(),
    })
}

pub(crate) fn lower_sanitize_html_profile(
    field: &str,
    profile: &str,
) -> Result<ir::SanitizeHtmlProfile, AnalyzeError> {
    match profile {
        "strict" => Ok(ir::SanitizeHtmlProfile::Strict),
        "basic" => Ok(ir::SanitizeHtmlProfile::Basic),
        "markdown_safe" => Ok(ir::SanitizeHtmlProfile::MarkdownSafe),
        other => Err(AnalyzeError::UnknownSanitizeHtmlProfile {
            field: field.to_owned(),
            profile: other.to_owned(),
        }),
    }
}

/// `inline_validator_range_invariant_001` — reject empty numeric
/// ranges at compile time. A `min N max M` pair with N>M produces an
/// uninhabited domain; same for `between A and B` with A>B. The
/// shipped parser stores both bounds as `i64`, so the comparison is
/// total. This check runs after the combination rules so that conflict
/// errors (which already cover redundancy) take precedence.
pub(crate) fn validate_constraint_range_invariant(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    if let (Some(min), Some(max)) = (c.min, c.max) {
        if min > max {
            return Err(AnalyzeError::InlineValidatorRangeInvariant {
                field: field.to_owned(),
                rule: "min>max".to_owned(),
                low: min.to_string(),
                high: max.to_string(),
            });
        }
    }
    if let Some((a, b)) = c.between {
        if a > b {
            return Err(AnalyzeError::InlineValidatorRangeInvariant {
                field: field.to_owned(),
                rule: "between".to_owned(),
                low: a.to_string(),
                high: b.to_string(),
            });
        }
    }
    Ok(())
}

/// `inline_validator_type_mismatch_001` — reject constraint keywords
/// applied to a field whose underlying `BuiltinType` is outside the
/// §10.1 "Applies to" column. The check is intentionally generous on
/// `UserDefined` / `EnumRef` / `Capability` / `Many` / `Unresolved`
/// type refs (we skip them) so the existing `TypeRef::Unresolved`
/// path keeps owning the "this is an unknown name" error class.
///
/// Catalog (mirrors `docs/proposals/lzx-integration-codegen.md §10.1`):
/// - `min` / `max`: Text, Integer, Decimal, semantic string variants
/// - `length`: Text + semantic string variants ONLY
/// - `pattern`: Text + semantic string variants ONLY
/// - `between`: Integer, Decimal ONLY
/// - `in`: Text, Integer, Decimal + semantic string variants
pub(crate) fn validate_constraint_type_compatibility(
    field: &str,
    type_text: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    use ir::{BuiltinType as B, TypeRef};
    // Resolve once; bail out on non-Builtin refs (those classes never
    // carry inline constraints in v0 — and we don't want to false-
    // positive on unresolved names).
    let resolved = type_ref_from_syntax(type_text);
    let builtin = match resolved {
        TypeRef::Builtin(b) => b,
        _ => return Ok(()),
    };

    // Helper closures for the three categories.
    let is_text_like = matches!(
        builtin,
        B::Text
            | B::SemanticEmail
            | B::SemanticPhone
            | B::SemanticUrl
            | B::SemanticUuid
            | B::SemanticCurrency
    ) || matches!(
        &builtin,
        // B3 — a plugin-contributed semantic with a text carrier
        // accepts the same inline constraint families as Text. Wider
        // carriers gated by a separate proposal so they cannot land
        // here yet (loader enforces `carrier_type = "String"` only).
        B::SemanticPluginType { carrier, .. } if matches!(**carrier, B::Text)
    );
    let is_numeric = matches!(builtin, B::Integer | B::Decimal);
    let is_min_max_compatible = is_text_like || is_numeric;
    let is_in_compatible = is_text_like || is_numeric;

    if c.min.is_some() && !is_min_max_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "min".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    if c.max.is_some() && !is_min_max_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "max".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    if c.length.is_some() && !is_text_like {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "length".to_owned(),
            applies_to: "Text".to_owned(),
        });
    }
    if c.pattern.is_some() && !is_text_like {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "pattern".to_owned(),
            applies_to: "Text".to_owned(),
        });
    }
    if c.between.is_some() && !is_numeric {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "between".to_owned(),
            applies_to: "Integer, Decimal".to_owned(),
        });
    }
    if c.r#in.is_some() && !is_in_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "in".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    Ok(())
}

/// `inline_validator_pattern_compile_001` — reject obviously
/// malformed regex patterns at lowering. The analyzer stays regex-
/// free by design (no `regex` crate dep in Cargo.toml — see comment
/// in `validate_default_against_constraints`); we only flag the
/// unambiguous shape errors that the Go/JS regex compilers also
/// reject (unbalanced `(`, unbalanced `[`, trailing `\`). Anything
/// passing this check is still subject to the runtime regex
/// compiler's authoritative judgement.
pub(crate) fn validate_constraint_pattern_compile(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    let Some(pattern) = c.pattern.as_deref() else {
        return Ok(());
    };
    // Trailing unescaped backslash: `^a\` — both RE2 and JS RegExp
    // reject.
    if pattern.ends_with('\\') {
        // Count trailing backslashes; an odd count means the last
        // backslash is unescaped.
        let trailing = pattern.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            return Err(AnalyzeError::InlineValidatorPatternCompile {
                field: field.to_owned(),
                pattern: pattern.to_owned(),
                reason: "trailing unescaped `\\`".to_owned(),
            });
        }
    }
    // Bracket / paren balance check. Walk left-to-right, skipping the
    // character after `\` (escape). Inside a character class `[...]`
    // we still treat `\]` as escaped. We only flag the unambiguous
    // shape errors: paren or bracket counts that go negative or end
    // non-zero.
    let mut paren_depth: i32 = 0;
    let mut in_class = false;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Skip the next char (treat as escaped). If the next
                // char is missing, the trailing-`\` check above already
                // fired.
                chars.next();
            }
            '[' if !in_class => {
                in_class = true;
            }
            ']' if in_class => {
                in_class = false;
            }
            '(' if !in_class => {
                paren_depth += 1;
            }
            ')' if !in_class => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Err(AnalyzeError::InlineValidatorPatternCompile {
                        field: field.to_owned(),
                        pattern: pattern.to_owned(),
                        reason: "unbalanced `)`".to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
    if in_class {
        return Err(AnalyzeError::InlineValidatorPatternCompile {
            field: field.to_owned(),
            pattern: pattern.to_owned(),
            reason: "unbalanced `[`".to_owned(),
        });
    }
    if paren_depth != 0 {
        return Err(AnalyzeError::InlineValidatorPatternCompile {
            field: field.to_owned(),
            pattern: pattern.to_owned(),
            reason: "unbalanced `(`".to_owned(),
        });
    }
    Ok(())
}

/// L0 #3 §10.2 — enforce inline constraint combination rules. Returns
/// the first conflict so authors get one focused diagnostic per field
/// (consistent with the rest of the analyzer).
pub(crate) fn validate_constraint_combinations(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    // length + min/max — `length N` already pins both bounds.
    if c.length.is_some() && c.min.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "length+min".to_owned(),
        });
    }
    if c.length.is_some() && c.max.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "length+max".to_owned(),
        });
    }
    // between + min/max — redundant.
    if c.between.is_some() && c.min.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "between+min".to_owned(),
        });
    }
    if c.between.is_some() && c.max.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "between+max".to_owned(),
        });
    }
    // in [...] + pattern — use enum instead.
    if c.r#in.is_some() && c.pattern.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "in+pattern".to_owned(),
        });
    }
    Ok(())
}

/// L0 #3 §10.3 — verify that a default literal satisfies the declared
/// inline constraints. The parser captures `default` verbatim (incl.
/// surrounding quotes for string literals); we strip the outer quotes
/// before length/pattern/in checks. Numeric checks parse the literal
/// as `i64`; non-integer literals fall back to a no-op rather than
/// raise a different error code, because the analyzer already has a
/// type-mismatch check elsewhere.
pub(crate) fn validate_default_against_constraints(
    field: &str,
    default_raw: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    let default_raw = default_raw.trim();
    // Strip surrounding double quotes for string-typed defaults.
    let unquoted =
        if default_raw.len() >= 2 && default_raw.starts_with('"') && default_raw.ends_with('"') {
            &default_raw[1..default_raw.len() - 1]
        } else {
            default_raw
        };
    // Numeric path: try parsing the (unquoted) literal as an integer.
    let as_int = unquoted.parse::<i64>().ok();
    // length check (string only — applies to char count of the
    // unquoted literal).
    if let Some(n) = c.length {
        if unquoted.chars().count() != n {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("length={}", n),
            });
        }
    }
    // min on numerics OR text length.
    if let Some(min) = c.min {
        if let Some(n) = as_int {
            if n < min {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("min={}", min),
                });
            }
        } else {
            // text-min checks character count.
            let len = unquoted.chars().count() as i64;
            if len < min {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("min={}", min),
                });
            }
        }
    }
    if let Some(max) = c.max {
        if let Some(n) = as_int {
            if n > max {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("max={}", max),
                });
            }
        } else {
            let len = unquoted.chars().count() as i64;
            if len > max {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("max={}", max),
                });
            }
        }
    }
    if let Some((lo, hi)) = c.between {
        if let Some(n) = as_int {
            if n < lo || n > hi {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("between={}..{}", lo, hi),
                });
            }
        }
    }
    if let Some(values) = &c.r#in {
        // For text: compare unquoted string against the list verbatim.
        // For numerics: also compare unquoted, since `in [1,2,3]` is
        // stored as `["1", "2", "3"]` in the AST.
        if !values.iter().any(|v| v == unquoted) {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("in=[{}]", values.join(", ")),
            });
        }
    }
    if let Some(pattern) = &c.pattern {
        // We do NOT compile the regex here (Lazuli analyzer is regex-
        // free by design — RE2 enforcement lives in doctor + runtime).
        // For empty defaults the parser fails on the bare `""` anyway,
        // but we explicitly catch them so they don't silently pass.
        if unquoted.is_empty() && !pattern.is_empty() {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("pattern=\"{}\"", pattern),
            });
        }
    }
    Ok(())
}
