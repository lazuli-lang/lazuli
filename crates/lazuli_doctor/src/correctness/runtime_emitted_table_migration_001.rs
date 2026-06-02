//! RUNTIME-EMITTED-TABLE-MIGRATION-001 — a framework-synthesized table the
//! runtime/codegen WRITES at execution time has no `CREATE TABLE` migration
//! emitted into the project's `dist/go/migrations/` tree.
//!
//! ## The bug class
//!
//! The Lazuli Go runtime hard-wires SQL against a handful of
//! framework-synthesized tables that are NOT authored resources:
//!
//! - `audit_log`   — `INSERT INTO audit_log` (`runtime/.../handle.go`,
//!   `auth/audit.go`) for every command. Its DDL is emitted UNCONDITIONALLY
//!   by codegen (`emit_audit_log_ddl`), so it is effectively always present.
//! - `lazuli_audit`  — `INSERT INTO lazuli_audit` inside the command tx
//!   (SEC-C2, `handle.go::writeAuditRow`) whenever the command declares
//!   `audit ...`.
//! - `lazuli_outbox` — `INSERT INTO lazuli_outbox` inside the command tx
//!   (`handle.go::writeGuaranteedOutboxRows`) whenever an emitted event is
//!   `outbox guaranteed`; the pump SELECTs/UPDATEs it
//!   (`events/outbox.go`).
//! - `lazuli_inbox`  — `SELECT/INSERT lazuli_inbox` for inbound-event
//!   idempotency (`events/inbox.go`).
//!
//! If codegen emits runtime code that touches one of these tables but no
//! migration `CREATE`s it, the app 500s at runtime ("relation ... does not
//! exist") on the first request that exercises the path. A point-fix exists
//! for `audit_log` (it is always emitted); this rule GENERALIZES that to the
//! whole synthesized-table set.
//!
//! ## What it checks
//!
//! Fires when an active synthesized table has no emitted `CREATE TABLE`
//! migration. For each framework table in the [`SynthesizedTable`] catalog whose
//! activation predicate is satisfied by the IR (see [`Activation`]):
//!
//! 1. collect the tables actually `CREATE`d by the emitted migrations under
//!    `<root>/dist/go/migrations/*.sql`, and
//! 2. fire when an active table is absent from that set.
//!
//! Tables whose activation cannot be proven from the IR (e.g. `lazuli_inbox`,
//! whose consumer side is a runtime/registry concern) are carried in the
//! catalog for documentation and for the codegen-side face-parity harness,
//! but are NOT fired by this rule — see each entry's [`Activation::Manual`].
//!
//! ## Severity
//!
//! - prototype: `info`
//! - strict: `warning`
//! - production / tdd-iron-hand: `error`
//!
//! A missing synthesized-table migration is a hard runtime 500, so the
//! production default is `error`. The aggregator resolves the per-profile
//! severity (mirroring its sibling migration rules).
//!
//! ## Opt-out
//!
//! `-- doctor:allow RUNTIME-EMITTED-TABLE-MIGRATION-001` in ANY migration
//! file under the tree silences the diagnostic (e.g. a pilot that supplies
//! these tables via a hand-managed migration runner outside `dist/`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, OutboxMode};

use crate::DoctorSeverity;

// ── synthesized-table catalog (single source of truth) ────────────────────────

/// How a synthesized framework table becomes active for an app — the
/// predicate this rule evaluates against the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Always emitted/required (e.g. `audit_log`, whose DDL codegen emits
    /// unconditionally). Never a finding in practice, but pinned in the
    /// catalog so the face-parity harness can assert a migration source.
    Always,
    /// Active when any command in any feature declares `audit ...`.
    AnyCommandAudits,
    /// Active when any event (or event-group) opts into the transactional
    /// outbox (`outbox guaranteed`).
    AnyGuaranteedOutbox,
    /// Activation is a runtime/registry concern that cannot be proven from
    /// the IR — carried for documentation + the codegen face-parity harness,
    /// NOT fired by this doctor rule.
    Manual,
}

/// One framework-synthesized table: the table the runtime writes, the
/// activation predicate, and a human anchor for the migration source.
#[derive(Debug, Clone, Copy)]
pub struct SynthesizedTable {
    /// The SQL table name the runtime hard-wires (`INSERT INTO <name>` /
    /// `FROM <name>`).
    pub table: &'static str,
    /// When this table becomes active for an app.
    pub activation: Activation,
    /// Where the migration that should `CREATE` it lives — names the emitter
    /// (or static migration) for the diagnostic message.
    pub migration_source: &'static str,
}

/// The closed catalog of framework-synthesized runtime-written tables.
///
/// Kept in sync with the runtime by the codegen-side face-parity harness
/// (`crates/lazuli_codegen_go/tests/emitted_table_parity.rs`), which scrapes
/// the runtime Go for `lazuli_*` table references and asserts each is listed
/// here. A new runtime-written framework table forces an entry here (and a
/// migration source) or the parity test fails.
pub const SYNTHESIZED_TABLES: &[SynthesizedTable] = &[
    SynthesizedTable {
        table: "audit_log",
        activation: Activation::Always,
        migration_source: "lazuli_codegen_go::emitter::audit::emit_audit_log_ddl \
                           (migrations/audit_log.sql)",
    },
    SynthesizedTable {
        table: "lazuli_audit",
        activation: Activation::AnyCommandAudits,
        migration_source: "migrations/000_lazuli_audit.sql (SEC-C2 audit-trail table)",
    },
    SynthesizedTable {
        table: "lazuli_outbox",
        activation: Activation::AnyGuaranteedOutbox,
        migration_source: "migrations/002_create_lazuli_outbox.sql \
                           (EVENT-OUTBOX §3.3 transactional outbox)",
    },
    SynthesizedTable {
        table: "lazuli_inbox",
        // Inbound-event dedup is a runtime/registry concern (the consumer
        // side is not modelled in the per-feature IR), so we cannot prove
        // activation here. Carried for the face-parity harness only.
        activation: Activation::Manual,
        migration_source: "migrations/003_create_lazuli_inbox.sql \
                           (EVENT-OUTBOX §3.3 inbound dedup)",
    },
];

// ── output ───────────────────────────────────────────────────────────────────

/// One finding of `RUNTIME-EMITTED-TABLE-MIGRATION-001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The migrations directory we searched (diagnostic anchor).
    pub path: PathBuf,
    /// The framework table the runtime writes but no migration creates.
    pub table: String,
    /// Why the table is active (rendered into the message).
    pub reason: String,
    /// Where the migration creating it should come from.
    pub migration_source: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "RUNTIME-EMITTED-TABLE-MIGRATION-001";

    /// Default severity per the doctor profile defaults; the aggregator
    /// overrides per `--profile`. Returns the `strict` default.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::correctness::runtime_emitted_table_migration_001::Finding;
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    /// ```
    pub fn default_severity() -> DoctorSeverity {
        DoctorSeverity::Warning
    }

    /// Render the diagnostic message — names the table, why it is active,
    /// the missing migration source, and the remediation.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::runtime_emitted_table_migration_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("dist/go/migrations"),
    ///     table: "lazuli_outbox".into(),
    ///     reason: "an event declares `outbox guaranteed`".into(),
    ///     migration_source: "migrations/002_create_lazuli_outbox.sql".into(),
    /// };
    /// assert!(f.message().contains("lazuli_outbox"));
    /// assert!(f.message().contains("does not exist"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "the runtime writes the framework table `{table}` ({reason}), but no \
             migration under {dir} creates it. The first request that exercises \
             this path will 500 with `relation \"{table}\" does not exist`. The \
             table's DDL comes from {source}; ensure it is emitted into \
             `dist/go/migrations/`. Opt-out: \
             `-- doctor:allow RUNTIME-EMITTED-TABLE-MIGRATION-001 — reason \"...\"`.",
            table = self.table,
            reason = self.reason,
            dir = self.path.display(),
            source = self.migration_source,
        )
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `RUNTIME-EMITTED-TABLE-MIGRATION-001` once per project.
///
/// `features` is the full set of synthesized feature views (the same shape
/// the correctness aggregator builds); `root` is the project root holding
/// `dist/go/migrations/`. The rule is history-aware only insofar as it reads
/// the on-disk migration tree — it does not consult prior runs.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::runtime_emitted_table_migration_001::check;
///
/// let features: Vec<&lazuli_ir::Feature> = vec![];
/// let _ = check(&features, Path::new("/app"));
/// ```
pub fn check(features: &[&Feature], root: &Path) -> Vec<Finding> {
    let migrations_dir = root.join("dist").join("go").join("migrations");

    // No migration tree at all → nothing emitted yet. The
    // `schema_migration_present` rule owns the "run initial codegen"
    // message; staying silent here avoids a double-diagnostic.
    if !migrations_dir.is_dir() {
        return Vec::new();
    }

    let (created, opt_out) = scan_migrations(&migrations_dir);
    if opt_out {
        return Vec::new();
    }

    let mut out = Vec::new();
    for entry in SYNTHESIZED_TABLES {
        let Some(reason) = activation_reason(entry.activation, features) else {
            continue;
        };
        if created.contains(entry.table) {
            continue;
        }
        out.push(Finding {
            path: migrations_dir.clone(),
            table: entry.table.to_owned(),
            reason: reason.to_owned(),
            migration_source: entry.migration_source.to_owned(),
        });
    }
    out
}

/// Resolve whether an activation predicate is satisfied by the IR, returning
/// the human reason when it is. `Manual` always returns `None` (not fired).
fn activation_reason(activation: Activation, features: &[&Feature]) -> Option<&'static str> {
    match activation {
        Activation::Always => Some("the runtime emits it for every command"),
        Activation::AnyCommandAudits => features
            .iter()
            .flat_map(|f| f.commands.iter())
            .any(|c| c.audit.is_some())
            .then_some("a command declares `audit ...`, so the runtime writes an audit row"),
        Activation::AnyGuaranteedOutbox => {
            let from_events = features
                .iter()
                .flat_map(|f| f.events.iter())
                .any(|e| e.outbox.is_guaranteed());
            // Event groups carry the outbox mode on two parallel slots
            // (mirroring codegen's `build_outbox_index`): the legacy
            // `events_outbox` vector and the typed `variants[].outbox`.
            let from_groups = features.iter().flat_map(|f| f.event_groups.iter()).any(|g| {
                g.events_outbox.iter().any(OutboxMode::is_guaranteed)
                    || g.variants.iter().any(|v| v.outbox.is_guaranteed())
            });
            (from_events || from_groups)
                .then_some("an event declares `outbox guaranteed`, so the runtime writes an outbox row")
        }
        Activation::Manual => None,
    }
}

// ── migration directory scan ──────────────────────────────────────────────────

/// Walk every `*.sql` (forward) migration under `dir`, returning the set of
/// `CREATE TABLE`-d table names and whether ANY file carries the
/// rule's `-- doctor:allow` opt-out.
fn scan_migrations(dir: &Path) -> (BTreeSet<String>, bool) {
    let mut created = BTreeSet::new();
    let mut opt_out = false;
    let Ok(read) = fs::read_dir(dir) else {
        return (created, opt_out);
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        // Forward migrations only — `.down.sql` DROPs, not CREATEs.
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        let Ok(sql) = fs::read_to_string(&path) else {
            continue;
        };
        if source_contains_sql_doctor_allow(&sql, Finding::CODE) {
            opt_out = true;
        }
        for table in parse_create_table_names(&sql) {
            created.insert(table);
        }
    }
    (created, opt_out)
}

/// Collect every `CREATE TABLE [IF NOT EXISTS] <ident>` table name in `sql`.
/// Mirrors the conservative single-pass scanner in
/// [`crate::correctness::migration_idempotent_create_001`] (names only; the
/// idempotency flag is not needed here). Schema qualifiers (`public.foo`) are
/// reduced to the last segment.
fn parse_create_table_names(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = sql.to_ascii_lowercase();
    let mut cursor = 0usize;
    while cursor < lower.len() {
        let Some(rel) = lower[cursor..].find("create table") else {
            break;
        };
        let pos = cursor + rel;
        if pos > 0 {
            let prev = sql.as_bytes()[pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                cursor = pos + 1;
                continue;
            }
        }
        let after = &sql[pos + "create table".len()..];
        let after_lower = after.to_ascii_lowercase();
        let name_after = if after_lower.trim_start().starts_with("if not exists") {
            let trimmed_left = after.trim_start();
            let consumed = after.len() - trimmed_left.len();
            &after[consumed + "if not exists".len()..]
        } else {
            after
        };
        let table = first_ident(name_after);
        if !table.is_empty() {
            let bare = match table.rsplit_once('.') {
                Some((_, last)) => last.to_owned(),
                None => table,
            };
            out.push(bare);
        }
        cursor = pos + "create table".len();
    }
    out
}

/// First whitespace-separated identifier from `s`, stripping surrounding
/// double quotes / backticks.
fn first_ident(s: &str) -> String {
    let s = s.trim_start();
    if s.is_empty() {
        return String::new();
    }
    if let Some(rest) = s.strip_prefix('"')
        && let Some(end) = rest.find('"')
    {
        return rest[..end].to_owned();
    }
    let token = s
        .split(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == ',')
        .next()
        .unwrap_or("");
    token.trim_matches('"').trim_matches('`').to_owned()
}

/// SQL-native `--` line-comment opt-out scanner — mirror of the sibling in
/// [`crate::correctness::migration_idempotent_create_001`]. Case-insensitive
/// on both `doctor:allow` and the code token.
fn source_contains_sql_doctor_allow(source: &str, code: &str) -> bool {
    let needle_lower = format!("doctor:allow {}", code.to_ascii_lowercase());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("--") {
            continue;
        }
        if trimmed.to_ascii_lowercase().contains(&needle_lower) {
            return true;
        }
    }
    false
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("runtime_emitted_table_migration_001_tests.rs");
}
