//! MIGRATION-DSL-UNIQUE-001 — `.lzi` `unique` field has no `UNIQUE` constraint in migration.
//!
//! Catches LAZ-MIGRATION-DSL-UNIQUE — codegen drops the `unique` keyword from
//! scalar field declarations when emitting migration DDL, so handlers using
//! `ON CONFLICT (<col>) DO UPDATE` fail at runtime with SQLSTATE 42P10. The
//! field author declared the constraint; the migration must materialize it.
//!
//! Detection (v0.1 — regex-based file scan, deliberately simple):
//!
//! 1. Walk every `Resource.fields` whose `unique == true`.
//! 2. Locate the migration `.sql` files at `<project_root>/migrations/`.
//! 3. For each unique field, scan the SQL text for any of:
//!    * `UNIQUE` keyword followed by `(<col>)` (column-list constraint)
//!    * `CREATE UNIQUE INDEX ... ON ... (<col>)`
//!    * inline `<col> ... UNIQUE` on a column declaration
//! 4. Fire when no migration carries the constraint for the column.
//!
//! v0.1 limitation: text-pattern scan over migration SQL. A future v0.2 should
//! parse the DDL AST to handle multi-column unique constraints, partial
//! indexes, and renamed-table cases. For LAZ-MIGRATION-DSL-UNIQUE we only
//! need to confirm the keyword reaches DDL at all.
//!
//! Severity: `error` (strict + production). The bug causes runtime 500s.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One MIGRATION-DSL-UNIQUE-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` source path that declares the resource.
    pub path: PathBuf,
    /// Feature containing the resource.
    pub feature: String,
    /// Resource that owns the unique field.
    pub resource: String,
    /// Field name as authored.
    pub field: String,
    /// SQL column name (snake_case of `field`).
    pub column: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "MIGRATION-DSL-UNIQUE-001";

    /// Render the user-facing diagnostic body — names the column and
    /// the SQLSTATE 42P10 runtime failure that drops the constraint.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::migration_dsl_unique_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messaging.lzi"),
    ///     feature: "messaging".into(),
    ///     resource: "Subscription".into(),
    ///     field: "endpoint".into(),
    ///     column: "endpoint".into(),
    /// };
    /// assert!(f.message().contains("endpoint"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` declares `unique` in `.lzi` but no migration SQL \
             materializes a `UNIQUE` constraint on column `{}`. Handlers using \
             `ON CONFLICT ({}) DO UPDATE` will fail with SQLSTATE 42P10.",
            self.resource, self.field, self.column, self.column
        )
    }
}

/// Run MIGRATION-DSL-UNIQUE-001 over a feature. Reads every
/// `<project_root>/migrations/*.sql` file and short-circuits to empty
/// when the directory is absent or empty (no false positives before
/// any migrations are authored).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::migration_dsl_unique_001::check;
///
/// let findings = check(&feature, Path::new("messaging.lzi"), Path::new("/proj"));
/// ```
pub fn check(feature: &Feature, source_path: &Path, project_root: &Path) -> Vec<Finding> {
    let migrations_dir = project_root.join("migrations");
    if !migrations_dir.is_dir() {
        return Vec::new();
    }
    let migration_sql = read_all_migrations(&migrations_dir);
    if migration_sql.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for resource in &feature.resources {
        for field in &resource.fields {
            if !field.unique {
                continue;
            }
            let column = field.name.clone();
            if !has_unique_constraint(&migration_sql, &column) {
                findings.push(Finding {
                    path: source_path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    column,
                });
            }
        }
    }
    findings
}

fn read_all_migrations(dir: &Path) -> String {
    let Ok(entries) = fs::read_dir(dir) else {
        return String::new();
    };
    let mut out = String::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}

/// Conservative scan for any of the three canonical unique-constraint shapes.
/// Case-insensitive on the SQL keyword (`UNIQUE`); column name match is exact
/// (Postgres convention treats unquoted identifiers as lowercase but we only
/// emit lower-snake-case columns, so exact match is safe in v0.1).
fn has_unique_constraint(sql: &str, column: &str) -> bool {
    // Strategy: search every occurrence of "unique" (case-insensitive) and
    // check if the column appears within a small window after it.
    let lower = sql.to_ascii_lowercase();
    let needle = "unique";
    let mut start = 0usize;
    let col_lower = column.to_ascii_lowercase();
    while let Some(pos) = lower[start..].find(needle) {
        let abs = start + pos;
        // Window: look ahead up to 200 chars for `(`...col...`)` OR look back
        // up to 80 chars for `<col>` inline before the keyword.
        let look_ahead_end = (abs + needle.len() + 200).min(lower.len());
        let after = &lower[abs + needle.len()..look_ahead_end];
        if window_contains_column(after, &col_lower) {
            return true;
        }
        let look_back_start = abs.saturating_sub(80);
        let before = &lower[look_back_start..abs];
        if before.contains(&col_lower) && inline_column_match(before, &col_lower) {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

fn window_contains_column(window: &str, col: &str) -> bool {
    // We want a column reference inside parentheses or just after the keyword.
    // Allow optional quotes around the column.
    let quoted = format!("\"{col}\"");
    if window.contains(&format!("({col}")) || window.contains(&format!(", {col}")) {
        return true;
    }
    if window.contains(&format!("({quoted}")) || window.contains(&format!(", {quoted}")) {
        return true;
    }
    // Bare keyword followed by identifier (e.g. `CREATE UNIQUE INDEX foo ON t (col)`)
    // — search for `(col)` or `(col ` or `(col,` anywhere in the window.
    for prefix in ["(", " "] {
        for suffix in [")", ",", " "] {
            if window.contains(&format!("{prefix}{col}{suffix}")) {
                return true;
            }
            if window.contains(&format!("{prefix}{quoted}{suffix}")) {
                return true;
            }
        }
    }
    false
}

fn inline_column_match(before: &str, col: &str) -> bool {
    // Inline column declaration: `<col> <type> ... UNIQUE`. The column is on
    // the same line as the keyword, separated by whitespace and possibly
    // type/modifiers. We look for the column name as a whole token within the
    // preceding window.
    let tokens: Vec<&str> = before
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .collect();
    tokens.contains(&col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{BuiltinType, Defaults, Field, Policies, Resource, TypeRef};
    use tempfile::tempdir;

    fn mk_resource_with_unique_field(field_name: &str) -> Resource {
        Resource {
            name: "Subscription".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![Field {
                name: field_name.to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Text),
                required: true,
                unique: true,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: Default::default(),
                full_text: false,
                previous_names: vec![],
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            }],
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
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resource: Resource) -> Feature {
        Feature {
            name: "messaging".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![resource],
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

    #[test]
    fn missing_unique_in_migration_fires() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir(&migrations).unwrap();
        fs::write(
            migrations.join("001_init.sql"),
            "CREATE TABLE subscription (endpoint TEXT NOT NULL);",
        )
        .unwrap();
        let feature = mk_feature(mk_resource_with_unique_field("endpoint"));
        let findings = check(&feature, Path::new("messaging.lzi"), dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].column, "endpoint");
    }

    #[test]
    fn unique_inline_in_migration_clears() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir(&migrations).unwrap();
        fs::write(
            migrations.join("001_init.sql"),
            "CREATE TABLE subscription (endpoint TEXT NOT NULL UNIQUE);",
        )
        .unwrap();
        let feature = mk_feature(mk_resource_with_unique_field("endpoint"));
        assert!(check(&feature, Path::new("messaging.lzi"), dir.path()).is_empty());
    }

    #[test]
    fn unique_constraint_clause_in_migration_clears() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir(&migrations).unwrap();
        fs::write(
            migrations.join("002_uniq.sql"),
            "ALTER TABLE subscription ADD CONSTRAINT sub_endpoint_uniq UNIQUE (endpoint);",
        )
        .unwrap();
        let feature = mk_feature(mk_resource_with_unique_field("endpoint"));
        assert!(check(&feature, Path::new("messaging.lzi"), dir.path()).is_empty());
    }

    #[test]
    fn create_unique_index_in_migration_clears() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir(&migrations).unwrap();
        fs::write(
            migrations.join("003_index.sql"),
            "CREATE UNIQUE INDEX sub_endpoint_uniq ON subscription (endpoint);",
        )
        .unwrap();
        let feature = mk_feature(mk_resource_with_unique_field("endpoint"));
        assert!(check(&feature, Path::new("messaging.lzi"), dir.path()).is_empty());
    }

    #[test]
    fn no_migrations_dir_does_not_fire() {
        let dir = tempdir().unwrap();
        let feature = mk_feature(mk_resource_with_unique_field("endpoint"));
        // No `migrations/` subdir — rule short-circuits.
        assert!(check(&feature, Path::new("messaging.lzi"), dir.path()).is_empty());
    }
}
