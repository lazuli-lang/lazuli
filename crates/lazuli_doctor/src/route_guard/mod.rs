//! Route-guard doctor rules — `ROUTE-GUARD-*` and `ROUTE-LIFECYCLE-*`
//! diagnostic codes for the escape-hatch surface added by
//! `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md` §4.3.
//!
//! ## Codes shipped in this module
//!
//! - `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001` — `requires_lifecycle`
//!   AND `requires_lifecycle_in` on the same guard → error.
//! - `ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002` — empty allow-list →
//!   error (unreachable view).
//! - `ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003` — allow-list state not
//!   declared on the resource's `Lifecycle` → error.
//! - `ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004` — feature has no
//!   `lookup_my_<resource>` query → error.
//! - `ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005` — field doesn't exist on
//!   the resource → error.
//! - `ROUTE-GUARD-FIELD-TYPE-MISMATCH-006` — expected literal type
//!   doesn't match the field's IR type → error.
//! - `ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007` —
//!   `forbid_when ... only_when lifecycle Host = ...` but the route
//!   targets Traveler → warn.
//! - `ROUTE-LIFECYCLE-CANONICAL-FORM-001` — both shorthand AND
//!   allow-list forms used in the same project; advises canonical
//!   form per §6.5.
//! - `ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001` — Shape-C gate
//!   without a paired server-side enforcement (SQL trigger or
//!   `policy` companion); warn at strict, error at production.
//!
//! Each rule is a sub-module exposing `Finding` + `pub const CODE` +
//! `check(...)`. Inline `#[cfg(test)] mod tests` exercises ≥3 cases
//! per rule (happy path / fires / opt-out or alternate path).
//!
//! Per `ir-route-guard-escape-hatch-2026-05-28.md` §4.3.

pub mod field_missing_server_pair_001;
pub mod field_type_mismatch_006;
pub mod field_unknown_feature_004;
pub mod field_unknown_field_005;
pub mod forbid_only_when_resource_mismatch_007;
pub mod lifecycle_canonical_form_001;
pub mod lifecycle_exclusive_001;
pub mod lifecycle_in_empty_002;
pub mod lifecycle_in_unknown_003;
