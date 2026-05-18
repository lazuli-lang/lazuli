//! IR Error-Vocab — analyzer cross-checks for the typed error-message
//! resolution chain (Cell ANALYZE-1).
//!
//! Each rule lives in `error_vocab.rs` as a sibling `check_*` function so
//! the doctor pipeline can call them independently and the LSP can mount
//! the same checks for live diagnostics in a later cell. The dispatcher
//! that adapts findings to `DoctorDiagnostic` lives in
//! `lazuli_cli::doctor::error_vocab_diagnostics`.
//!
//! Catalog (see `docs/proposals/ir-error-messages-vocab.md` §6):
//!
//! | Code                              | Severity | Trigger                                                                              |
//! |-----------------------------------|----------|--------------------------------------------------------------------------------------|
//! | `ERR-VOCAB-001`                   | warning  | feature has `policies` but no `when_denied` and no `errors policy_denied` catch-all |
//! | `ERR-VOCAB-002`                   | error    | `@translation.<key>` does not resolve in feature (incl. cross-feature `uses`)        |
//! | `ERR-VOCAB-003`                   | warning  | per-command policy without override and no feature catch-all                         |
//! | `ERR-VOCAB-CODE-UNKNOWN`          | error    | `errors <code>` outside the closed catalog of 8 codes                                |
//! | `ERR-VOCAB-EXPOSE-UNKNOWN`        | error    | unknown field in `expose client 4xx|5xx`                                             |
//! | `ERR-VOCAB-WHEN-DENIED-NO-POLICY` | error    | `when_denied` on command with no `policy`                                            |
//! | `ERR-VOCAB-EXPOSE-5XX-MESSAGE`    | error    | `message` listed in `expose client 5xx`                                              |
//!
//! Closed catalogs (proposal §2.B / §2.C):
//! * Error codes (8): `policy_denied`, `validation_failed`, `tenant_mismatch`,
//!   `not_found`, `rate_limited`, `bad_request`, `method_not_allowed`,
//!   `integration_error`.
//! * 4xx exposure fields: `message`, `code`, `data`, `message_key`.
//! * 5xx exposure fields: `code`, `data`. (`message` is rejected — see
//!   ERR-VOCAB-EXPOSE-5XX-MESSAGE).
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 §11 Cell ANALYZE-1.

pub mod error_vocab;
