//! Report / storage / query.view aggregator — three loosely-coupled
//! diagnostic families that share the same fact-bundle plumbing:
//!
//! 1. **`report_diagnostics`** — REPORT-* rule fan-out per
//!    `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP. Builds a
//!    synthetic `ir::Feature` view from each `Tier3FeatureFacts` row
//!    (`make_synthetic_feature_for_reports`) and dispatches the closed
//!    REPORT-* catalog: column shape, signed-URL TTL, source kind,
//!    rate-limit gating, path collisions, signed-no-storage,
//!    storage-ambiguous, format-unknown.
//!
//! 2. **`cap_file_storage_diagnostics`** — five `@cap.File(...)`
//!    checks against the lifted file-capability facts:
//!    `cap_file_visibility_undeclared`,
//!    `cap_file_visibility_signed_ttl_mismatch`,
//!    `cap_file_mime_family_unknown`,
//!    `cap_file_size_unit_invalid`,
//!    `cap_file_accept_input_output_mismatch`.
//!
//! 3. **`query_view_sql_file_diagnostics`** — Wave B.4 `query.view`
//!    follow-up. Resolves each `@file.<name>.sql` path against the
//!    project root, requires the file to exist
//!    (`QUERY-VIEW-SQL-FILE-001`), and runs a best-effort unsafe-SQL
//!    scan (`QUERY-VIEW-SQL-UNSAFE-001`).
//!
//! Extracted from `doctor/mod.rs` in rails-style R6-2 and re-split in
//! R9 into the three sibling files below.

mod cap_file;
mod query_view;
mod report;

pub(crate) use cap_file::cap_file_storage_diagnostics;
pub(crate) use query_view::query_view_sql_file_diagnostics;
pub(crate) use report::{make_synthetic_feature_for_reports, report_diagnostics};
