//! MIGRATION-ALTER-MISSING-001 — IR adds columns that no ALTER TABLE
//! migration ever applies to already-deployed databases.
//!
//! Fires when a resource's IR field set is a strict superset of the
//! column set declared by the migration history (baseline `CREATE
//! TABLE` plus subsequent `ALTER TABLE ... ADD COLUMN`s).
//!
//! Sibling to `@correctness.migration_out_of_sync`
//! ([`crate::correctness::schema_migration_present`]) — that rule
//! catches "author added a field but forgot to regen". This rule
//! catches the deployment-history failure mode: the codegen DID
//! refresh the on-disk migration, but it edited the baseline
//! `CREATE TABLE` in place (Mode 1) or relied on `CREATE TABLE IF
//! NOT EXISTS` against a pre-existing schema (Mode 2). The static
//! IR↔file diff passes; production databases never gain the column.
//!
//! Reference: docs/proposals/doctor-migration-schema-drift-unrepresented.md.
//!
//! ## Detection
//!
//! Per-resource walk:
//!
//! 1. Collect every `dist/go/migrations/NNN_<feature>_<resource>*.sql`
//!    file in ascending `NNN` order.
//! 2. Take the FIRST migration as the baseline; parse its
//!    `CREATE TABLE` column list (the "what already-deployed databases
//!    have").
//! 3. For every subsequent migration, parse `ALTER TABLE ... ADD
//!    COLUMN ...` lines and union their columns into the "deployed"
//!    set.
//! 4. Compute `IR \ deployed`. Non-empty → fire.
//!
//! ## Severity
//!
//! - prototype: `info`
//! - strict: `warning`
//! - production / tdd-iron-hand: `error`
//!
//! The dispatcher maps the profile; this module exposes the default
//! via [`Finding::default_severity`] so callers can downgrade /
//! escalate per-profile.
//!
//! ## Mitigations
//!
//! - **Multi-column ALTER ambiguity**: when the rule encounters an
//!   `ALTER TABLE` form it cannot parse (multi-column ADD,
//!   transaction-wrapped ALTER, ALTER COLUMN TYPE), it emits an
//!   [`FindingKind::UnrecognisedMigration`] finding instead of
//!   silently false-firing. The author can audit and opt-out.
//! - **Opt-out**: `# doctor:allow MIGRATION-ALTER-MISSING-001` in the
//!   resource's `.lzi` file silences the per-resource diagnostic.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, CapabilityRef, Feature, Resource, Tenancy, TypeRef};

use crate::DoctorSeverity;
use crate::allow_comment::file_contains_doctor_allow;

// ── output ───────────────────────────────────────────────────────────────────

include!("migration_alter_missing_001_p1.rs");
include!("migration_alter_missing_001_p2.rs");
