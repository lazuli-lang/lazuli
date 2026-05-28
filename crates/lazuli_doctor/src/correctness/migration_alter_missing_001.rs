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

/// One finding of `MIGRATION-ALTER-MISSING-001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` file the resource was authored in (diagnostic anchor).
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Resource name within the feature.
    pub resource: String,
    /// Why the rule fired — drives the message renderer.
    pub kind: FindingKind,
}

/// Variant payloads for [`Finding`]. The rule's two failure modes:
///
/// - [`FindingKind::MissingAlter`] — IR has columns not present in
///   `baseline ∪ forward_additions`. The high-confidence case.
/// - [`FindingKind::UnrecognisedMigration`] — the rule encountered
///   an `ALTER TABLE` form it could not parse (multi-column ADD,
///   transaction-wrapped). The rule's confidence is reduced; the
///   author audits manually. Per the proposal §"False-positive cases".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// IR has columns the deployment history will never apply.
    MissingAlter {
        /// Path of the baseline migration (the original CREATE TABLE).
        baseline_migration: PathBuf,
        /// Column names in IR but not added by any ALTER migration.
        /// Sorted ASCII-ascending for stable diagnostics.
        missing: Vec<String>,
    },
    /// A migration carried an `ALTER TABLE` shape the parser could not
    /// decode. Confidence in the main check is reduced for this
    /// resource; emit the warning so the author can opt-out with a
    /// reason or reshape the migration.
    UnrecognisedMigration {
        /// Path of the unparseable migration file.
        migration: PathBuf,
        /// Short text snippet showing the unrecognised line.
        snippet: String,
    },
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "MIGRATION-ALTER-MISSING-001";

    /// Default severity per the doctor profile defaults. The CLI
    /// dispatcher may override per `--profile`. v0.1 returns `Warning`
    /// (the `strict` default); production callers should escalate to
    /// `Error` and prototype callers may downgrade to `Info`.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::correctness::migration_alter_missing_001::Finding;
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    /// ```
    pub fn default_severity() -> DoctorSeverity {
        DoctorSeverity::Warning
    }

    /// Render the finding's diagnostic prose.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::migration_alter_missing_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("account.lzi"),
    ///     feature: "account".into(),
    ///     resource: "User".into(),
    ///     kind: FindingKind::MissingAlter {
    ///         baseline_migration: PathBuf::from("dist/go/migrations/004_account_user.sql"),
    ///         missing: vec!["updated_at".to_string()],
    ///     },
    /// };
    /// assert!(f.message().contains("updated_at"));
    /// assert!(f.message().contains("lazuli generate go ."));
    /// ```
    pub fn message(&self) -> String {
        match &self.kind {
            FindingKind::MissingAlter {
                baseline_migration,
                missing,
            } => format!(
                "resource '{feature}.{resource}' declares column(s) [{cols}] in the IR \
                 that no ALTER TABLE migration ever adds. Baseline at {baseline} has \
                 already been applied in production; editing it will not propagate. \
                 Run `lazuli generate go .` to emit an ALTER TABLE migration; do NOT \
                 modify the baseline file.",
                feature = self.feature,
                resource = self.resource,
                cols = missing.join(", "),
                baseline = baseline_migration.display(),
            ),
            FindingKind::UnrecognisedMigration { migration, snippet } => format!(
                "resource '{feature}.{resource}' has migration {migration} with an \
                 ALTER TABLE shape the doctor cannot parse ({snippet}). The \
                 MIGRATION-ALTER-MISSING-001 check is skipped for this resource. \
                 Reshape the migration to single-column ALTER ADD COLUMN, or add \
                 `# doctor:allow MIGRATION-ALTER-MISSING-001 — reason \"...\"` to \
                 the resource's .lzi after manual verification.",
                feature = self.feature,
                resource = self.resource,
                migration = migration.display(),
                snippet = snippet,
            ),
        }
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `MIGRATION-ALTER-MISSING-001` for every resource in `feature`,
/// anchored at the capsule root `root` (the directory that holds
/// `dist/go/migrations/`).
///
/// `feature_path` is the diagnostic anchor (typically the `.lzi`
/// file). It is also scanned for the `# doctor:allow
/// MIGRATION-ALTER-MISSING-001` opt-out — when present, no findings
/// are emitted for any resource in this feature file.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::migration_alter_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources");
/// let _ = check(&feature, Path::new("account.lzi"), Path::new("/app"));
/// ```
pub fn check(feature: &Feature, feature_path: &Path, root: &Path) -> Vec<Finding> {
    // Whole-file opt-out — silence ALL findings from this feature
    // when the `.lzi` carries the canonical allow comment.
    if file_contains_doctor_allow(feature_path, Finding::CODE) {
        return Vec::new();
    }

    let migrations_dir = root.join("dist").join("go").join("migrations");
    let entries = list_migration_entries(&migrations_dir);
    let feature_slug = lower_snake(&feature.name);

    let mut out = Vec::new();
    for resource in &feature.resources {
        let resource_slug = lower_snake(&resource.name);
        let prefix = format!("{feature_slug}_{resource_slug}");
        let table = lower_snake(&resource.name);

        // Filter + sort migrations by NNN ascending.
        let mut owned: Vec<&MigrationEntry> = entries
            .iter()
            .filter(|e| migration_matches(&e.stem, &prefix))
            .collect();
        owned.sort_by_key(|e| e.number);

        // No migrations for the resource — sibling rule
        // (@correctness.migration_out_of_sync) owns MigrationMissing.
        let Some(baseline) = owned.first() else {
            continue;
        };

        let Ok(baseline_sql) = fs::read_to_string(&baseline.path) else {
            continue;
        };
        let baseline_cols = parse_create_table_columns(&baseline_sql, &table);
        let mut deployed: BTreeSet<String> = baseline_cols;

        // Walk subsequent migrations for ALTER ADD COLUMN additions.
        // If we see an `ALTER TABLE` shape we cannot parse for this
        // table, emit UnrecognisedMigration and skip the MissingAlter
        // check (reduced confidence — proposal §"False-positive cases").
        let mut unrecognised: Option<(PathBuf, String)> = None;
        for entry in &owned[1..] {
            let Ok(sql) = fs::read_to_string(&entry.path) else {
                continue;
            };
            match parse_alter_add_columns(&sql, &table) {
                AlterParseResult::Parsed(cols) => {
                    for c in cols {
                        deployed.insert(c);
                    }
                }
                AlterParseResult::Unrecognised(snippet) => {
                    unrecognised = Some((entry.path.clone(), snippet));
                    break;
                }
            }
        }

        if let Some((path, snippet)) = unrecognised {
            out.push(Finding {
                path: feature_path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                kind: FindingKind::UnrecognisedMigration {
                    migration: path,
                    snippet,
                },
            });
            continue;
        }

        let ir_cols = expected_columns_for(feature, resource);
        let missing: Vec<String> = ir_cols.difference(&deployed).cloned().collect();
        if missing.is_empty() {
            continue;
        }

        out.push(Finding {
            path: feature_path.to_path_buf(),
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            kind: FindingKind::MissingAlter {
                baseline_migration: baseline.path.clone(),
                missing,
            },
        });
    }
    out
}

// ── migration directory walk ────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MigrationEntry {
    path: PathBuf,
    number: u32,
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
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        let trimmed = name.trim_end_matches(".sql");
        let Some((number_text, stem)) = trimmed.split_once('_') else {
            continue;
        };
        let Ok(number) = number_text.parse::<u32>() else {
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

/// Owner-test mirrors [`crate::correctness::schema_migration_present::
/// migration_matches`]. Match when the stem is exactly the prefix
/// (baseline shape) or carries a non-empty `_<tail>` (ALTER shape).
fn migration_matches(stem: &str, prefix: &str) -> bool {
    if stem == prefix {
        return true;
    }
    if let Some(rest) = stem.strip_prefix(&format!("{prefix}_")) {
        if !rest.is_empty() {
            return true;
        }
    }
    false
}

// ── IR → expected column names (mirror schema_migration_present) ────────────

fn expected_columns_for(feature: &Feature, resource: &Resource) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();

    let composite_primary = resource.composite_key.as_ref().is_some_and(|ck| ck.primary);
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

    let explicit_currency_overrides: BTreeSet<String> = resource
        .fields
        .iter()
        .filter_map(|f| {
            if matches!(f.type_ref, TypeRef::Builtin(BuiltinType::SemanticCurrency)) {
                f.name.strip_suffix("_currency").map(|stem| stem.to_owned())
            } else {
                None
            }
        })
        .collect();

    for field in &resource.fields {
        if matches!(
            field.type_ref,
            TypeRef::Capability(CapabilityRef::File { .. })
        ) {
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

// ── minimal SQL CREATE TABLE column parser ──────────────────────────────────

/// See [`crate::correctness::schema_migration_present::
/// parse_create_table_columns`] for the canonical version this mirrors.
/// Duplicated to keep the sibling rule self-contained until A10 lands
/// the shared helper.
fn parse_create_table_columns(sql: &str, table_name: &str) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();
    let lower = sql.to_ascii_lowercase();
    let needles = [
        format!("create table if not exists \"{}\"", table_name),
        format!("create table if not exists {}", table_name),
        format!("create table \"{}\"", table_name),
        format!("create table {}", table_name),
    ];

    let Some(start) = needles.iter().filter_map(|n| lower.find(n.as_str())).min() else {
        return cols;
    };

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
    let line = match line.find("--") {
        Some(i) => line[..i].trim_end(),
        None => line,
    };
    if line.is_empty() {
        return;
    }
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

// ── ALTER TABLE ADD COLUMN parser ───────────────────────────────────────────

/// Result of scanning a migration body for ALTER TABLE ADD COLUMN
/// targeting the given table.
#[derive(Debug, Clone)]
enum AlterParseResult {
    /// Successfully parsed all ALTER lines for this table. May be
    /// empty if the migration targets a different table (e.g. an
    /// index-only or unrelated DDL file).
    Parsed(Vec<String>),
    /// Encountered an ALTER form the parser cannot decode.
    Unrecognised(String),
}

/// Parse `ALTER TABLE <ident> ADD COLUMN <ident> <type>` lines from
/// `sql`, matching ONLY when the target table matches `table_name`.
///
/// Recognised shapes:
///
/// - `ALTER TABLE "<table>" ADD COLUMN "<col>" <type> ...;`
/// - `ALTER TABLE <table> ADD COLUMN <col> <type> ...;`
/// - `ALTER TABLE "<table>" ADD COLUMN IF NOT EXISTS "<col>" ...;`
/// - `ALTER TABLE <table> ADD COLUMN IF NOT EXISTS <col> ...;`
///
/// Out-of-scope (return [`AlterParseResult::Unrecognised`]):
///
/// - Multi-column `ALTER TABLE x ADD COLUMN a INT, ADD COLUMN b TEXT;`
/// - Transaction-wrapped (`BEGIN; ALTER TABLE ...; COMMIT;`) when the
///   ALTER itself uses multi-column syntax
/// - `ALTER TABLE ... ADD CONSTRAINT ...` is silently skipped (not
///   column-shape drift).
fn parse_alter_add_columns(sql: &str, table_name: &str) -> AlterParseResult {
    let mut out = Vec::new();
    let lower = sql.to_ascii_lowercase();
    let target_quoted = format!("alter table \"{}\"", table_name);
    let target_bare = format!("alter table {}", table_name);

    let mut cursor = 0usize;
    while cursor < lower.len() {
        let rest = &lower[cursor..];
        let next_alter = match rest.find("alter table ") {
            Some(p) => cursor + p,
            None => break,
        };

        // Determine the ALTER statement's table — we only care about
        // ones targeting `table_name`. Other ALTERs (e.g. ALTER on a
        // sibling table) are skipped silently.
        let header = &lower[next_alter..];
        let owns_target = header.starts_with(&target_quoted)
            || (header.starts_with(&target_bare)
                && header
                    .as_bytes()
                    .get(target_bare.len())
                    .map(|b| !b.is_ascii_alphanumeric() && *b != b'_')
                    .unwrap_or(true));

        // Find end of statement (`;`) — confines the parse window.
        let stmt_end = match lower[next_alter..].find(';') {
            Some(p) => next_alter + p,
            None => lower.len(),
        };

        if !owns_target {
            cursor = stmt_end + 1;
            continue;
        }

        let stmt = &sql[next_alter..stmt_end];
        let stmt_lower = &lower[next_alter..stmt_end];

        // Skip ADD CONSTRAINT (not column-shape).
        if stmt_lower.contains("add constraint") && !stmt_lower.contains("add column") {
            cursor = stmt_end + 1;
            continue;
        }

        // Reject multi-column shape: more than one `add column` token
        // inside the same statement → unrecognised.
        let add_col_count = stmt_lower.matches("add column").count();
        if add_col_count == 0 {
            // ALTER targeting our table but no ADD COLUMN — likely a
            // DROP / ALTER COLUMN TYPE / RENAME. Out-of-scope for v0.1
            // but not an error for this rule; skip silently.
            cursor = stmt_end + 1;
            continue;
        }
        if add_col_count > 1 {
            let snippet = first_line(stmt).to_owned();
            return AlterParseResult::Unrecognised(snippet);
        }

        // Single-column ADD COLUMN — extract the column identifier.
        // SAFETY: `add_col_count` was checked to be exactly 1 above
        // (`stmt_lower.matches("add column").count()` returned 1), so
        // `stmt_lower.find("add column")` MUST return Some here.
        let Some(add_col_pos) = stmt_lower.find("add column") else {
            // Defensive: if the invariant were ever violated we treat
            // the statement as unrecognised rather than panicking.
            let snippet = first_line(stmt).to_owned();
            return AlterParseResult::Unrecognised(snippet);
        };
        let mut after = &stmt[add_col_pos + "add column".len()..];
        after = after.trim_start();
        // Optional `IF NOT EXISTS`.
        let after_lower = after.to_ascii_lowercase();
        if after_lower.starts_with("if not exists") {
            after = after["if not exists".len()..].trim_start();
        }
        // Extract first token (quoted or bare).
        let token = first_ident(after);
        if token.is_empty() {
            let snippet = first_line(stmt).to_owned();
            return AlterParseResult::Unrecognised(snippet);
        }
        out.push(token);

        cursor = stmt_end + 1;
    }
    AlterParseResult::Parsed(out)
}

/// First whitespace-separated identifier from `s`, stripping
/// surrounding double quotes / backticks.
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
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .unwrap_or("");
    token.trim_matches('"').trim_matches('`').to_owned()
}

fn first_line(s: &str) -> &str {
    match s.find('\n') {
        Some(i) => &s[..i],
        None => s,
    }
}

// ── lower_snake (mirror schema_migration_present) ────────────────────────────

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

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{BuiltinType, Defaults, Field, FieldConstraints, Policies, Resource, TypeRef};
    use std::fs;
    use tempfile::TempDir;

    // ── IR fixture builders ──────────────────────────────────────────────

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>, timestamps: Option<bool>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "account".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    /// Write `dist/go/migrations/<name>` under `root`, creating parents.
    fn write_migration(root: &Path, name: &str, body: &str) -> PathBuf {
        let dir = root.join("dist").join("go").join("migrations");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    // ── positive case ────────────────────────────────────────────────────

    #[test]
    fn positive_ir_adds_column_no_alter_migration_fires() {
        let tmp = TempDir::new().unwrap();
        // Baseline CREATE without `updated_at`. IR has timestamps=true.
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        match &findings[0].kind {
            FindingKind::MissingAlter { missing, .. } => {
                assert!(missing.contains(&"updated_at".to_string()), "{missing:?}");
            }
            other => panic!("expected MissingAlter, got {other:?}"),
        }
        assert_eq!(Finding::CODE, "MIGRATION-ALTER-MISSING-001");
        assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    }

    // ── baseline already has column → silent ────────────────────────────

    #[test]
    fn negative_baseline_contains_column_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL,\n  updated_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── ALTER migration adds the column → silent ────────────────────────

    #[test]
    fn negative_alter_exists_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        write_migration(
            tmp.path(),
            "005_account_user_add_updated_at.sql",
            "ALTER TABLE \"user\" ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── multi-column ALTER → UnrecognisedMigration, not false-fire ──────

    #[test]
    fn negative_unrecognised_alter_form_does_not_fire() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL\n);\n",
        );
        write_migration(
            tmp.path(),
            "005_account_user_add_misc.sql",
            "ALTER TABLE \"user\" ADD COLUMN a INT, ADD COLUMN b TEXT;\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name"), mk_field("a"), mk_field("b")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        match &findings[0].kind {
            FindingKind::UnrecognisedMigration { snippet, .. } => {
                assert!(
                    snippet.to_ascii_lowercase().contains("alter table"),
                    "{snippet}"
                );
            }
            other => panic!("expected UnrecognisedMigration, got {other:?}"),
        }
    }

    // ── allow-comment opt-out silences the rule ─────────────────────────

    #[test]
    fn negative_allow_comment_silences() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(
            &lzi,
            "# doctor:allow MIGRATION-ALTER-MISSING-001 — reason \"manual verify\"\nfeature account\n",
        )
        .unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── no migrations on disk → sibling rule's job, silent here ─────────

    #[test]
    fn negative_no_migrations_silent() {
        let tmp = TempDir::new().unwrap();
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── ALTER parser unit tests ─────────────────────────────────────────

    #[test]
    fn alter_parser_picks_single_column_add() {
        let r = parse_alter_add_columns(
            "ALTER TABLE \"user\" ADD COLUMN updated_at TIMESTAMPTZ NOT NULL;",
            "user",
        );
        match r {
            AlterParseResult::Parsed(cols) => {
                assert_eq!(cols, vec!["updated_at".to_string()]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_recognises_if_not_exists() {
        let r = parse_alter_add_columns(
            "ALTER TABLE user ADD COLUMN IF NOT EXISTS phone TEXT;",
            "user",
        );
        match r {
            AlterParseResult::Parsed(cols) => assert_eq!(cols, vec!["phone".to_string()]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_ignores_other_tables() {
        let r = parse_alter_add_columns("ALTER TABLE other_table ADD COLUMN foo INT;", "user");
        match r {
            AlterParseResult::Parsed(cols) => assert!(cols.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_flags_multi_column_as_unrecognised() {
        let r = parse_alter_add_columns(
            "ALTER TABLE \"user\" ADD COLUMN a INT, ADD COLUMN b TEXT;",
            "user",
        );
        assert!(matches!(r, AlterParseResult::Unrecognised(_)), "{r:?}");
    }
}
