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

// Rails-style R9 — constraint validators + rate-limit lowering moved
// to sibling modules; re-export so `crate::resource::<sym>` paths used
// across the analyzer continue to resolve unchanged.
pub(crate) use crate::resource_rate_limit::lower_rate_limit_spec;
pub(crate) use crate::resource_validators::{
    lift_field_constraints, validate_constraint_combinations, validate_constraint_pattern_compile,
    validate_constraint_range_invariant, validate_constraint_type_compatibility,
};
use crate::resource_validators::validate_default_against_constraints;

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
    // GAP-13 — lower `polymorphic_ref` declarations into typed IR.
    let polymorphic_refs = r
        .polymorphic_refs
        .iter()
        .map(|pr| ir::PolymorphicRef {
            type_field: pr.type_field.clone(),
            id_field: pr.id_field.clone(),
            targets: pr.targets.clone(),
        })
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
        polymorphic_refs,
        // GAP-AUDIT-02 — insert-only marker. Doctor RESOURCE-APPEND-ONLY-001
        // rejects commands that update/delete this resource.
        append_only: r.append_only,
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
                // GAP-NEW-001 — run the verbatim `when` text through the
                // shared closed-predicate parser (same path as
                // `invariant when`); unrecognized shapes land in
                // `EvalPredicate::Unparsed` so doctor can echo the source.
                when: unique
                    .when
                    .as_deref()
                    .map(crate::agent::parse_closed_predicate),
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
    // GAP-12 — project the `target @feature.<feature>.<Resource>`
    // cross-feature FK annotation onto the IR. The reference is logical;
    // doctor (`REF-CROSS-FEATURE-UNKNOWN-001`) validates the feature is in
    // the declaring feature's `uses` and the resource exists there.
    let cross_feature_target = f
        .cross_feature_target
        .as_ref()
        .map(|t| ir::CrossFeatureTarget {
            feature: t.feature.clone(),
            resource: t.resource.clone(),
        });
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
        // W3 GAP-03 — project the typed `computed_date from <base> offset
        // <offset>` field kind onto the IR. The references are validated by
        // doctor (`COMPUTED-DATE-EXPR-001`): `base` must be a declared
        // `Date` field and `offset` an `Integer` field or integer literal.
        computed_date: f.computed_date.as_ref().map(lower_computed_date),
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
        cross_feature_target,
        span_ref: Some(span_of(f.span)),
    })
}

/// W3 GAP-03 — lower the AST `computed_date from <base> offset <offset>`
/// payload onto the typed IR node. Pure projection: reference validity
/// (base is a `Date` field, offset is an `Integer` field or literal) is
/// enforced by doctor `COMPUTED-DATE-EXPR-001`, not here, mirroring how
/// `derived from` keeps its expression verbatim and defers field-resolution
/// to doctor.
fn lower_computed_date(ast: &syntax::ComputedDateAst) -> ir::ComputedDate {
    let offset = match &ast.offset {
        syntax::ComputedDateOffsetAst::Field(name) => ir::ComputedDateOffset::Field(name.clone()),
        syntax::ComputedDateOffsetAst::Literal(n) => ir::ComputedDateOffset::Literal(*n),
    };
    // W4 GAP-08 — project the base selector: a bare `Date` field (W3) or a
    // rule-enum + `@fn` (W4 `schedule_rule`). Reference validity (the `@fn`
    // is a declared binding fn, the rule arg references a field) is enforced
    // by doctor `SCHEDULE-RULE-001`, not here.
    let base = match &ast.base {
        syntax::ComputedDateBaseAst::Field(name) => ir::ComputedDateBase::Field(name.clone()),
        syntax::ComputedDateBaseAst::Rule { rule, fn_ref } => ir::ComputedDateBase::Rule {
            rule: rule.clone(),
            fn_ref: fn_ref.clone(),
        },
    };
    ir::ComputedDate { base, offset }
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
