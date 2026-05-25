//! Resource declaration lowering — the "schema" slot of a feature.
//!
//! ## Role in the pipeline
//!
//! This module owns the projection from `syntax::ResourceDecl` onto
//! `ir::Resource`. A resource is the canonical persisted shape of a
//! domain noun (`Customer`, `Order`, `Photo`) — fields, constraints,
//! tenancy axis, soft-delete flag, timestamps, retention, lifecycle
//! routes, invariants, lock + composite-key strategy, owner-scope
//! conventions.
//!
//! Field-level lowering peels three layers in order:
//!
//! 1. `extract_field_level_pii_decorator` — if the field's `type_text`
//!    carries a tail `@cap.PII(...)`, lift it to `Field.pii` and clean
//!    the surface text. Leading `@cap.PII(...)` (the field's only
//!    carrier) stays in `TypeRef`.
//! 2. `peel_trailing_field_modifiers` — recover `required|optional|unique`
//!    suffix tokens that the syntax parser leaves attached when a
//!    decorator was peeled in step 1.
//! 3. `lift_field_constraints` + the four `validate_constraint_*`
//!    gates — project the inline-validator surface (`min`, `max`,
//!    `length`, `pattern`, `between`, `in`, `sanitize_html`, `utf8_safe`,
//!    `max_recursion`, `max_size`, `covers_pii`) and reject empty
//!    domains, type mismatches, malformed regex shapes, and conflicting
//!    combinations at lower-time. Default-literal compatibility runs
//!    last (§10.3).
//!
//! Inline rate-limit literals (`rate_limit "60/min"` plus the
//! env-qualified `by_env` form) also land here because they ride
//! alongside resource conventions in the parser's surface area, even
//! though the IR they project to (`ir::RateLimitSpec`) is consumed by
//! `command` and `agent` lowering too.
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::ResourceDecl`,
//!   `ResourceFieldDecl`, `ResourceConstraintAst`, `RateLimitSpecAst`.
//! * Output: `lazuli_ir::Resource`, `Field`, `Constraint`,
//!   `FieldConstraints`, `RateLimitSpec`.
//! * Diagnostics: `inline_validator_*`, `constraint_conflict_*`,
//!   `default_violates_constraint`, `owner_axis_on_non_fk`,
//!   `unknown_sanitize_html_profile`. All raised through
//!   `AnalyzeError` so doctor can fan out per code.
//!
//! ## ABI guarantee
//!
//! Every fn here is `pub(crate)` — internal to the analyzer. Nothing
//! external (codegen, doctor, LSP) calls a resource fn directly; they
//! all read the lowered `ir::Resource` through `lower_feature_skeleton`.

use crate::helpers::{find_balanced_decorator_end, span_of};
use crate::{
    AnalyzeError, lower_invariant_decl, lower_public_contract, parse_cap_pii_type, parse_default,
    type_ref_from_syntax,
};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L Tier 4c — lower a canonical-indent `resource` block into
/// `ir::Resource`. `tenancy` (resource-local override), `soft_delete`,
/// `timestamps`, `retention`, `validates`, and `derived_from` all
/// project through additive IR fields landed alongside this lowering.
pub(crate) fn lower_resource_decl(r: &syntax::ResourceDecl) -> Result<ir::Resource, AnalyzeError> {
    let tenancy = r.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(axis) => ir::Tenancy::Custom(axis.clone()),
    });
    let fields = r
        .fields
        .iter()
        .map(lower_resource_field)
        .collect::<Result<Vec<_>, _>>()?;
    let retention = r.retention.as_ref().map(|ret| ir::RetentionSpec {
        duration: ret.duration.clone(),
        action: match ret.action {
            syntax::ResourceRetentionAction::Anonymize => ir::RetentionAction::Anonymize,
            syntax::ResourceRetentionAction::Delete => ir::RetentionAction::Delete,
            syntax::ResourceRetentionAction::Archive => ir::RetentionAction::Archive,
        },
    });
    // `validates @validator.tier_check` collapses onto `Resource.validate`
    // for a single-entry case (the fixture pattern). Multi-entry would
    // need a `Vec`; defer until pilot evidence demands it.
    let validate = r.validates.first().map(|v| ir::PathRef::authored(v));
    // CL.C.4 — lower resource-scoped `invariant <name>` blocks.
    let invariants = r
        .invariants
        .iter()
        .map(lower_invariant_decl)
        .collect::<Vec<_>>();
    // Roadmap §1.5 (CL.C.2) — lower `lock` decorator into typed IR.
    let lock = r.lock.as_ref().map(|spec| match spec {
        syntax::ResourceLock::Optimistic { version_field } => ir::LockSpec::Optimistic {
            version_field: version_field.clone(),
        },
        syntax::ResourceLock::Pessimistic => ir::LockSpec::Pessimistic,
        syntax::ResourceLock::RowLevel => ir::LockSpec::RowLevel,
    });
    // Roadmap §1.5 (CL.C.2) — lower `composite_key` block into typed IR.
    let composite_key = r.composite_key.as_ref().map(|ck| ir::CompositeKey {
        fields: ck.fields.clone(),
        primary: ck.primary,
    });
    let conventions = r
        .conventions
        .iter()
        .map(|c| match c {
            syntax::ResourceConventionAst::Crud => ir::ConventionRef::Crud,
            syntax::ResourceConventionAst::Me => ir::ConventionRef::Me,
        })
        .collect();
    let constraints = r
        .constraints
        .iter()
        .map(lower_resource_constraint)
        .collect();
    let lifecycle_routes = r.lifecycle_routes.as_ref().map(|lr| ir::LifecycleRoutes {
        arms: lr
            .arms
            .iter()
            .map(|arm| ir::LifecycleRouteArm {
                state: arm.state.clone(),
                url: arm.url.clone(),
            })
            .collect(),
        span_ref: Some(span_of(lr.span)),
    });
    Ok(ir::Resource {
        name: r.name.clone(),
        public_contract: lower_public_contract(&r.public_contract),
        tenancy,
        soft_delete: r.soft_delete,
        timestamps: if r.timestamps { Some(true) } else { None },
        fields,
        constraints,
        validate,
        validates: Vec::new(),
        retention,
        previous_names: r
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        span_ref: Some(span_of(r.span)),
        lifecycle: None,
        invariants,
        lock,
        composite_key,
        conventions,
        lifecycle_routes,
    })
}

pub(crate) fn lower_resource_constraint(
    constraint: &syntax::ResourceConstraintAst,
) -> ir::Constraint {
    match constraint {
        syntax::ResourceConstraintAst::Unique(unique) => {
            ir::Constraint::Unique(ir::UniqueConstraint {
                fields: unique.fields.clone(),
                per: None,
            })
        }
        syntax::ResourceConstraintAst::Index(index) => ir::Constraint::Index(ir::IndexConstraint {
            fields: index.fields.clone(),
            method: index.method.map(|method| match method {
                syntax::ResourceIndexMethodAst::Btree => ir::IndexMethod::Btree,
                syntax::ResourceIndexMethodAst::Gin => ir::IndexMethod::Gin,
                syntax::ResourceIndexMethodAst::Gist => ir::IndexMethod::Gist,
            }),
            full_text: index.full_text,
        }),
    }
}

/// Migrations bucket cycle Route C — strip the `migrated`/`alias` mode
/// prefix from a parsed `previously` line. `previously migrated Foo`
/// keeps `Foo` in IR; `previously alias Foo` ditto. Doctor compares
/// against current symbol names, so the mode keyword is noise here.
fn strip_previously_mode(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("migrated ") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = trimmed.strip_prefix("alias ") {
        return rest.trim().to_owned();
    }
    trimmed.to_owned()
}

pub(crate) fn lower_resource_field(
    f: &syntax::ResourceFieldDecl,
) -> Result<ir::Field, AnalyzeError> {
    let (type_text_with_recovered_modifiers, type_pii) =
        extract_field_level_pii_decorator(&f.type_text);
    let recovered = peel_trailing_field_modifiers(&type_text_with_recovered_modifiers);
    let (default_text, default_pii) = match f.default.as_deref() {
        Some(raw) => {
            let (cleaned, pii) = extract_field_level_pii_decorator(raw);
            (Some(cleaned), pii)
        }
        None => (None, None),
    };
    let pii = type_pii.or(default_pii);
    let default = default_text.as_deref().map(|raw| parse_default(raw.trim()));
    let constraints = lift_field_constraints(&f.name, &f.constraints)?;
    // L0 #3 §10.2 + §10.3 — combination rules + default compatibility.
    validate_constraint_combinations(&f.name, &f.constraints)?;
    // Wave-B-CL4 — three follow-up diagnostics for the inline-validator
    // surface: range invariants (`min>max`, `between A>B`), per-type
    // applicability (§10.1), and structural regex sanity. Combination
    // conflicts run first so `length+min` etc. take precedence over
    // the per-constraint type / range checks.
    validate_constraint_range_invariant(&f.name, &f.constraints)?;
    validate_constraint_type_compatibility(&f.name, &recovered.type_text, &f.constraints)?;
    validate_constraint_pattern_compile(&f.name, &f.constraints)?;
    if let Some(default_text) = default_text.as_deref() {
        validate_default_against_constraints(&f.name, default_text.trim(), &f.constraints)?;
    }
    let type_ref = type_ref_from_syntax(&recovered.type_text);
    // `ir-resource-conventions-owner-scope` §11.1 — `owner_axis_on_non_fk`.
    // The annotation is only meaningful on FK fields (`UserDefined`
    // resources). Primitives, builtins, and capability-typed fields
    // can't carry an ownership chain; reject at lowering so the synth
    // pass (O2) doesn't have to guard against malformed IR.
    let owner_axis = match f.owner_axis.as_ref() {
        Some(axis) => {
            if !matches!(type_ref, ir::TypeRef::UserDefined(_)) {
                return Err(AnalyzeError::OwnerAxisOnNonFk {
                    field: f.name.clone(),
                    type_text: recovered.type_text.clone(),
                });
            }
            Some(ir::OwnerAxis {
                through_column: axis.through_column.clone(),
            })
        }
        None => None,
    };
    Ok(ir::Field {
        name: f.name.clone(),
        // Phase L Tier 4 follow-up — use `type_ref_from_syntax` so
        // `@cap.Hashed(algorithm:…)`, `@cap.Encrypted(key:…)`,
        // `@cap.Token(…)`, and `@semantic.*` lift into typed variants.
        // The legacy `type_ref_from_text` path is preserved for
        // call sites that pass cleaned-up identifiers only.
        type_ref,
        required: f.required || recovered.required,
        unique: f.unique || recovered.unique,
        // CL.C.4 — lift `@slug` decorator presence into the typed IR.
        slug: f.slug,
        default,
        derived_from: f.derived_from.clone(),
        constraints,
        // Roadmap §1.5 (CL.C.2) — `@full_text` decorator captured by
        // the parser as a flag on the field declaration; threaded
        // through to the IR so DDL emission can attach a GIN tsvector
        // index per marked field.
        full_text: f.full_text,
        previous_names: f
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        pii,
        owner_axis,
        span_ref: Some(span_of(f.span)),
    })
}

struct RecoveredFieldType {
    type_text: String,
    required: bool,
    unique: bool,
}

/// FR-PII-STACK — when `@cap.PII(...)` is authored after field modifiers,
/// the syntax parser leaves `required|optional|unique` inside `type_text`
/// because it only peels final bare tokens. Recover those modifiers after
/// removing the field-level decorator so the existing parser remains stable.
fn peel_trailing_field_modifiers(text: &str) -> RecoveredFieldType {
    let mut head = text.trim().to_owned();
    let mut required = false;
    let mut unique = false;
    loop {
        let trimmed = head.trim_end();
        if trimmed.ends_with(" required") {
            required = true;
            head = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            head = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else if trimmed.ends_with(" unique") {
            unique = true;
            head = trimmed[..trimmed.len() - " unique".len()].to_owned();
        } else {
            head = trimmed.to_owned();
            break;
        }
    }
    RecoveredFieldType {
        type_text: head,
        required,
        unique,
    }
}

/// FR-PII-STACK — peel a non-leading `@cap.PII(...)` marker out of a
/// resource/record field's type tail and lower it into `Field.pii`. Leading
/// `@cap.PII(...)` remains a normal capability `TypeRef` for fields whose
/// only carrier is PII.
fn extract_field_level_pii_decorator(type_text: &str) -> (String, Option<ir::PiiCapability>) {
    let original = type_text.trim().to_owned();
    let Some((start, end)) = find_field_level_cap_pii_span(type_text) else {
        return (original, None);
    };
    let before = type_text[..start].trim_end();
    if before.is_empty() {
        return (original, None);
    }
    let token = &type_text[start..end];
    let Some(pii) = parse_cap_pii_type(token) else {
        return (original, None);
    };
    let after = type_text[end..].trim_start();
    let mut cleaned = before.to_owned();
    if !cleaned.is_empty() && !after.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(after);
    (cleaned.trim().to_owned(), Some(pii))
}

fn find_field_level_cap_pii_span(text: &str) -> Option<(usize, usize)> {
    const PREFIX: &[u8] = b"@cap.PII(";
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;
    while i + PREFIX.len() <= bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if depth == 0 && &bytes[i..i + PREFIX.len()] == PREFIX {
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            if before_ok {
                if let Some(end) = find_balanced_decorator_end(text, i) {
                    return Some((i, end));
                }
            }
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

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
fn validate_default_against_constraints(
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

/// `ir-rate-limit-env-aware` cell 1 — lower a parser `RateLimitSpecAst`
/// into the IR `RateLimitSpec`.
///
/// The single-line back-compat case (`rate_limit "X"`) lands here with
/// `default = Some("X")` and `by_env = []`. The new env-qualified case
/// folds each `RateLimitByEnvAst` into a `RateLimitByEnv` with closed-
/// catalog `EnvName`s separated from unknown identifiers; the unknown
/// bucket is what Cell 3 doctor will surface as `rate_limit_unknown_env`.
/// The proposal-defined `"unlimited"` keyword (§4.4) lowers to the
/// empty-string sentinel for both the default and per-env slots.
///
/// When the source authored only env-qualified lines (no unqualified
/// default), the IR default becomes the empty string — same sentinel
/// as `"unlimited"`. Cell 3 doctor surfaces
/// `rate_limit_no_default_with_qualifications` so the silent default
/// is visible at lint time, per proposal §9.2.
pub(crate) fn lower_rate_limit_spec(spec: &syntax::RateLimitSpecAst) -> ir::RateLimitSpec {
    let default = match spec.default.as_deref() {
        Some(literal) => lower_rate_limit_literal(literal),
        None => String::new(),
    };
    let by_env = spec
        .by_env
        .iter()
        .map(|entry| {
            let mut known = Vec::with_capacity(entry.envs.len());
            let mut unknown = Vec::new();
            for raw in &entry.envs {
                if let Some(env) = ir::EnvName::from_ident(raw) {
                    known.push(env);
                } else {
                    unknown.push(raw.clone());
                }
            }
            ir::RateLimitByEnv {
                envs: known,
                unknown_envs: unknown,
                limit: lower_rate_limit_literal(&entry.limit),
                span_ref: None,
            }
        })
        .collect();
    ir::RateLimitSpec {
        default,
        by_env,
        span_ref: None,
    }
}

/// `ir-rate-limit-env-aware` cell 1 — lower a single literal into the
/// canonical IR form, applying the `"unlimited"` keyword carve-out
/// (proposal §4.4). The keyword is recognised verbatim (case-sensitive)
/// and lowers to the empty string; everything else passes through
/// unchanged.
pub(crate) fn lower_rate_limit_literal(literal: &str) -> String {
    if literal == "unlimited" {
        String::new()
    } else {
        literal.to_owned()
    }
}

/// Test-only `validate <profile>` line lowering. Used by `lib.rs::tests`
/// to exercise the lift path without spinning up a full resource AST.
#[cfg(test)]
pub(crate) fn lower_validate_line(line: &str) -> Result<ir::FieldConstraints, AnalyzeError> {
    let trimmed = line.trim();
    let mut decl = syntax::FieldConstraintsDecl::default();
    if let Some(rest) = trimmed.strip_prefix("validate sanitize_html(") {
        let profile = rest.trim_end_matches(')').trim();
        decl.sanitize_html = Some(profile.to_owned());
    } else if trimmed == "validate utf8_safe" {
        decl.utf8_safe = Some(true);
    } else if let Some(rest) = trimmed.strip_prefix("validate max_recursion:") {
        decl.max_recursion = rest.trim().parse::<u32>().ok();
    } else if let Some(rest) = trimmed.strip_prefix("validate max_size:") {
        decl.max_size = rest.trim().parse::<u64>().ok();
    } else if trimmed == "validator covers_pii" {
        decl.covers_pii = Some("covers_pii".to_owned());
    } else {
        return Ok(ir::FieldConstraints::default());
    };
    lift_field_constraints("validate", &decl)
}
