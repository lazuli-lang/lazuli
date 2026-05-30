//! `HANDLER-SQL-COLUMN-DRIFT-001` — hand-written SQL inside a Go
//! handler omits a NOT NULL column declared by the codegen-emitted
//! resource struct.
//!
//! Severity: `error` (strict / production profile), `warning`
//! (prototype profile). Fires when an `INSERT INTO "<table>" (...)`
//! statement in handler Go code omits a column the resource struct
//! declares as NOT NULL.
//!
//! Sibling to:
//! - [`crate::correctness::handler_missing_001`] — same handler walker
//!   posture; HANDLER-MISSING-001 confirms the file is on disk, this
//!   rule walks its source.
//! - [`crate::correctness::schema_migration_present`] — established
//!   the pattern of "read codegen artifact + parse SQL + diff
//!   columns + emit finding". That rule diffs IR↔migration; this rule
//!   diffs handler-SQL↔resource-struct.
//!
//! Pilot incident (2026-05-27): a hand-written
//! `INSERT INTO "user" (..., created_at)` in
//! `app/features/account/handlers/register_with_google.go` shipped to
//! production missing `updated_at`. The codegen-emitted `User` struct
//! in `dist/go/account/resource.gen.go:92-108` declares
//! `UpdatedAt lazuli.Time \`db:"updated_at"\`` and the migration
//! declares `updated_at TIMESTAMPTZ NOT NULL`. Three of four
//! surfaces agreed; the handler — written by a human under pressure —
//! drifted. The INSERT failed with a NOT NULL violation on every
//! Google OAuth registration. `lazuli doctor` was 0/0/0/0.
//!
//! ## Heuristic (v0.1, deliberately narrow)
//!
//! 1. Walk every handler `.go` file via the existing
//!    [`crate::error_handling::walker::walk_workspace_go_handlers`].
//!    Skip `*_test.go` (test fixtures use arbitrary SQL shapes).
//! 2. For each handler, scan for SQL inside Go *raw string literals*
//!    delimited by backticks. Raw strings are the authoring
//!    convention; double-quoted strings can't span lines and rarely
//!    hold full INSERT/UPDATE statements.
//! 3. Inside each raw string, look for `INSERT INTO "<ident>" (...)`
//!    / `INSERT INTO <ident> (...)` headers. Extract the
//!    parenthesised column list.
//! 4. Resolve `<ident>` against the column catalog built from
//!    `dist/go/<feature>/resource.gen.go` files. Each resource struct
//!    in those files carries `db:"<col>"` tags; pointer-typed fields
//!    (`*string`, `*Role`, etc.) are nullable, value-typed fields are
//!    NOT NULL.
//! 5. Fire when the INSERT column list omits a NOT NULL column from
//!    the catalog. Skip silently when:
//!    - The table doesn't resolve to a known resource (handler talks
//!      to a non-Lazuli table).
//!    - No `resource.gen.go` files were found (codegen not yet run).
//!    - The author wrote `// doctor:allow HANDLER-SQL-COLUMN-DRIFT-001`
//!      on the line above the offending statement.
//!
//! ## What v0.1 does NOT do
//!
//! - Parse string-concatenated SQL (`"INSERT INTO " + t + ...`). The
//!   walker only sees one fragment; concatenation is out of scope.
//!   The spec defers this to `VOCAB-SECURITY-SQL-CONCAT-001`. The
//!   rule emits a separate [`SqlUnreadable`](FindingKind::SqlUnreadable)
//!   finding noting the fragment can't be statically validated.
//! - Drift on UPDATE SET clauses. UPDATE doesn't have a NOT NULL
//!   contract — every column is optional in a SET. The proposal lists
//!   "UPDATE SET writes unknown column" as a v0.1 fire condition; this
//!   cell defers it to a follow-up because resolving "unknown column"
//!   per-resource without a `WHERE` clause analysis multiplies the
//!   surface beyond the v0.1 budget. Today the rule sees UPDATE
//!   statements and returns no findings (verified by
//!   `negative_update_does_not_false_positive`).
//! - Type checks on the VALUES list. Handled by a follow-up
//!   `HANDLER-SQL-TYPE-DRIFT-001` per the proposal.
//! - Cross-resource `IR helpers` integration. The proposal calls for
//!   `expected_columns_for` to be extracted into
//!   `crate::correctness::resource_columns`; this cell inlines the
//!   `db:` tag parser per the task instruction. The extraction is a
//!   deferred follow-up.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::allow_comment::source_contains_doctor_allow;
use crate::error_handling::walker::GoHandlerSourceFile;

// ── output ───────────────────────────────────────────────────────────────────

include!("handler_sql_column_drift_001_p1.rs");
include!("handler_sql_column_drift_001_p2.rs");
include!("handler_sql_column_drift_001_p3.rs");
