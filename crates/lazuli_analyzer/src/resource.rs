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

include!("resource_p1.rs");
include!("resource_p2.rs");
