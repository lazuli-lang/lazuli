//! FACE-PARITY harness — emitted-table contract (W2-9).
//!
//! Sibling to the codegen↔runtime face-parity tests
//! (`crates/lazuli_codegen_go/tests/{ctx_path,source_kind}_parity.rs` and
//! `runtime/go/lazuli/readctx_parity_test.go`). Those gate the ctx-path and
//! source-kind faces; THIS one gates the *emitted-table* face that W2-4's
//! `RUNTIME-EMITTED-TABLE-MIGRATION-001` doctor rule guards per-pilot.
//!
//! ## The contract
//!
//! The Lazuli Go runtime hard-wires SQL against framework-synthesized
//! tables (`audit_log`, `lazuli_audit`, `lazuli_outbox`, `lazuli_inbox`).
//! Each MUST be:
//!   1. catalogued in the doctor SoT
//!      (`lazuli_doctor::correctness::runtime_emitted_table_migration_001::
//!      SYNTHESIZED_TABLES`), so the per-pilot doctor rule knows to check it;
//!   2. backed by a real migration source on disk.
//!
//! Two directions, so neither face can drift:
//!
//!   - **runtime ⊆ catalog** (`runtime_synth_tables_are_catalogued`): every
//!     `lazuli_*` table the runtime references (scraped from the runtime Go)
//!     is in the catalog. A new runtime-written framework table forces a
//!     catalog entry — which forces a migration source + the doctor rule to
//!     check it. THE DANGEROUS DIRECTION (an uncatalogued runtime table is
//!     exactly the 500-at-runtime gap W2-4 targets).
//!   - **catalog ⊆ migration-source** (`catalogued_tables_have_a_migration`):
//!     every catalogued table's migration source actually exists on disk
//!     (emitter for `audit_log`, static `migrations/*.sql` for the rest).
//!     Keeps the catalog honest — no orphan entries pointing at a deleted
//!     migration.
//!
//! ## Why this lives in `lazuli_doctor`, not `lazuli_codegen_go`
//!
//! The SoT is the `SYNTHESIZED_TABLES` const in `lazuli_doctor`. The codegen
//! crate cannot depend on `lazuli_doctor` (doctor already depends on codegen
//! — that would cycle), so the natural home for a test that reads the const
//! is `lazuli_doctor`'s own test tree. It still reaches across to the
//! runtime Go + codegen sources via the workspace root, exactly like the
//! codegen-side harnesses do.
//!
//! ## Runtime-symbol (DBTX) parity — W2-9 scope note
//!
//! A full codegen↔runtime symbol-parity (every `lazuli.<Sym>` the codegen
//! emits is defined in the runtime) is broad and overlaps the existing
//! source-kind / ctx-path harnesses. `dbtx_symbol_is_defined_both_sides`
//! pins the one symbol W2-9 calls out — `lazuli.DBTX` — from both faces so a
//! rename on either side fails loudly. TODO(W2-9-followup): generalize to a
//! scrape of ALL emitted `lazuli.<TypeSym>(` references vs the runtime's
//! exported type set (tracked alongside the source_kind_parity author-tail
//! gap).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_doctor::correctness::runtime_emitted_table_migration_001::{
    Activation, SYNTHESIZED_TABLES,
};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_doctor
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(rel: &str) -> String {
    let p = workspace_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Recursively collect every `.go` file under `dir` (skipping `_test.go`,
/// whose assertions name tables not necessarily touched by production code).
fn collect_go_sources(dir: &Path, acc: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_go_sources(&path, acc);
        } else if path.extension().and_then(|e| e.to_str()) == Some("go") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with("_test.go") {
                continue;
            }
            if let Ok(src) = fs::read_to_string(&path) {
                acc.push(src);
            }
        }
    }
}

/// Scrape `lazuli_*` table names out of the runtime SQL string literals
/// (`INSERT INTO lazuli_x`, `FROM lazuli_x`, `UPDATE lazuli_x`,
/// `DELETE FROM lazuli_x`). The framework tables of interest all carry the
/// `lazuli_` prefix; `audit_log` is the one unprefixed peer and is added
/// explicitly via the known-INSERT scan below.
fn runtime_synthesized_tables() -> BTreeSet<String> {
    let mut sources = Vec::new();
    let runtime = workspace_root().join("runtime").join("go").join("lazuli");
    collect_go_sources(&runtime, &mut sources);

    let mut out = BTreeSet::new();
    for src in &sources {
        // `lazuli_<table>` literals — the framework-synthesized prefix.
        let bytes = src.as_bytes();
        let mut from = 0;
        while let Some(i) = src[from..].find("lazuli_") {
            let start = from + i;
            // Take the contiguous [a-z0-9_] run.
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            let ident = &src[start..end];
            // Filter to actual table names (lazuli_outbox / lazuli_inbox /
            // lazuli_audit). Exclude Go package paths (`lazuli.dev/...`) and
            // identifiers that are clearly not tables (heuristic: known set
            // of suffixes). We keep only the three known framework tables'
            // exact spellings to avoid scraping `lazuli_outbox_undispatched_idx`
            // (an index) or Go symbol fragments.
            if matches!(ident, "lazuli_outbox" | "lazuli_inbox" | "lazuli_audit") {
                out.insert(ident.to_owned());
            }
            from = end.max(start + 1);
        }
        // `audit_log` — the unprefixed framework table. Match the canonical
        // SQL verbs so we don't pick up comment prose.
        for verb in ["INTO audit_log", "FROM audit_log", "UPDATE audit_log"] {
            if src.contains(verb) {
                out.insert("audit_log".to_owned());
            }
        }
    }
    out
}

#[test]
fn runtime_synth_tables_are_catalogued() {
    let runtime = runtime_synthesized_tables();
    // Floor: the scraper must find the known framework tables, or a refactor
    // silently emptied it (false negative).
    assert!(
        runtime.contains("audit_log") && runtime.contains("lazuli_outbox"),
        "emitted-table scraper found neither audit_log nor lazuli_outbox in the \
         runtime Go — the scraper has gone stale (false-negative). Found: {runtime:?}"
    );

    let catalog: BTreeSet<String> = SYNTHESIZED_TABLES
        .iter()
        .map(|t| t.table.to_owned())
        .collect();

    let missing: Vec<&String> = runtime.difference(&catalog).collect();
    assert!(
        missing.is_empty(),
        "emitted-table FACE-PARITY violation (W2-4 class): the runtime Go writes \
         these framework table(s) but they are ABSENT from the doctor SoT catalog \
         (SYNTHESIZED_TABLES in runtime_emitted_table_migration_001.rs). An \
         uncatalogued runtime-written table = no doctor check = a 500 \
         (`relation does not exist`) ships silently. Add each to SYNTHESIZED_TABLES \
         with its activation predicate + migration source:\n  - {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

#[test]
fn catalogued_tables_have_a_migration() {
    // audit_log's DDL is emitted by codegen (no static file); the rest are
    // checked-in static migrations under `migrations/`.
    let audit_emitter = read("crates/lazuli_codegen_go/src/emitter/audit.rs");
    assert!(
        audit_emitter.contains("CREATE TABLE IF NOT EXISTS audit_log"),
        "audit_log catalog entry claims `emit_audit_log_ddl` as its source, but \
         that emitter no longer contains the audit_log CREATE TABLE."
    );

    let migrations_dir = workspace_root().join("migrations");
    let dist_migrations = workspace_root().join("dist").join("go").join("migrations");

    for entry in SYNTHESIZED_TABLES {
        if entry.table == "audit_log" {
            continue; // emitter-sourced, checked above.
        }
        // The migration creating this table must exist somewhere on disk:
        // either a static `migrations/*.sql` or the framework's own
        // `dist/go/migrations/*.sql` (lazuli_audit ships there).
        let needle = format!("CREATE TABLE IF NOT EXISTS {}", entry.table);
        let found = dir_has_create(&migrations_dir, &needle)
            || dir_has_create(&dist_migrations, &needle);
        assert!(
            found,
            "catalog entry `{}` (source: {}) has no `{needle}` migration on disk \
             under {} or {}. Either the migration was deleted or the catalog \
             migration_source is stale.",
            entry.table,
            entry.migration_source,
            migrations_dir.display(),
            dist_migrations.display(),
        );
    }
}

/// Any forward `.sql` under `dir` whose body contains `needle`.
fn dir_has_create(dir: &Path, needle: &str) -> bool {
    let Ok(read) = fs::read_dir(dir) else {
        return false;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        if let Ok(sql) = fs::read_to_string(&path)
            && sql.contains(needle)
        {
            return true;
        }
    }
    false
}

#[test]
fn manual_activation_only_for_inbox() {
    // Pins the activation classification: lazuli_inbox is the only Manual
    // entry (its consumer side is not IR-derivable). If a future table is
    // added Manual, this forces a conscious update + a documented reason.
    let manual: Vec<&str> = SYNTHESIZED_TABLES
        .iter()
        .filter(|t| t.activation == Activation::Manual)
        .map(|t| t.table)
        .collect();
    assert_eq!(
        manual,
        vec!["lazuli_inbox"],
        "Manual-activation set changed; confirm the new entry's activation \
         truly cannot be proven from the IR and update this pin."
    );
}

#[test]
fn dbtx_symbol_is_defined_both_sides() {
    // W2-9 runtime-symbol parity (scoped to the symbol W2-9 names). The
    // codegen emits referential-guard signatures typed `lazuli.DBTX`; the
    // runtime must define `type DBTX`. A rename on either side that drops
    // the other = a `go build` break; pin both faces.
    let guard = read("crates/lazuli_codegen_go/src/emitter/referential_guard.rs");
    assert!(
        guard.contains("lazuli.DBTX"),
        "codegen referential_guard.rs no longer emits `lazuli.DBTX` — if the \
         emitted handle type was renamed, update the runtime `type DBTX` + this pin."
    );
    let db = read("runtime/go/lazuli/db.go");
    assert!(
        db.contains("type DBTX interface"),
        "runtime db.go no longer defines `type DBTX interface`, but codegen emits \
         `lazuli.DBTX` guard signatures — this is a `go build` break. Restore the \
         type or update both faces together."
    );
}
