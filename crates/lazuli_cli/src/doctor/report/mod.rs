//! Doctor rules for the `report <name>` vocab. One module per
//! diagnostic code, mirroring the `correctness/` / `vocab/` layout.
//!
//! See `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP.
//!
//! Codes:
//! - REPORT-COLUMN-MISMATCH-001        — column refs `row.X` but `X` not in source
//! - REPORT-SIGNED-NO-STORAGE-001      — signed report without `object_storage` capability
//! - REPORT-SIGNED-TTL-MISSING-001     — signed report missing `signed_ttl`
//! - REPORT-SIGNED-TTL-FORBIDDEN-001   — `signed_ttl` with `public` / `private` visibility
//! - REPORT-FORMAT-UNKNOWN-001         — `formats` token outside `{csv, xlsx}`
//! - REPORT-FILENAME-TOKEN-UNKNOWN-001 — filename pattern uses unknown `{...}`
//! - REPORT-COLUMNS-EMPTY-001          — `columns` block has zero rows
//! - REPORT-SOURCE-KIND-001            — `source` resolves to `query.lookup`
//! - REPORT-STORAGE-AMBIGUOUS-001      — implicit storage with ≠ 1 cap available
//! - REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001 — `@scope.public` policy without rate_limit
//! - REPORT-PATH-COLLISION-001         — auto-mounted report route collides

pub mod report_column_mismatch_001;
pub mod report_columns_empty_001;
pub mod report_filename_token_unknown_001;
pub mod report_format_unknown_001;
pub mod report_path_collision_001;
pub mod report_policy_public_no_rate_limit_001;
pub mod report_signed_no_storage_001;
pub mod report_signed_ttl_forbidden_001;
pub mod report_signed_ttl_missing_001;
pub mod report_source_kind_001;
pub mod report_storage_ambiguous_001;
