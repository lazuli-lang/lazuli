//! MIGRATION-IDEMPOTENT-CREATE-001 — `CREATE TABLE IF NOT EXISTS`
//! used in a non-baseline migration is a deployment-history footgun.
//!
//! Fires when a non-first migration for the same `<table>` uses
//! `CREATE TABLE IF NOT EXISTS`. Severity: `warning` default,
//! `error` strict/production.
//!
//! Sibling to [`crate::correctness::migration_alter_missing_001`].
//! Both rules target the same root cause (history-blind static
//! checks); this one catches it from the SQL side. `CREATE TABLE IF
//! NOT EXISTS` is the canonical greenfield-safe shape for the FIRST
//! migration of a table — and ONLY the first. Subsequent migrations
//! that re-CREATE the same table even idempotently hide drift: when
//! the table already exists with a different (pre-polluted) shape,
//! the IF NOT EXISTS clause silently no-ops and any new columns
//! declared in the body never materialise.
//!
//! Documented in the 2026-05-25 AcmeCRM pollution incident
//! ([feedback_lazuli_framework_migrations_dir_pollution.md]).
//!
//! Reference: docs/proposals/doctor-migration-schema-drift-unrepresented.md
//! §"Detection — MIGRATION-IDEMPOTENT-CREATE-001".
//!
//! ## Detection
//!
//! Walk every `dist/go/migrations/NNN_*.sql` file in ascending `NNN`
//! order. For each `CREATE TABLE IF NOT EXISTS <table>` clause:
//!
//! 1. If this is the first migration we've seen mention `<table>` → no
//!    fire (this is the canonical baseline shape).
//! 2. Otherwise → fire, naming the earlier baseline migration.
//!
//! Bare `CREATE TABLE <table>` (no `IF NOT EXISTS`) in a non-first
//! migration is a different bug class (DDL failure on rerun) and not
//! this rule's job.
//!
//! ## Severity
//!
//! - prototype: `info`
//! - strict: `warning`
//! - production / tdd-iron-hand: `error`
//!
//! ## Opt-out
//!
//! `-- doctor:allow MIGRATION-IDEMPOTENT-CREATE-001` in the SQL file
//! silences the diagnostic. The `--` line-comment form is recognised
//! in addition to the canonical `#` form because `--` is the
//! SQL-native comment marker; the proposal explicitly calls for both.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::DoctorSeverity;

// ── output ───────────────────────────────────────────────────────────────────

/// One finding of `MIGRATION-IDEMPOTENT-CREATE-001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// SQL migration file that fired (the OFFENDING migration —
    /// non-baseline `CREATE TABLE IF NOT EXISTS`).
    pub path: PathBuf,
    /// Table name targeted by the offending CREATE clause.
    pub table: String,
    /// Path of the earlier migration that already serves as the
    /// baseline `CREATE TABLE IF NOT EXISTS` for this table.
    pub earlier_migration: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "MIGRATION-IDEMPOTENT-CREATE-001";

    /// Default severity per the doctor profile defaults. The CLI
    /// dispatcher may override per `--profile`. v0.1 returns `Warning`
    /// (the `strict` default).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::correctness::migration_idempotent_create_001::Finding;
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    /// ```
    pub fn default_severity() -> DoctorSeverity {
        DoctorSeverity::Warning
    }

    /// Render the diagnostic message — names the offending migration,
    /// the table, the earlier baseline, and the recommended ALTER
    /// TABLE remediation.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::migration_idempotent_create_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("dist/go/migrations/007_account_user_session.sql"),
    ///     table: "user_session".into(),
    ///     earlier_migration: PathBuf::from("dist/go/migrations/004_account_user.sql"),
    /// };
    /// assert!(f.message().contains("CREATE TABLE IF NOT EXISTS"));
    /// assert!(f.message().contains("ALTER TABLE"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "migration {path} contains `CREATE TABLE IF NOT EXISTS \"{table}\"`, \
             but an earlier migration ({earlier}) is the baseline for that table. \
             In production the IF NOT EXISTS clause silently no-ops if a prior \
             migration's CREATE succeeded against a pre-existing table — any new \
             columns declared here will never materialise. Forward column \
             additions must use ALTER TABLE; baseline CREATE is one-shot per \
             table. Opt-out: `-- doctor:allow MIGRATION-IDEMPOTENT-CREATE-001 — \
             reason \"...\"`.",
            path = self.path.display(),
            table = self.table,
            earlier = self.earlier_migration.display(),
        )
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `MIGRATION-IDEMPOTENT-CREATE-001` against every migration file
/// under `<root>/dist/go/migrations/`. The rule is pure on disk —
/// it does NOT consult the IR.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::migration_idempotent_create_001::check;
///
/// let _ = check(Path::new("/app"));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let migrations_dir = root.join("dist").join("go").join("migrations");
    let mut entries = list_migration_entries(&migrations_dir);
    entries.sort_by_key(|e| e.number);

    // First migration (by NNN) we saw mention a given table — the
    // canonical baseline. Subsequent CREATEs are footguns.
    let mut first_seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut out = Vec::new();

    for entry in &entries {
        let Ok(sql) = fs::read_to_string(&entry.path) else {
            continue;
        };
        let allow_opt_out = source_contains_sql_doctor_allow(&sql, Finding::CODE);

        // All CREATE TABLE [IF NOT EXISTS] <table> tables in this file
        // — paired with whether the clause used IF NOT EXISTS.
        let creates = parse_create_table_clauses(&sql);

        for (table, idempotent) in creates {
            let Some(baseline_for_table) = first_seen.get(&table).cloned() else {
                first_seen.insert(table.clone(), entry.path.clone());
                continue; // first sighting — legitimate baseline
            };
            // Re-CREATE in a non-first position. We fire ONLY when the
            // shape is the idempotent IF NOT EXISTS (per proposal —
            // bare CREATE TABLE in non-first position is a different
            // bug class).
            if !idempotent {
                continue;
            }
            if allow_opt_out {
                continue;
            }
            out.push(Finding {
                path: entry.path.clone(),
                table,
                earlier_migration: baseline_for_table,
            });
        }
    }
    out
}

// ── migration directory walk ────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MigrationEntry {
    path: PathBuf,
    number: u32,
}

fn list_migration_entries(dir: &Path) -> Vec<MigrationEntry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        let trimmed = name.trim_end_matches(".sql");
        let Some((number_text, _)) = trimmed.split_once('_') else {
            continue;
        };
        let Ok(number) = number_text.parse::<u32>() else {
            continue;
        };
        out.push(MigrationEntry { path, number });
    }
    out
}

// ── SQL CREATE TABLE scanner ────────────────────────────────────────────────

/// Walk `sql` for every `CREATE TABLE [IF NOT EXISTS] <ident>` clause.
/// Returns `(table_name, idempotent_flag)` pairs in source order.
///
/// `idempotent_flag == true` when the clause used `IF NOT EXISTS`.
///
/// Recognised shapes:
///
/// - `CREATE TABLE IF NOT EXISTS "<table>" (...)`
/// - `CREATE TABLE IF NOT EXISTS <table> (...)`
/// - `CREATE TABLE "<table>" (...)`
/// - `CREATE TABLE <table> (...)`
///
/// Schema-qualified identifiers (`public.<table>`) are reduced to the
/// last segment. Comments and string literals are NOT stripped (the
/// scanner is conservative; if the rule misfires inside a comment the
/// author can opt-out).
fn parse_create_table_clauses(sql: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let lower = sql.to_ascii_lowercase();
    let mut cursor = 0usize;
    while cursor < lower.len() {
        let Some(rel) = lower[cursor..].find("create table") else {
            break;
        };
        let pos = cursor + rel;
        // Boundary check: previous byte must not be alphanumeric/_
        // (avoid matching `recreate_table_xxx` etc.).
        if pos > 0 {
            let prev = sql.as_bytes()[pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                cursor = pos + 1;
                continue;
            }
        }
        let after = &sql[pos + "create table".len()..];
        let after_lower = after.to_ascii_lowercase();
        let (idempotent, name_after) = if after_lower.trim_start().starts_with("if not exists") {
            let trimmed_left = after.trim_start();
            let consumed = after.len() - trimmed_left.len();
            (true, &after[consumed + "if not exists".len()..])
        } else {
            (false, after)
        };

        let table = first_ident(name_after);
        if !table.is_empty() {
            // Strip schema qualifier `public.foo` → `foo`.
            let bare = match table.rsplit_once('.') {
                Some((_, last)) => last.to_owned(),
                None => table,
            };
            out.push((bare, idempotent));
        }
        cursor = pos + "create table".len();
    }
    out
}

/// First whitespace-separated identifier from `s`, stripping
/// surrounding double quotes / backticks. Returns the identifier
/// without trailing `(` / whitespace.
fn first_ident(s: &str) -> String {
    let s = s.trim_start();
    if s.is_empty() {
        return String::new();
    }
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return rest[..end].to_owned();
        }
    }
    let token = s
        .split(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == ',')
        .next()
        .unwrap_or("");
    token.trim_matches('"').trim_matches('`').to_owned()
}

// ── SQL allow-comment scanner (`--` line form) ──────────────────────────────

/// Mirror of [`crate::allow_comment::source_contains_doctor_allow`]
/// for the SQL-native `--` line-comment form. The canonical helper
/// only recognises `#`-prefixed comments (`.lzi` / `.lzx`); SQL files
/// use `--`. The proposal explicitly calls for both.
///
/// Case-insensitive on both `doctor:allow` and the code token. Read
/// failures degrade silently to `false`.
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

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_migration(root: &Path, name: &str, body: &str) -> PathBuf {
        let dir = root.join("dist").join("go").join("migrations");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn positive_repeat_create_if_not_exists_fires() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL\n);\n",
        );
        let offending = write_migration(
            tmp.path(),
            "007_account_user_session.sql",
            "CREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  phone TEXT\n);\n",
        );

        let findings = check(tmp.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].path, offending);
        assert_eq!(findings[0].table, "user");
        assert!(
            findings[0]
                .earlier_migration
                .ends_with("004_account_user.sql")
        );
        assert_eq!(Finding::CODE, "MIGRATION-IDEMPOTENT-CREATE-001");
        assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    }

    #[test]
    fn negative_first_create_if_not_exists_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "001_account_user.sql",
            "CREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY\n);\n",
        );

        let findings = check(tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn negative_bare_create_non_first_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "001_account_user.sql",
            "CREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY\n);\n",
        );
        // Non-first migration uses bare CREATE TABLE — not this
        // rule's job (different bug class).
        write_migration(
            tmp.path(),
            "002_account_user_strict.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY\n);\n",
        );

        let findings = check(tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn negative_allow_comment_silences_idempotent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY\n);\n",
        );
        write_migration(
            tmp.path(),
            "007_account_user_session.sql",
            "-- doctor:allow MIGRATION-IDEMPOTENT-CREATE-001 — reason \"test fixture reset\"\nCREATE TABLE IF NOT EXISTS \"user\" (\n  id BIGSERIAL PRIMARY KEY\n);\n",
        );

        let findings = check(tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn order_independent_of_dir_listing() {
        // Two tables, one first-baselined late in the sequence. Verify
        // the rule keys on the NNN number, not directory enumeration
        // order.
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "010_billing_invoice.sql",
            "CREATE TABLE IF NOT EXISTS \"invoice\" (id BIGSERIAL PRIMARY KEY);\n",
        );
        write_migration(
            tmp.path(),
            "020_billing_invoice_redux.sql",
            "CREATE TABLE IF NOT EXISTS \"invoice\" (id BIGSERIAL PRIMARY KEY, total NUMERIC);\n",
        );

        let findings = check(tmp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].table, "invoice");
        assert!(findings[0].path.ends_with("020_billing_invoice_redux.sql"));
    }

    // ── parser unit tests ───────────────────────────────────────────────

    #[test]
    fn parser_picks_quoted_and_bare_with_idempotent_flag() {
        let v = parse_create_table_clauses(
            "CREATE TABLE IF NOT EXISTS \"a\" (id INT); CREATE TABLE b (id INT);",
        );
        assert_eq!(v, vec![("a".to_string(), true), ("b".to_string(), false)],);
    }

    #[test]
    fn parser_strips_schema_qualifier() {
        let v = parse_create_table_clauses("CREATE TABLE IF NOT EXISTS public.user (id INT);");
        assert_eq!(v, vec![("user".to_string(), true)]);
    }

    #[test]
    fn parser_ignores_word_boundary_collision() {
        // `recreate_table` is NOT a CREATE TABLE statement.
        let v = parse_create_table_clauses("-- comment recreate_table\nCREATE TABLE foo (id INT);");
        assert_eq!(v, vec![("foo".to_string(), false)]);
    }

    #[test]
    fn sql_allow_comment_recognised_dash_dash() {
        assert!(source_contains_sql_doctor_allow(
            "-- doctor:allow MIGRATION-IDEMPOTENT-CREATE-001\nCREATE TABLE x();\n",
            "MIGRATION-IDEMPOTENT-CREATE-001"
        ));
    }

    #[test]
    fn sql_allow_comment_ignored_when_not_in_comment() {
        assert!(!source_contains_sql_doctor_allow(
            "INSERT INTO logs VALUES ('doctor:allow MIGRATION-IDEMPOTENT-CREATE-001');\n",
            "MIGRATION-IDEMPOTENT-CREATE-001"
        ));
    }
}
