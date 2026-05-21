//! Schema diff algorithm for incremental migration emission.
//!
//! Today every `CREATE TABLE IF NOT EXISTS` migration we emit is a
//! no-op against a database that already has the table — adding a
//! column to a resource and re-running `lazuli generate` rebuilds
//! the same `CREATE TABLE` block with the new column wedged into the
//! middle, but PostgreSQL skips the create entirely (the table
//! exists), so the new column never lands. Pilots have had to
//! `ALTER TABLE ... ADD COLUMN` by hand. This cell (A10) delivers
//! the schema-diff primitive; cell A11 consumes it to emit ALTER
//! SQL and cell A12 adds the doctor lint that prevents silent
//! divergence.
//!
//! Scope of this file:
//! - `Column` / `TypeChange` / `ResourceSchema` / `SchemaDiff` types.
//! - `diff(baseline, current)` — pure comparison.
//! - `parse_baseline_from_migration(path)` — recovers the last
//!   emitted column shape from a generated `.sql` file. The parser
//!   is hand-rolled because (a) we control the emitter's output
//!   format, so a full SQL parser is overkill, and (b) `lazuli`
//!   already has zero SQL-parser dependency and we want to keep it
//!   that way.
//! - `current_schema_from_ir(resource)` — projects an IR resource
//!   into a `ResourceSchema`, mirroring the type mapping used by
//!   `migration_ddl.rs` (`pg_type_for` / `pg_type_for_builtin`).
//!   We deliberately do NOT do FK resolution here — the cell A11
//!   ALTER emitter never re-targets a foreign key without an
//!   explicit migration plan, so columns whose type is
//!   `TypeRef::UserDefined` that would normally lower to `BIGINT`
//!   are emitted as their nominal IR mapping. Callers that need
//!   the cross-feature view stay on `migration_ddl::pg_type_for_field`.
//!
//! Out of scope for this cell:
//! - Emitting the ALTER SQL (cell A11, separate file `migration.rs`).
//! - Doctor lint that warns when ALTER coverage is missing (A12).
//! - Index diffs (separate primitive, future cell).
//! - Constraint diffs (the parser intentionally skips them; A11's
//!   ALTER emission also stays inside the column dimension for v1).

use std::fs;
use std::io;
use std::path::Path;

use lazuli_ir::{BuiltinType, CapabilityRef, DefaultValue, Field, Resource, TypeRef};

/// A single column extracted either from a generated migration or
/// from an IR resource. The shape is intentionally the lowest
/// common denominator across the two sources: name, SQL type, NOT
/// NULL bit, optional `DEFAULT` literal. Generated-as expressions
/// and trailing comments are dropped on the parser side — the
/// diff is shape-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

impl Column {
    pub fn new(name: impl Into<String>, sql_type: impl Into<String>, nullable: bool) -> Self {
        Self {
            name: name.into(),
            sql_type: sql_type.into(),
            nullable,
            default: None,
        }
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// A column whose SQL type changed between the baseline migration
/// and the current IR. Nullability flips and default flips are
/// captured separately by A11 as part of the ALTER plan; this
/// struct is the dimension that matters for `ALTER COLUMN TYPE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeChange {
    pub column: String,
    pub old_type: String,
    pub new_type: String,
}

/// Column-level shape of a resource at one point in time. Order is
/// preserved (matches the order columns appear in the migration or
/// in the IR) so callers that want stable error messages can index
/// by position. The diff itself is order-insensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceSchema {
    pub columns: Vec<Column>,
}

impl ResourceSchema {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    fn find(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// The output of `diff(baseline, current)`. `adds` are columns the
/// IR has that the baseline migration does not; `drops` are the
/// reverse; `type_changes` are columns present in both but whose
/// SQL type does not match. Nullability and default flips are
/// reserved for A11 (they require `ALTER COLUMN SET NOT NULL` /
/// `ALTER COLUMN SET DEFAULT` clauses that A11 handles in the
/// emitter, not here).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDiff {
    pub adds: Vec<Column>,
    pub drops: Vec<Column>,
    pub type_changes: Vec<TypeChange>,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.adds.is_empty() && self.drops.is_empty() && self.type_changes.is_empty()
    }
}

/// Pure diff. Treat both inputs as unordered column sets keyed by
/// name. SQL type comparison is case-sensitive after normalization
/// — both the parser and `current_schema_from_ir` emit uppercase
/// type names matching `migration_ddl::pg_type_for_builtin`, so a
/// type mismatch is always a semantic shift (e.g. `TEXT` ↔ `JSONB`)
/// rather than a `text` vs `TEXT` formatting drift.
pub fn diff(baseline: &ResourceSchema, current: &ResourceSchema) -> SchemaDiff {
    let mut out = SchemaDiff::default();

    for current_col in &current.columns {
        match baseline.find(&current_col.name) {
            None => out.adds.push(current_col.clone()),
            Some(baseline_col) => {
                if baseline_col.sql_type != current_col.sql_type {
                    out.type_changes.push(TypeChange {
                        column: current_col.name.clone(),
                        old_type: baseline_col.sql_type.clone(),
                        new_type: current_col.sql_type.clone(),
                    });
                }
            }
        }
    }

    for baseline_col in &baseline.columns {
        if current.find(&baseline_col.name).is_none() {
            out.drops.push(baseline_col.clone());
        }
    }

    out
}

/// Errors from `parse_baseline_from_migration`. Kept tight — every
/// generated migration is well-formed by construction, so reaching
/// any of these variants means either an externally hand-edited
/// file or a codegen-format drift the parser needs to catch up to.
#[derive(Debug)]
pub enum ParseError {
    Io(io::Error),
    MissingCreateTable,
    UnterminatedCreateTable,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Io(err) => write!(f, "read migration: {err}"),
            ParseError::MissingCreateTable => {
                write!(f, "migration has no `CREATE TABLE` block")
            }
            ParseError::UnterminatedCreateTable => {
                write!(f, "`CREATE TABLE` block is not closed by `);`")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<io::Error> for ParseError {
    fn from(err: io::Error) -> Self {
        ParseError::Io(err)
    }
}

/// Parse the column list out of the last emitted migration for a
/// resource. The generated format (see `migration_ddl.rs`) looks
/// like:
///
/// ```sql
/// -- Code generated by lazuli; DO NOT EDIT.
/// -- source: <label>
/// -- resource: <feature>.<resource>
///
/// CREATE TABLE IF NOT EXISTS "host" (
///     id BIGSERIAL PRIMARY KEY,
///     org_id BIGINT NOT NULL,
///     name TEXT NOT NULL, -- optional trailing comment
///     address TEXT
/// );
/// ```
///
/// The parser:
/// - skips the leading comment block,
/// - finds the `CREATE TABLE` (with or without `IF NOT EXISTS`),
/// - walks the parenthesized body line by line,
/// - strips trailing `-- ...` comments and the line's trailing
///   comma,
/// - rejects lines that are obviously not column definitions
///   (constraint tails like `PRIMARY KEY (...)` / `UNIQUE (...)` /
///   `FOREIGN KEY (...)` / `CHECK (...)`).
///
/// The `id BIGSERIAL PRIMARY KEY` row identity column IS captured
/// — it shows up in the IR's projection too (see
/// `current_schema_from_ir`), so symmetric capture keeps the diff
/// from spuriously dropping it.
pub fn parse_baseline_from_migration(path: &Path) -> Result<ResourceSchema, ParseError> {
    let contents = fs::read_to_string(path)?;
    parse_baseline_from_str(&contents)
}

/// Test-friendly variant of `parse_baseline_from_migration` that
/// takes the SQL string directly. Exposed `pub` so callers (the
/// A11 emitter, the A12 doctor lint, integration tests) can avoid
/// round-tripping through tempfiles when they already have the
/// SQL in memory.
pub fn parse_baseline_from_str(sql: &str) -> Result<ResourceSchema, ParseError> {
    // Find the `CREATE TABLE` line. Be liberal: accept any
    // whitespace, optional `IF NOT EXISTS`, and the table ident
    // (we don't need to remember the table name — the caller knows
    // which resource it's parsing).
    let lines: Vec<&str> = sql.lines().collect();
    let start = lines
        .iter()
        .position(|line| line_starts_create_table(line))
        .ok_or(ParseError::MissingCreateTable)?;

    // Body starts on the line after CREATE TABLE (the spec's
    // generated format always puts the `(` on the same line as
    // CREATE TABLE; handle both cases just in case the format
    // drifts a single space).
    let body_lines = &lines[start + 1..];

    let mut columns: Vec<Column> = Vec::new();
    let mut terminated = false;

    for raw in body_lines {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with(");") || line == ")" {
            terminated = true;
            break;
        }

        // Strip trailing inline comment first (`col TEXT, -- foo`).
        // The emitter places the comma before the `--`, so by the
        // time we drop the comment the trailing comma is preserved
        // and stripped below.
        let without_comment = match line.find("--") {
            Some(idx) => line[..idx].trim_end(),
            None => line,
        };

        // Drop the trailing comma if present. Constraints inside
        // the CREATE TABLE body also end with commas; we filter
        // those out by inspecting the body keyword.
        let body = without_comment
            .strip_suffix(',')
            .unwrap_or(without_comment)
            .trim();

        if body.is_empty() {
            continue;
        }
        if is_table_level_constraint(body) {
            continue;
        }

        if let Some(column) = parse_column_line(body) {
            columns.push(column);
        }
    }

    if !terminated {
        return Err(ParseError::UnterminatedCreateTable);
    }

    Ok(ResourceSchema { columns })
}

fn line_starts_create_table(line: &str) -> bool {
    let trimmed = line.trim_start().to_ascii_uppercase();
    trimmed.starts_with("CREATE TABLE")
}

/// Recognize the closed catalog of table-level constraint
/// keywords the emitter writes on dedicated lines inside the
/// CREATE TABLE body. Kept as a simple prefix match — the emitter
/// always starts these lines with the keyword.
fn is_table_level_constraint(body: &str) -> bool {
    let upper = body.to_ascii_uppercase();
    upper.starts_with("PRIMARY KEY ")
        || upper.starts_with("PRIMARY KEY(")
        || upper.starts_with("UNIQUE ")
        || upper.starts_with("UNIQUE(")
        || upper.starts_with("FOREIGN KEY ")
        || upper.starts_with("FOREIGN KEY(")
        || upper.starts_with("CHECK ")
        || upper.starts_with("CHECK(")
        || upper.starts_with("CONSTRAINT ")
}

/// Parse a single column-definition line. The emitter renders
/// columns as:
///
/// - `id BIGSERIAL PRIMARY KEY`
/// - `org_id BIGINT NOT NULL`
/// - `name TEXT NOT NULL DEFAULT 'unknown'`
/// - `address TEXT`
/// - `payload JSONB`
///
/// We tokenize on whitespace, take the first token as the name,
/// then walk the remaining tokens to find the SQL type (one or
/// more tokens until we hit a structural keyword like `NOT`,
/// `NULL`, `DEFAULT`, `PRIMARY`, `REFERENCES`, `CHECK`, `UNIQUE`,
/// `GENERATED`, or `COLLATE`). Identifier quoting (`"id"`) is
/// stripped for the diff-key.
fn parse_column_line(body: &str) -> Option<Column> {
    let mut tokens = body.split_whitespace();
    let raw_name = tokens.next()?;
    let name = strip_ident_quotes(raw_name).to_owned();
    if name.is_empty() {
        return None;
    }

    let mut sql_type_parts: Vec<String> = Vec::new();
    let mut nullable = true;
    let mut default: Option<String> = None;

    // Re-tokenize so we can do lookahead for multi-word tokens
    // like `NOT NULL` and `DEFAULT <expr>`.
    let rest: Vec<&str> = tokens.collect();
    let mut idx = 0usize;
    while idx < rest.len() {
        let tok = rest[idx];
        let upper = tok.to_ascii_uppercase();
        match upper.as_str() {
            "NOT" => {
                // Either `NOT NULL` (column is non-nullable) or
                // `NOT DEFERRABLE` (constraint clause we don't
                // care about). Either way we stop accumulating
                // the type.
                if idx + 1 < rest.len() && rest[idx + 1].eq_ignore_ascii_case("NULL") {
                    nullable = false;
                    idx += 2;
                    continue;
                }
                idx += 1;
            }
            "NULL" => {
                // Bare `NULL` keyword — explicit nullable. Type
                // is already accumulated.
                idx += 1;
            }
            "DEFAULT" => {
                // Capture the default expression as the remaining
                // tokens up to a recognized terminator. The
                // emitter's defaults are short (`0`, `now()`,
                // `''`, `NOW()`); we don't need to be clever.
                let mut expr: Vec<String> = Vec::new();
                idx += 1;
                while idx < rest.len() {
                    let upper_next = rest[idx].to_ascii_uppercase();
                    if matches!(
                        upper_next.as_str(),
                        "CHECK" | "REFERENCES" | "UNIQUE" | "PRIMARY" | "GENERATED" | "COLLATE"
                    ) {
                        break;
                    }
                    expr.push(rest[idx].to_owned());
                    idx += 1;
                }
                if !expr.is_empty() {
                    default = Some(expr.join(" "));
                }
            }
            "PRIMARY" | "REFERENCES" | "CHECK" | "UNIQUE" | "GENERATED" | "COLLATE" => {
                // Structural suffix — stop type accumulation.
                // We don't care about the rest of the line for
                // the diff; A11 will rebuild the suffix from the
                // IR when it has to.
                break;
            }
            _ => {
                if default.is_some() {
                    // Already past the DEFAULT clause; nothing
                    // else belongs to the type.
                    break;
                }
                sql_type_parts.push(tok.to_owned());
                idx += 1;
            }
        }
    }

    if sql_type_parts.is_empty() {
        return None;
    }

    Some(Column {
        name,
        sql_type: sql_type_parts.join(" "),
        nullable,
        default,
    })
}

fn strip_ident_quotes(raw: &str) -> &str {
    raw.trim_matches('"')
}

/// Project an IR resource into a column-level `ResourceSchema`.
/// Mirrors the type mapping in `migration_ddl::pg_type_for` /
/// `pg_type_for_builtin` so the diff against a freshly emitted
/// migration is identity.
///
/// We capture:
/// - the implicit `id BIGSERIAL PRIMARY KEY` row identity,
/// - each IR `Field` lowered through the same mapping,
/// - the implicit `created_at` / `updated_at` columns when the
///   resource opts in to timestamps (the IR `Resource.timestamps`
///   axis is `Option<bool>`; `None` and `Some(true)` both opt-in
///   — feature-level defaults are resolved upstream),
/// - the `deleted_at TIMESTAMPTZ` column when `soft_delete`.
///
/// We deliberately do NOT capture:
/// - The `org_id BIGINT NOT NULL` tenancy column. That requires
///   `Module + Feature` to resolve tenancy inheritance; A11's
///   first-cut ALTER emitter restricts itself to the resource-
///   intrinsic columns and A12's doctor lint warns when tenancy
///   would have to change.
/// - The `<money>_currency` paired columns (same reason — needs
///   feature context).
/// - FK column type narrowing to `BIGINT` (needs `CrossFeatureIndex`).
/// - Generated-as expressions for `derived from` fields (these
///   never participate in ALTER TABLE; A11 skips them).
pub fn current_schema_from_ir(resource: &Resource) -> ResourceSchema {
    let mut columns = Vec::new();

    columns.push(Column::new("id", "BIGSERIAL", false));

    for field in &resource.fields {
        if field.derived_from.is_some() {
            // Generated columns are never altered by hand; A11
            // emits a `DROP COLUMN` + `ADD GENERATED COLUMN` plan
            // through a different path (out of A10 scope).
            continue;
        }
        columns.push(field_to_column(field));
    }

    // `timestamps` defaults to opted-in unless the resource set
    // `Some(false)`. We don't have the feature here, so mirror the
    // resource-local default; A12 will warn if the feature default
    // disagrees.
    let timestamps_enabled = resource.timestamps.unwrap_or(true);
    if timestamps_enabled {
        columns.push(Column::new("created_at", "TIMESTAMPTZ", false).with_default("NOW()"));
        columns.push(Column::new("updated_at", "TIMESTAMPTZ", false).with_default("NOW()"));
    }

    if resource.soft_delete {
        columns.push(Column::new("deleted_at", "TIMESTAMPTZ", true));
    }

    ResourceSchema { columns }
}

fn field_to_column(field: &Field) -> Column {
    let sql_type = ir_pg_type(&field.type_ref);
    let nullable = !field.required;
    let default = field.default.as_ref().map(render_default);
    Column {
        name: field.name.clone(),
        sql_type,
        nullable,
        default,
    }
}

/// Subset of `migration_ddl::pg_type_for` that operates without a
/// `Module` / `Feature` / `CrossFeatureIndex`. The cell-scope
/// signature `current_schema_from_ir(resource)` is intentionally
/// narrower than the main codegen path; the trade-off is that FK
/// types stay at their nominal mapping (e.g. a `UserDefined`
/// resource ref renders as `TEXT`). A11's caller-side wiring will
/// upgrade this to the cross-feature view if doctor (A12) flags a
/// false-positive type change against an FK column.
fn ir_pg_type(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => ir_pg_type_for_builtin(builtin),
        TypeRef::Many(inner) => format!("{}[]", ir_pg_type(inner)),
        TypeRef::Capability(cap) => ir_pg_type_for_capability(cap),
        TypeRef::UserDefined(_) | TypeRef::EnumRef(_) | TypeRef::Unresolved(_) => "TEXT".to_owned(),
    }
}

fn ir_pg_type_for_builtin(builtin: &BuiltinType) -> String {
    match builtin {
        BuiltinType::Id => "BIGINT".to_owned(),
        BuiltinType::Text => "TEXT".to_owned(),
        BuiltinType::Boolean => "BOOLEAN".to_owned(),
        BuiltinType::Integer => "BIGINT".to_owned(),
        BuiltinType::Decimal => "NUMERIC(20, 6)".to_owned(),
        BuiltinType::Date => "DATE".to_owned(),
        BuiltinType::DateTime => "TIMESTAMPTZ".to_owned(),
        BuiltinType::Json => "JSONB".to_owned(),
        BuiltinType::SemanticEmail
        | BuiltinType::SemanticPhone
        | BuiltinType::SemanticUrl
        | BuiltinType::SemanticUuid
        | BuiltinType::SemanticCurrency => "TEXT".to_owned(),
        BuiltinType::SemanticMoney { .. } => "NUMERIC(20,4)".to_owned(),
        BuiltinType::SemanticGeoPoint => "geography(point, 4326)".to_owned(),
        BuiltinType::SemanticPluginType { carrier, .. } => ir_pg_type_for_builtin(carrier),
        BuiltinType::CapSecret => "TEXT".to_owned(),
        BuiltinType::CapFile => "JSONB".to_owned(),
    }
}

fn ir_pg_type_for_capability(capability: &CapabilityRef) -> String {
    match capability {
        CapabilityRef::Encrypted(_) | CapabilityRef::E2ee(_) => "BYTEA".to_owned(),
        CapabilityRef::File(_) => "JSONB".to_owned(),
        _ => "TEXT".to_owned(),
    }
}

fn render_default(default: &DefaultValue) -> String {
    match default {
        DefaultValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        DefaultValue::Integer(i) => i.to_string(),
        DefaultValue::Boolean(b) => if *b { "true" } else { "false" }.to_owned(),
        DefaultValue::EnumLiteral(lit) => format!("'{}'", lit.variant),
        DefaultValue::Nil => "NULL".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Field, FieldConstraints, Resource, TypeRef};

    fn col(name: &str, sql_type: &str, nullable: bool) -> Column {
        Column::new(name, sql_type, nullable)
    }

    fn empty_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn empty_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: Some(true),
            fields,
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    // ---------- diff() core cases ----------

    #[test]
    fn diff_detects_added_column() {
        let baseline = ResourceSchema::new(vec![col("a", "TEXT", true), col("b", "BIGINT", false)]);
        let current = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("c", "JSONB", true),
        ]);
        let out = diff(&baseline, &current);
        assert_eq!(out.adds, vec![col("c", "JSONB", true)]);
        assert!(out.drops.is_empty());
        assert!(out.type_changes.is_empty());
    }

    #[test]
    fn diff_detects_dropped_column() {
        let baseline = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("c", "JSONB", true),
        ]);
        let current = ResourceSchema::new(vec![col("a", "TEXT", true), col("b", "BIGINT", false)]);
        let out = diff(&baseline, &current);
        assert_eq!(out.drops, vec![col("c", "JSONB", true)]);
        assert!(out.adds.is_empty());
        assert!(out.type_changes.is_empty());
    }

    #[test]
    fn diff_detects_type_change() {
        let baseline = ResourceSchema::new(vec![col("address", "TEXT", true)]);
        let current = ResourceSchema::new(vec![col("address", "JSONB", true)]);
        let out = diff(&baseline, &current);
        assert_eq!(
            out.type_changes,
            vec![TypeChange {
                column: "address".to_owned(),
                old_type: "TEXT".to_owned(),
                new_type: "JSONB".to_owned(),
            }]
        );
        assert!(out.adds.is_empty());
        assert!(out.drops.is_empty());
    }

    #[test]
    fn diff_no_change_is_empty() {
        let baseline = ResourceSchema::new(vec![col("a", "TEXT", false), col("b", "BIGINT", true)]);
        let current = baseline.clone();
        let out = diff(&baseline, &current);
        assert!(out.is_empty(), "expected empty diff, got {out:?}");
    }

    #[test]
    fn diff_combines_add_drop_type_change() {
        let baseline = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("address", "TEXT", true),
        ]);
        let current = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("address", "JSONB", true),
            col("profile_photo", "JSONB", true),
        ]);
        let out = diff(&baseline, &current);
        assert_eq!(out.adds, vec![col("profile_photo", "JSONB", true)]);
        assert_eq!(out.drops, vec![col("b", "BIGINT", false)]);
        assert_eq!(
            out.type_changes,
            vec![TypeChange {
                column: "address".to_owned(),
                old_type: "TEXT".to_owned(),
                new_type: "JSONB".to_owned(),
            }]
        );
    }

    // ---------- parse_baseline_from_str ----------

    #[test]
    fn parse_extracts_columns_from_generated_migration() {
        let sql = r#"
-- Code generated by lazuli; DO NOT EDIT.
-- source: examples/full-capsule
-- resource: host.host

CREATE TABLE IF NOT EXISTS "host" (
    id BIGSERIAL PRIMARY KEY,
    org_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX host_org_id_idx ON "host" (org_id);
"#;
        let schema = parse_baseline_from_str(sql).expect("parse ok");
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "id",
                "org_id",
                "name",
                "address",
                "created_at",
                "updated_at"
            ]
        );

        let name_col = schema
            .columns
            .iter()
            .find(|c| c.name == "name")
            .expect("name column present");
        assert_eq!(name_col.sql_type, "TEXT");
        assert!(!name_col.nullable, "NOT NULL should set nullable=false");
        assert!(name_col.default.is_none());

        let address_col = schema
            .columns
            .iter()
            .find(|c| c.name == "address")
            .expect("address column present");
        assert_eq!(address_col.sql_type, "TEXT");
        assert!(address_col.nullable, "no NOT NULL → nullable=true");

        let created_col = schema
            .columns
            .iter()
            .find(|c| c.name == "created_at")
            .expect("created_at column present");
        assert_eq!(created_col.sql_type, "TIMESTAMPTZ");
        assert_eq!(created_col.default.as_deref(), Some("NOW()"));
    }

    #[test]
    fn parse_skips_table_level_constraints() {
        let sql = r#"
CREATE TABLE IF NOT EXISTS "order" (
    id BIGSERIAL PRIMARY KEY,
    org_id BIGINT NOT NULL,
    code TEXT NOT NULL,
    UNIQUE (org_id, code),
    CHECK (LENGTH(code) > 0)
);
"#;
        let schema = parse_baseline_from_str(sql).expect("parse ok");
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "org_id", "code"]);
    }

    #[test]
    fn parse_strips_trailing_inline_comment_and_comma() {
        let sql = r#"
CREATE TABLE IF NOT EXISTS "secret" (
    id BIGSERIAL PRIMARY KEY,
    token BYTEA NOT NULL, -- lazuli:encrypted @key.tenant algorithm=aes_256_gcm
    note TEXT
);
"#;
        let schema = parse_baseline_from_str(sql).expect("parse ok");
        let token_col = schema
            .columns
            .iter()
            .find(|c| c.name == "token")
            .expect("token column present");
        assert_eq!(token_col.sql_type, "BYTEA");
        assert!(!token_col.nullable);
    }

    #[test]
    fn parse_errors_when_create_table_missing() {
        let sql = "-- nothing here\n";
        let err = parse_baseline_from_str(sql).unwrap_err();
        assert!(
            matches!(err, ParseError::MissingCreateTable),
            "expected MissingCreateTable, got {err:?}"
        );
    }

    #[test]
    fn parse_errors_when_block_not_closed() {
        let sql = "CREATE TABLE foo (\n    id BIGSERIAL PRIMARY KEY\n";
        let err = parse_baseline_from_str(sql).unwrap_err();
        assert!(
            matches!(err, ParseError::UnterminatedCreateTable),
            "expected UnterminatedCreateTable, got {err:?}"
        );
    }

    // ---------- current_schema_from_ir ----------

    #[test]
    fn current_schema_emits_implicit_id_and_timestamps() {
        let resource = empty_resource(
            "Host",
            vec![empty_field(
                "name",
                TypeRef::Builtin(BuiltinType::Text),
                true,
            )],
        );
        let schema = current_schema_from_ir(&resource);
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name", "created_at", "updated_at"]);
    }

    #[test]
    fn current_schema_round_trip_against_parser() {
        // The diff between a baseline parsed from a generated
        // migration AND the IR projection of the same resource
        // shape must be empty. This is the smoke test that says:
        // re-running `lazuli generate` on an unchanged spec emits
        // no ALTER plan.
        let resource = empty_resource(
            "Host",
            vec![
                empty_field("name", TypeRef::Builtin(BuiltinType::Text), true),
                empty_field("address", TypeRef::Builtin(BuiltinType::Text), false),
            ],
        );

        let sql = r#"
CREATE TABLE IF NOT EXISTS "host" (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;
        let baseline = parse_baseline_from_str(sql).expect("parse ok");
        let current = current_schema_from_ir(&resource);

        // The parser captures `id BIGSERIAL` (no PRIMARY KEY in
        // the type string, that's a structural suffix), and the
        // IR projection emits `BIGSERIAL` — same string.
        let out = diff(&baseline, &current);
        assert!(
            out.is_empty(),
            "expected empty round-trip diff, got {out:?}"
        );
    }

    #[test]
    fn current_schema_emits_profile_photo_jsonb_for_jsonb_field() {
        // Concrete hostpoint regression case: a `profile_photo: Json`
        // field MUST project to a `JSONB` column so the A11
        // ALTER emitter doesn't loop adding/dropping the same column.
        let mut resource = empty_resource(
            "Host",
            vec![empty_field(
                "profile_photo",
                TypeRef::Builtin(BuiltinType::Json),
                false,
            )],
        );
        resource.timestamps = Some(false);
        let schema = current_schema_from_ir(&resource);
        let pp = schema
            .columns
            .iter()
            .find(|c| c.name == "profile_photo")
            .expect("profile_photo column present");
        assert_eq!(pp.sql_type, "JSONB");
        assert!(pp.nullable);
    }
}
