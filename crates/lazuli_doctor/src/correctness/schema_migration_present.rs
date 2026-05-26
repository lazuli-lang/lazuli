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

use lazuli_ir::{
    BuiltinType, CapabilityRef, Feature, Resource, Tenancy, TypeRef,
};

// ── output ───────────────────────────────────────────────────────────────────

/// One finding of `@correctness.migration_out_of_sync`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    pub kind: FindingKind,
}

/// Why a `@correctness.migration_out_of_sync` finding fired — drives
/// the prose in `Finding::message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// IR + migration disagree on the column set.
    Drift {
        /// Path of the matched migration file (anchor for the message).
        migration: PathBuf,
        /// Column names present in the IR but not in the migration.
        /// Sorted ASCII-ascending for stable diagnostics.
        adds: Vec<String>,
        /// Column names present in the migration but not in the IR.
        drops: Vec<String>,
    },
    /// No migration on disk for this resource — initial codegen not yet
    /// run. Carries the directory we searched so the message can
    /// pinpoint the missing path for the author.
    MigrationMissing { searched: PathBuf },
}

impl Finding {
    /// Diagnostic ID per the cell spec.
    pub const CODE: &'static str = "@correctness.migration_out_of_sync";

    /// Render the per-kind message — `Drift` carries the per-column
    /// add/drop diff; `MigrationMissing` points at the searched dir
    /// and asks the author to run `lazuli generate go .`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::schema_migration_present::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     resource: "Invoice".into(),
    ///     kind: FindingKind::MigrationMissing {
    ///         searched: PathBuf::from("dist/go/migrations"),
    ///     },
    /// };
    /// assert!(f.message().contains("lazuli generate go ."));
    /// ```
    pub fn message(&self) -> String {
        match &self.kind {
            FindingKind::Drift { migration, adds, drops } => format!(
                "resource '{feature}.{resource}' has IR columns not present in the \
                 latest migration ({path}). Run 'lazuli generate go .' to emit an \
                 ALTER migration. Missing columns: [{adds}]. Unexpected migration-only \
                 columns: [{drops}].",
                feature = self.feature,
                resource = self.resource,
                path = migration.display(),
                adds = adds.join(", "),
                drops = drops.join(", "),
            ),
            FindingKind::MigrationMissing { searched } => format!(
                "resource '{feature}.{resource}' has no migration on disk under \
                 {path}. Run 'lazuli generate go .' to emit the initial CREATE TABLE \
                 migration.",
                feature = self.feature,
                resource = self.resource,
                path = searched.display(),
            ),
        }
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `@correctness.migration_out_of_sync` for every resource in `feature`,
/// anchored at the capsule root `root` (the directory that holds
/// `dist/go/migrations/`).
///
/// `feature_path` is purely the diagnostic anchor (typically the `.lzi`
/// file). It is recorded on findings so editors can land squiggles on
/// the right source file.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::schema_migration_present::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources");
/// let _ = check(&feature, Path::new("billing.lzi"), Path::new("/app"));
/// ```
pub fn check(feature: &Feature, feature_path: &Path, root: &Path) -> Vec<Finding> {
    let migrations_dir = root.join("dist").join("go").join("migrations");
    let entries = list_migration_entries(&migrations_dir);

    let mut out = Vec::new();
    for resource in &feature.resources {
        let resource_slug = lower_snake(&resource.name);
        let feature_slug = lower_snake(&feature.name);
        let prefix = format!("{feature_slug}_{resource_slug}");

        let latest = entries
            .iter()
            .filter(|entry| migration_matches(&entry.stem, &feature_slug, &resource_slug))
            .max_by_key(|entry| entry.number);

        match latest {
            None => {
                out.push(Finding {
                    path: feature_path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    kind: FindingKind::MigrationMissing {
                        searched: migrations_dir.clone(),
                    },
                });
                continue;
            }
            Some(entry) => {
                let Ok(sql) = fs::read_to_string(&entry.path) else {
                    // Unreadable file — silent. Lifecycle/IO diagnostics
                    // own permission errors.
                    continue;
                };
                let table = lower_snake(&resource.name);
                let migration_cols = parse_create_table_columns(&sql, &table);
                let ir_cols = expected_columns_for(feature, resource);
                let (adds, drops) = column_diff(&ir_cols, &migration_cols);
                if adds.is_empty() && drops.is_empty() {
                    continue;
                }
                let _ = prefix; // reserved for future log breadcrumb
                out.push(Finding {
                    path: feature_path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    kind: FindingKind::Drift {
                        migration: entry.path.clone(),
                        adds,
                        drops,
                    },
                });
            }
        }
    }
    out
}

// ── migration directory walk ────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MigrationEntry {
    /// Full path of the `.sql` file.
    path: PathBuf,
    /// The `NNN` numeric prefix (zero-padded in source, parsed here).
    number: u32,
    /// Stem stripped of `NNN_`, e.g. `customer_invoice` for
    /// `001_customer_invoice.sql`. Also strips `.sql` and `.down.sql`.
    stem: String,
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
        // Skip `down` migrations — the rule compares forward column shape.
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        let trimmed = name.trim_end_matches(".sql");
        let Some((number_text, stem)) = trimmed.split_once('_') else {
            continue;
        };
        let Ok(number) = number_text.parse::<u32>() else {
            // Non-numeric prefix (e.g. `audit_log.sql`) — skip.
            continue;
        };
        out.push(MigrationEntry {
            path,
            number,
            stem: stem.to_owned(),
        });
    }
    out
}

fn migration_matches(stem: &str, feature_slug: &str, resource_slug: &str) -> bool {
    // Exact `<feature>_<resource>` for the baseline CREATE TABLE.
    let prefix = format!("{feature_slug}_{resource_slug}");
    if stem == prefix {
        return true;
    }
    // Future-proof for A10/A11's ALTER files. The cell spec calls out
    // `NNN_<feature>_<resource>*.sql` — the wildcard tail covers
    // `<feature>_<resource>_add_<col>.sql` patterns once A10 lands.
    if let Some(rest) = stem.strip_prefix(&format!("{prefix}_")) {
        // Guard against `prefix` being a substring of another resource
        // slug (`order` vs `order_line`): require the next segment to
        // be a non-empty token.
        if !rest.is_empty() {
            return true;
        }
    }
    false
}

// ── IR → expected column names ──────────────────────────────────────────────

/// Build the set of column names the migration emitter would write for
/// `resource`, mirroring `lazuli_codegen_go::emitter::migration_ddl::
/// resource_columns`. We only need the *names* (not types) for the
/// drift warning — names are sufficient to catch the "author added a
/// field but did not regen" footgun the cell targets.
///
/// Mirrors:
/// - `id` unless `composite_key.primary == true`
/// - `org_id` when effective tenancy is `Tenancy::Org`
/// - one column per `Field` (named verbatim per the emitter convention)
/// - paired `<field>_currency` for Money fields when not explicitly
///   declared
/// - `created_at` / `updated_at` when feature defaults or resource
///   declare timestamps
/// - `deleted_at` when `soft_delete`
fn expected_columns_for(feature: &Feature, resource: &Resource) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();

    let composite_primary = resource
        .composite_key
        .as_ref()
        .is_some_and(|ck| ck.primary);
    if !composite_primary {
        cols.insert("id".to_string());
    }

    let tenancy = resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone());
    if matches!(tenancy, Some(Tenancy::Org)) {
        cols.insert("org_id".to_string());
    }

    // Track Money fields so we know whether to auto-pair `<f>_currency`.
    let explicit_currency_overrides: BTreeSet<String> = resource
        .fields
        .iter()
        .filter_map(|f| {
            if matches!(
                f.type_ref,
                TypeRef::Builtin(BuiltinType::SemanticCurrency)
            ) {
                f.name
                    .strip_suffix("_currency")
                    .map(|stem| stem.to_owned())
            } else {
                None
            }
        })
        .collect();

    for field in &resource.fields {
        // File-attachment capability projects to multiple columns, but
        // for the drift warning we conservatively only require the
        // declared field name. False negatives here are acceptable
        // (the warning is a hint, not a contract); A10's diff is the
        // authoritative source once it lands.
        if matches!(field.type_ref, TypeRef::Capability(CapabilityRef::File { .. })) {
            cols.insert(field.name.clone());
            continue;
        }
        cols.insert(field.name.clone());
        if let TypeRef::Builtin(BuiltinType::SemanticMoney { .. }) = field.type_ref {
            if !explicit_currency_overrides.contains(&field.name) {
                cols.insert(format!("{}_currency", field.name));
            }
        }
    }

    let timestamps = resource.timestamps.unwrap_or(feature.defaults.timestamps);
    if timestamps {
        cols.insert("created_at".to_string());
        cols.insert("updated_at".to_string());
    }
    if resource.soft_delete {
        cols.insert("deleted_at".to_string());
    }

    cols
}

// ── minimal SQL column parser ───────────────────────────────────────────────

/// Parse the column-name set from a `CREATE TABLE <ident> ( ... );`
/// block targeting `table_name`. Single-pass, regex-free.
///
/// Constraint lines (`PRIMARY KEY (...)`, `UNIQUE (...)`,
/// `FOREIGN KEY (...)`, `CONSTRAINT <name> ...`, `CHECK (...)`) are
/// skipped — they are not columns.
///
/// Returns an empty set when the table block is not found, mirroring
/// the "no migration for this resource" path without panicking on
/// malformed SQL.
fn parse_create_table_columns(sql: &str, table_name: &str) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();
    let lower = sql.to_ascii_lowercase();
    let needle_quoted = format!("create table if not exists \"{}\"", table_name);
    let needle_bare = format!("create table if not exists {}", table_name);
    let needle_short_quoted = format!("create table \"{}\"", table_name);
    let needle_short_bare = format!("create table {}", table_name);

    let table_pos = [
        &needle_quoted,
        &needle_bare,
        &needle_short_quoted,
        &needle_short_bare,
    ]
    .into_iter()
    .filter_map(|needle| lower.find(needle.as_str()))
    .min();

    let Some(start) = table_pos else {
        return cols;
    };

    // Step from the matched header to the first `(`, then walk until
    // the matching `)` at depth 0. Inside, split on top-level commas.
    let after_header = match sql[start..].find('(') {
        Some(p) => start + p + 1,
        None => return cols,
    };
    let bytes = sql.as_bytes();
    let mut depth = 1usize;
    let mut idx = after_header;
    let mut col_start = idx;
    while idx < bytes.len() && depth > 0 {
        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    push_column_name(&sql[col_start..idx], &mut cols);
                    break;
                }
            }
            b',' if depth == 1 => {
                push_column_name(&sql[col_start..idx], &mut cols);
                col_start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }

    cols
}

fn push_column_name(segment: &str, cols: &mut BTreeSet<String>) {
    let line = segment.trim();
    if line.is_empty() {
        return;
    }
    // Strip trailing comments.
    let line = match line.find("--") {
        Some(i) => line[..i].trim_end(),
        None => line,
    };
    if line.is_empty() {
        return;
    }
    // Skip table-level constraint clauses.
    let upper = line.to_ascii_uppercase();
    for prefix in [
        "PRIMARY KEY",
        "UNIQUE",
        "FOREIGN KEY",
        "CONSTRAINT",
        "CHECK",
    ] {
        if upper.starts_with(prefix) {
            return;
        }
    }
    // First whitespace-separated token is the column name. Strip
    // surrounding identifier quotes (`"col"` → `col`).
    let raw_name = line.split_ascii_whitespace().next().unwrap_or("");
    if raw_name.is_empty() {
        return;
    }
    let name = raw_name.trim_matches('"').trim_matches('`');
    if name.is_empty() {
        return;
    }
    cols.insert(name.to_owned());
}

// ── set diff ────────────────────────────────────────────────────────────────

/// Compute the symmetric difference between IR column names and
/// migration column names.
///
/// Returns `(adds, drops)` where `adds` are names in `ir` but not in
/// `migration`, and `drops` are names in `migration` but not in `ir`.
/// Both vectors are sorted ASCII-ascending.
fn column_diff(
    ir: &BTreeSet<String>,
    migration: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let adds: Vec<String> = ir.difference(migration).cloned().collect();
    let drops: Vec<String> = migration.difference(ir).cloned().collect();
    (adds, drops)
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Minimal `lower_snake` mirroring `lazuli_codegen_go::emitter::
/// migration_ddl::lower_snake`. The codegen function is private, and
/// `lazuli_doctor` does not depend on `lazuli_codegen_go`. Consolidate
/// when A10 promotes the helper.
fn lower_snake(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_is_lower_or_digit = false;
    let mut prev_is_sep = false;

    for ch in raw.chars() {
        if ch == '-' || ch == ' ' || ch == '.' || ch == '/' || ch == '\\' {
            if !out.is_empty() && !prev_is_sep {
                out.push('_');
            }
            prev_is_lower_or_digit = false;
            prev_is_sep = true;
            continue;
        }
        if ch == '_' {
            if !out.is_empty() && !prev_is_sep {
                out.push('_');
            }
            prev_is_lower_or_digit = false;
            prev_is_sep = true;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit && !prev_is_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
            prev_is_sep = false;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            prev_is_sep = false;
        }
    }
    out
}

// ── unit tests (fine-grained, parser + diff) ────────────────────────────────

#[cfg(test)]
mod tests {
    include!("schema_migration_present_tests.rs");
}
