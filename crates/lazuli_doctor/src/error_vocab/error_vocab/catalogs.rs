//! Closed catalogs shared by the IR Error-Vocab rules.
//!
//! Kept in sync with the runtime (`runtime/go/lazuli/error.go`) and the
//! IR comments on `FeatureErrors.exposure_4xx` / `exposure_5xx`. Each
//! rule file imports from here so the canonical lists exist exactly once.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §2.B / §2.C.

use lazuli_ir::FeatureErrors;

/// Closed catalog of 12 framework error codes that may be overridden via
/// `errors <code> message @translation.<key>` (proposal §2.B).
///
/// DB-INTEGRITY-CATALOG-EXT (2026-05-19): extended with the 4 db-integrity
/// codes emitted by the runtime's `classifyDBError` helper. These map
/// Postgres class-23 SQLSTATEs (`23505`, `23503`, `23502`, `23514`) to
/// stable wire codes so authors can author `errors unique_violation
/// message @translation.<key>` overrides without leaking SQL internals.
pub const FRAMEWORK_ERROR_CODES: &[&str] = &[
    "policy_denied",
    "validation_failed",
    "tenant_mismatch",
    "not_found",
    "rate_limited",
    "bad_request",
    "method_not_allowed",
    "integration_error",
    "unique_violation",
    "foreign_key_violation",
    "not_null_violation",
    "check_violation",
];

/// Closed catalog of `expose client 4xx <field>` tokens (proposal §2.C).
pub const EXPOSE_4XX_FIELDS: &[&str] = &["message", "code", "data", "message_key"];

/// Closed catalog of `expose client 5xx <field>` tokens (proposal §2.C).
/// `message` is deliberately excluded — see ERR-VOCAB-EXPOSE-5XX-MESSAGE.
pub const EXPOSE_5XX_FIELDS: &[&str] = &["code", "data"];

/// Shared helper: does the feature carry a `errors policy_denied message
/// @translation.<key>` catch-all? Used by ERR-VOCAB-001 and ERR-VOCAB-003.
pub(super) fn has_policy_denied_catchall(errors: Option<&FeatureErrors>) -> bool {
    let Some(errors) = errors else { return false };
    errors.messages.iter().any(|m| m.code == "policy_denied")
}
