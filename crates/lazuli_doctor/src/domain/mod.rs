//! Doctor rules for the CL.C.4 domain-model vocabulary (roadmap §1.7).
//!
//! Four diagnostics ship alongside the new `aggregate` / `invariant` /
//! `@slug` primitives:
//!
//! - `aggregate-root-unknown` — aggregate's `root` references a
//!   resource that doesn't exist in the same feature.
//! - `aggregate-contains-unknown` — aggregate's `contains` lists an
//!   unknown resource.
//! - `invariant-predicate-invalid` — invariant `when <pred>` references
//!   a field the resource (or aggregate's root) doesn't declare.
//! - `slug-uniqueness-implicit` — `@slug` field that did not also
//!   declare `unique`. The rule auto-suggests adding `unique` and
//!   surfaces a warning (the IR slot is the canonical signal; we keep
//!   `Field.unique` untouched so codegen still sees the author intent).
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is wired in `doctor.rs`. Each
//! rule's inline `#[cfg(test)] mod tests` exercises the logic.
//!
//! Reference: spec wave-c-cl4 + Rule Zero (vocabulary over mechanism).

pub mod aggregate_contains_unknown;
pub mod aggregate_root_unknown;
pub mod computed_date_expr_invalid;
pub mod constraint_unique_when_invalid;
pub mod invariant_predicate_invalid;
pub mod reorder_position_field_invalid;
pub mod resource_append_only_invalid;
pub mod schedule_rule_invalid;
pub mod slug_uniqueness_implicit;
