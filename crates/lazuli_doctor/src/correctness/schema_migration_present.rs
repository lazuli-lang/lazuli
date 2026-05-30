//! @correctness.migration_out_of_sync — IR resource columns drift from
//! the latest emitted SQL migration on disk.
//!
//! Cycle: codegen-correctness-cycle-2026-05-21, cell A12.
//!
//! Even after cells A10 (`SchemaDiff` algorithm) and A11 (ALTER emission)
//! land, an author can add a field to an `.lzi` resource and forget to
//! run `lazuli generate go .`. The IR and the on-disk migration tree
//! then drift out of sync silently. This rule walks each resource in
//! the capsule, locates the highest-numbered emitted migration matching
//! `dist/go/migrations/NNN_<feature>_<resource>*.sql`, parses the
//! `CREATE TABLE` column list, and fires a warning when the IR's
//! current field set diverges (adds / drops / type changes).
//!
//! Severity: warning. The diagnostic is friendly (it does not block
//! `lazuli doctor` from passing in `--profile dev`) because the
//! corrective action is a single command. Strict profiles may
//! escalate.
//!
//! A10/A11 coordination: as of the worktree base (`main` @ c90bbab) the
//! `SchemaDiff` algorithm and `parse_baseline_from_migration` helpers
//! were not yet on disk. This file therefore inlines a minimal column
//! parser (`parse_create_table_columns`) + diff (`column_diff`) so the
//! warning path stands on its own. Once A10 lands, the parser + diff
//! should consolidate into the shared `lazuli_codegen_go` helpers and
//! this module shrinks to a `pub use` shim. The duplication is
//! deliberately scoped to two private fns at the bottom of this file so
//! the merge-time deletion is mechanical.
//!
//! No-codegen-yet handling: if `dist/go/migrations/` does not exist OR
//! exists but contains no file matching this resource, the rule emits
//! a `MigrationMissing` finding asking the author to run the initial
//! codegen. This is distinct from `Drift` so callers can downgrade
//! `MigrationMissing` independently if desired (e.g. greenfield init).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, CapabilityRef, Feature, Resource, Tenancy, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

include!("schema_migration_present_p1.rs");
include!("schema_migration_present_p2.rs");
