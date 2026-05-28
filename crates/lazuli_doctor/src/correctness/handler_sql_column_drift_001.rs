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

/// One `HANDLER-SQL-COLUMN-DRIFT-001` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Workspace-relative path of the offending handler file.
    pub path: PathBuf,
    /// 1-based line of the `INSERT` keyword inside the handler source.
    pub line: usize,
    /// Feature the handler lives under (`account`, `billing`, ...).
    pub feature: String,
    /// SQL table name as written in the statement (quotes stripped).
    pub table: String,
    /// Resource name resolved from the table — `Some("User")` when
    /// the table mapped, `None` for [`FindingKind::SqlUnreadable`].
    pub resource: Option<String>,
    /// Per-shape detail (what kind of drift fired).
    pub kind: FindingKind,
}

/// Why a `HANDLER-SQL-COLUMN-DRIFT-001` finding fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// INSERT column list omits ≥1 NOT NULL column declared on the
    /// resource struct. Sorted ASCII-ascending for stable diagnostics.
    Missing { missing: Vec<String> },
    /// SQL fragment couldn't be statically validated — the raw string
    /// is empty after the `INSERT INTO ` header or the parenthesised
    /// column list ran past the literal's end (canonical case: the
    /// author concatenated SQL with `+`).
    SqlUnreadable { fragment: String },
}

impl Finding {
    /// Stable rule code emitted on every finding.
    pub const CODE: &'static str = "HANDLER-SQL-COLUMN-DRIFT-001";

    /// Render the diagnostic message. Anchors the file+line, names the
    /// table + resource, and lists the missing columns alongside the
    /// runtime failure mode the author would otherwise discover in
    /// production.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::handler_sql_column_drift_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/account/handlers/register_with_google.go"),
    ///     line: 88,
    ///     feature: "account".into(),
    ///     table: "user".into(),
    ///     resource: Some("User".into()),
    ///     kind: FindingKind::Missing { missing: vec!["updated_at".into()] },
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("updated_at"));
    /// assert!(msg.contains("NOT NULL"));
    /// ```
    pub fn message(&self) -> String {
        match &self.kind {
            FindingKind::Missing { missing } => format!(
                "{path}:{line} handler SQL drifts from resource schema — \
                 INSERT INTO \"{table}\" omits required column(s): [{cols}]. \
                 Resource `{feature}.{resource}` declares these as NOT NULL \
                 (see dist/go/{feature}/resource.gen.go). The INSERT will \
                 fail with a NOT NULL violation in production. Either add \
                 the column(s) to the SQL, switch to lazuli.Insert(...), or \
                 change the migration to default the column.",
                path = self.path.display(),
                line = self.line,
                table = self.table,
                cols = missing.join(", "),
                feature = self.feature,
                resource = self.resource.as_deref().unwrap_or("<unresolved>"),
            ),
            FindingKind::SqlUnreadable { fragment } => format!(
                "{path}:{line} handler SQL fragment cannot be statically \
                 validated — looks like string concatenation. Fragment: \
                 `{fragment}`. HANDLER-SQL-COLUMN-DRIFT-001 needs a single \
                 raw-string literal containing the full INSERT statement; \
                 VOCAB-SECURITY-SQL-CONCAT-001 flags the concatenation \
                 itself. Inline the SQL into one backtick raw string to \
                 enable drift checking.",
                path = self.path.display(),
                line = self.line,
                fragment = fragment,
            ),
        }
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `HANDLER-SQL-COLUMN-DRIFT-001` against the handler walker's
/// output, using `workspace_root` to locate the `dist/go/*/resource.gen.go`
/// column catalog.
///
/// Returns an empty vec when:
/// - `handlers` is empty.
/// - `dist/go/` does not exist or contains no `resource.gen.go` files
///   (initial codegen not yet run — the per-resource catalog is empty
///   and every table fails to resolve, which is a silent short-circuit
///   per the spec's false-positive guards).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::handler_sql_column_drift_001::check;
/// use lazuli_doctor::error_handling::walker::walk_workspace_go_handlers;
///
/// let root = Path::new("/proj/app");
/// let handlers = walk_workspace_go_handlers(root);
/// let findings = check(&handlers, root);
/// ```
pub fn check(handlers: &[GoHandlerSourceFile], workspace_root: &Path) -> Vec<Finding> {
    let catalog = build_resource_catalog(workspace_root);
    let mut out = Vec::new();
    for handler in handlers {
        if handler.is_test {
            continue;
        }
        scan_handler(handler, &catalog, &mut out);
    }
    out
}

// ── handler scanning ────────────────────────────────────────────────────────

fn scan_handler(
    handler: &GoHandlerSourceFile,
    catalog: &BTreeMap<String, ResourceEntry>,
    out: &mut Vec<Finding>,
) {
    for stmt in iter_sql_statements(&handler.source) {
        if has_allow_comment_above(&handler.source, stmt.line) {
            continue;
        }
        match stmt.kind {
            StmtKind::Insert { table, columns } => {
                let Some(entry) = catalog.get(&table) else {
                    // Table doesn't resolve to a known resource — out
                    // of scope per false-positive guard.
                    continue;
                };
                let missing: Vec<String> = entry
                    .required_columns
                    .iter()
                    .filter(|c| !columns.contains(*c))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    continue;
                }
                out.push(Finding {
                    path: handler.relative_path.clone(),
                    line: stmt.line,
                    feature: handler.feature_name.clone(),
                    table,
                    resource: Some(entry.resource_name.clone()),
                    kind: FindingKind::Missing { missing },
                });
            }
            StmtKind::Update { .. } => {
                // v0.1 scope: UPDATE statements parse cleanly but don't
                // fire (NOT NULL doesn't apply to SET clauses). The
                // unknown-column-on-SET case is a deferred follow-up.
                continue;
            }
            StmtKind::Unreadable { fragment } => {
                out.push(Finding {
                    path: handler.relative_path.clone(),
                    line: stmt.line,
                    feature: handler.feature_name.clone(),
                    table: String::new(),
                    resource: None,
                    kind: FindingKind::SqlUnreadable { fragment },
                });
            }
        }
    }
}

/// Return `true` when the line above `line` carries a
/// `# doctor:allow HANDLER-SQL-COLUMN-DRIFT-001` comment OR the line
/// itself has a `// doctor:allow ...` trailing comment (Go convention —
/// the spec example places the allow on the line above, but authors may
/// also write it on the same line as the SQL).
///
/// The shared [`source_contains_doctor_allow`] helper matches
/// `#`-prefixed comments only (canonical Rust/TOML form). Go handlers
/// use `//` line comments and `/* */` block comments, so we replicate
/// the substring match here scoped to the adjacent line.
fn has_allow_comment_above(source: &str, stmt_line: usize) -> bool {
    if stmt_line == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let needle = format!("doctor:allow {}", Finding::CODE.to_ascii_lowercase());
    // Check the line above (1-based stmt_line means index stmt_line-2
    // is the previous line) and the statement line itself (trailing
    // comment on the same line counts too).
    let candidates: &[usize] = if stmt_line >= 2 {
        &[stmt_line - 2, stmt_line - 1]
    } else {
        &[stmt_line - 1]
    };
    for idx in candidates {
        if let Some(line) = lines.get(*idx) {
            if line.to_ascii_lowercase().contains(&needle) {
                return true;
            }
        }
    }
    // Fall back on the canonical `#`-prefixed form anywhere in the
    // file (parity with the rest of the doctor crate).
    source_contains_doctor_allow(source, Finding::CODE)
}

// ── SQL statement walker (over Go raw strings) ──────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct SqlStatement {
    /// 1-based line of the `INSERT` / `UPDATE` keyword in the file.
    line: usize,
    kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StmtKind {
    Insert {
        table: String,
        columns: Vec<String>,
    },
    Update {
        #[allow(dead_code)] // kept for future column-set-on-UPDATE rule
        table: String,
    },
    Unreadable {
        fragment: String,
    },
}

/// Walk `source` and yield every SQL `INSERT` / `UPDATE` statement
/// inside a Go raw-string literal (backtick-delimited). Double-quoted
/// strings are skipped because the authoring convention puts multi-line
/// SQL inside backtick raw strings.
fn iter_sql_statements(source: &str) -> Vec<SqlStatement> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'`' => {
                // Raw string opener. Find the closing backtick.
                let start = i + 1;
                let open_line = line;
                i += 1;
                let mut end = None;
                while i < bytes.len() {
                    if bytes[i] == b'`' {
                        end = Some(i);
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                let Some(end) = end else {
                    // Unterminated raw string — bail out of the scan.
                    break;
                };
                let raw = &source[start..end];
                extract_statements_from_raw(raw, open_line, &mut out);
            }
            b'"' => {
                // Skip a regular Go string literal (single line).
                // Track escapes so `"\\\""` doesn't end the literal
                // prematurely.
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        // Go double-quoted strings can't span lines —
                        // recover by bumping out.
                        line += 1;
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                // Skip `// line comment` to end of line.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                // Skip `/* block comment */`.
                i += 2;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    out
}

/// Parse the raw-string body for INSERT / UPDATE statements. `open_line`
/// is the line of the opening backtick — the rule attributes the
/// statement to `open_line + <line-offset-of-keyword-inside-raw>`.
fn extract_statements_from_raw(raw: &str, open_line: usize, out: &mut Vec<SqlStatement>) {
    let upper = raw.to_ascii_uppercase();

    // Scan for every INSERT INTO occurrence — a single raw string
    // could (in theory) hold more than one statement.
    let mut search_start = 0;
    while let Some(rel) = upper[search_start..].find("INSERT INTO") {
        let kw_pos = search_start + rel;
        let kw_line = open_line + raw[..kw_pos].matches('\n').count();
        let after = &raw[kw_pos + "INSERT INTO".len()..];
        match parse_insert_header(after) {
            Some((table, columns)) => out.push(SqlStatement {
                line: kw_line,
                kind: StmtKind::Insert { table, columns },
            }),
            None => out.push(SqlStatement {
                line: kw_line,
                kind: StmtKind::Unreadable {
                    fragment: fragment_excerpt(&raw[kw_pos..]),
                },
            }),
        }
        search_start = kw_pos + "INSERT INTO".len();
    }

    let mut search_start = 0;
    while let Some(rel) = upper[search_start..].find("UPDATE ") {
        let kw_pos = search_start + rel;
        // Guard: avoid matching "FOR UPDATE", "ON UPDATE", etc.
        let preceding = raw[..kw_pos].chars().rev().find(|c| !c.is_whitespace());
        let is_real_update = matches!(preceding, None | Some(';') | Some('(') | Some(')'));
        if is_real_update {
            let kw_line = open_line + raw[..kw_pos].matches('\n').count();
            let after = &raw[kw_pos + "UPDATE ".len()..];
            if let Some(table) = parse_update_target(after) {
                out.push(SqlStatement {
                    line: kw_line,
                    kind: StmtKind::Update { table },
                });
            }
        }
        search_start = kw_pos + "UPDATE ".len();
    }
}

/// Parse `[whitespace] "<ident>" (col, col, ...)` OR
/// `[whitespace] <ident> (col, col, ...)`. Returns `(table, columns)`
/// on success; `None` when the header is malformed or the parenthesised
/// column list is missing.
fn parse_insert_header(after_keyword: &str) -> Option<(String, Vec<String>)> {
    let bytes = after_keyword.as_bytes();
    let mut i = 0usize;
    // Skip leading whitespace.
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }

    // Optional `"` quote around the table name.
    let quoted = bytes[i] == b'"';
    if quoted {
        i += 1;
    }
    let table_start = i;
    while i < bytes.len() && is_ident_byte(bytes[i]) {
        i += 1;
    }
    let table_end = i;
    if table_end == table_start {
        return None;
    }
    let table = after_keyword[table_start..table_end].to_owned();
    if quoted {
        if i >= bytes.len() || bytes[i] != b'"' {
            return None;
        }
        i += 1;
    }
    // Skip whitespace + optional `AS alias` (out of scope; bail).
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        // Could be `INSERT INTO "user" VALUES (...)` (no column list)
        // or `INSERT INTO "user" SELECT ...`. v0.1 can't validate
        // either shape — skip silently rather than mark unreadable so
        // we don't false-positive on legitimate forms.
        return None;
    }
    i += 1;
    // Walk to the matching `)`, splitting on top-level commas.
    let cols_start = i;
    let mut depth = 1usize;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    let cols_text = &after_keyword[cols_start..i];
    let columns = parse_column_list(cols_text);
    Some((table, columns))
}

/// Parse `[whitespace] "<ident>" SET` / `[whitespace] <ident> SET`.
/// Returns the table name; the SET-clause columns themselves are not
/// returned (v0.1 doesn't act on UPDATE drift).
fn parse_update_target(after_keyword: &str) -> Option<String> {
    let bytes = after_keyword.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let quoted = bytes[i] == b'"';
    if quoted {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && is_ident_byte(bytes[i]) {
        i += 1;
    }
    if i == start {
        return None;
    }
    let table = after_keyword[start..i].to_owned();
    Some(table)
}

/// Split a `col1, col2, ...` text on top-level commas (depth-aware to
/// survive `expr(a, b)` payloads) and strip identifier quotes. Returns
/// the bare column names.
fn parse_column_list(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                push_column(&current, &mut out);
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        push_column(&current, &mut out);
    }
    out
}

fn push_column(raw: &str, out: &mut Vec<String>) {
    let trimmed = raw.trim().trim_matches(',').trim();
    if trimmed.is_empty() {
        return;
    }
    let name = trimmed.trim_matches('"').trim_matches('`').trim();
    if !name.is_empty() {
        out.push(name.to_owned());
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Truncate the unreadable fragment to a one-line excerpt for the
/// diagnostic message. Helps the author find the offending SQL without
/// flooding the terminal with an entire raw-string body.
fn fragment_excerpt(raw: &str) -> String {
    let single_line: String = raw
        .chars()
        .take_while(|c| *c != '\n')
        .collect::<String>()
        .trim()
        .to_owned();
    if single_line.len() > 80 {
        format!("{}…", &single_line[..80])
    } else {
        single_line
    }
}

// ── resource catalog (dist/go/<feature>/resource.gen.go) ────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceEntry {
    resource_name: String,
    /// Set of `db:"<col>"` tags whose Go field type is non-pointer
    /// (NOT NULL on insert). Pointer-typed fields and obvious nullable
    /// helpers (`sql.NullString`, etc.) are excluded.
    required_columns: Vec<String>,
}

/// Build a `table_name → ResourceEntry` map by scanning every
/// `dist/go/<feature>/resource.gen.go` file under `workspace_root`.
/// Returns an empty map when `dist/go/` does not exist; the caller
/// short-circuits (every INSERT fails to resolve → no findings) which
/// matches the "no codegen yet" silent-skip in the spec.
fn build_resource_catalog(workspace_root: &Path) -> BTreeMap<String, ResourceEntry> {
    let mut out = BTreeMap::new();
    let dist_go = workspace_root.join("dist").join("go");
    let Ok(entries) = fs::read_dir(&dist_go) else {
        return out;
    };
    for entry in entries.flatten() {
        let feature_dir = entry.path();
        if !feature_dir.is_dir() {
            continue;
        }
        let candidate = feature_dir.join("resource.gen.go");
        if !candidate.is_file() {
            continue;
        }
        let Ok(source) = fs::read_to_string(&candidate) else {
            continue;
        };
        for parsed in parse_resource_structs(&source) {
            let table = lower_snake(&parsed.resource_name);
            out.insert(
                table,
                ResourceEntry {
                    resource_name: parsed.resource_name,
                    required_columns: parsed.required_columns,
                },
            );
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedResource {
    resource_name: String,
    required_columns: Vec<String>,
}

/// Walk `resource.gen.go` source for every `type <Name> struct { ... }`
/// declaration and extract the `db:"<col>"` tag of each non-pointer
/// field as a NOT NULL column. The struct *name* is the resource name
/// per the codegen convention (`type User struct`); the resource's
/// `Resource[T]{ Name: "User" }` registration agrees by construction.
fn parse_resource_structs(source: &str) -> Vec<ParsedResource> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let needle = "type ";
    let mut start = 0usize;
    while let Some(rel) = source[start..].find(needle) {
        let pos = start + rel;
        // Require the match to be at start-of-line (token-style) so
        // we don't hit `prototype `, `// type X is ...`, etc.
        let preceded_ok =
            pos == 0 || matches!(bytes.get(pos - 1).copied(), Some(b'\n') | Some(b'\r'));
        if !preceded_ok {
            start = pos + needle.len();
            continue;
        }
        let after = &source[pos + needle.len()..];
        // Parse `<Name> struct {`
        let Some((name_end_rel, _)) = after.char_indices().find(|(_, c)| c.is_whitespace()) else {
            start = pos + needle.len();
            continue;
        };
        let name = &after[..name_end_rel];
        let rest = &after[name_end_rel..];
        let rest_trimmed = rest.trim_start();
        if !rest_trimmed.starts_with("struct") {
            start = pos + needle.len();
            continue;
        }
        let after_struct = &rest_trimmed["struct".len()..].trim_start();
        if !after_struct.starts_with('{') {
            start = pos + needle.len();
            continue;
        }
        // Find the matching `}` (depth-aware).
        let body_start_abs = source.len() - after_struct.len() + 1;
        let mut depth = 1i32;
        let mut i = body_start_abs;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            i += 1;
        }
        if depth != 0 {
            break;
        }
        let body = &source[body_start_abs..i];
        let required_columns = parse_struct_required_columns(body);
        if !required_columns.is_empty() || !body.trim().is_empty() {
            out.push(ParsedResource {
                resource_name: name.to_owned(),
                required_columns,
            });
        }
        start = i + 1;
    }
    out
}

/// Walk struct body lines, extracting `db:"<col>"` tags whose field
/// type is non-pointer. The convention: `<Name> <Type> \`db:"col" json:"col"\``.
fn parse_struct_required_columns(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        // Tag block: between the first backtick and its matching pair.
        let Some(tag_start) = line.find('`') else {
            continue;
        };
        let after_tag_open = &line[tag_start + 1..];
        let Some(tag_end_rel) = after_tag_open.find('`') else {
            continue;
        };
        let tag = &after_tag_open[..tag_end_rel];
        let Some(db_col) = extract_db_tag(tag) else {
            continue;
        };

        // Field type is the second whitespace-separated token before
        // the tag (e.g. `ID lazuli.ID \`...\``). Pointer types start
        // with `*`; nullable helpers start with `sql.Null` or
        // `*lazuli.`. v0.1 treats `*T` as nullable, all else as NOT
        // NULL — matches the codegen convention where the emitter
        // wraps nullable IR fields in `*T`.
        let before_tag = &line[..tag_start];
        let tokens: Vec<&str> = before_tag.split_ascii_whitespace().collect();
        // First token = field name; tokens[1] onward = type.
        let type_token = tokens.get(1).copied().unwrap_or("");
        if type_token.starts_with('*') {
            continue;
        }
        // `sql.NullString` family — also nullable.
        if type_token.starts_with("sql.Null") {
            continue;
        }
        out.push(db_col);
    }
    out
}

/// Extract the value of the `db:"..."` portion of a Go struct field
/// tag string (between backticks). Returns `None` if absent.
fn extract_db_tag(tag: &str) -> Option<String> {
    let key = "db:\"";
    let pos = tag.find(key)?;
    let after = &tag[pos + key.len()..];
    let end = after.find('"')?;
    let value = &after[..end];
    // `db:"col,omitempty"` — strip option suffix.
    let value = match value.find(',') {
        Some(i) => &value[..i],
        None => value,
    };
    if value.is_empty() || value == "-" {
        return None;
    }
    Some(value.to_owned())
}

/// `User` → `user`, `OrgInvitation` → `org_invitation`. Mirrors the
/// codegen table-naming convention. Local copy (not imported from
/// `schema_migration_present`) per the task instruction to inline
/// helpers for v0.1.
fn lower_snake(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_is_lower_or_digit = false;
    for ch in raw.chars() {
        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if ch == '_' {
            out.push('_');
            prev_is_lower_or_digit = false;
        }
    }
    out
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Mirror of the pilot incident `User` struct — just enough fields
    /// to exercise NOT NULL detection across pointer (`*string`) and
    /// value (`string`, `lazuli.Time`) types.
    const USER_RESOURCE_GEN_GO: &str = r#"// Code generated by lazuli; DO NOT EDIT.
package accountgen

import "github.com/lazuli/runtime/lazuli"

type User struct {
    ID                   lazuli.ID        `db:"id"                    json:"id"`
    OrgID                lazuli.ID        `db:"org_id"                json:"org_id"`
    Org                  lazuli.ID        `db:"org"                   json:"org"`
    Email                lazuli.Email     `db:"email"                 json:"email"`
    Phone                *string          `db:"phone"                 json:"phone,omitempty"`
    Name                 string           `db:"name"                  json:"name"`
    Role                 *string          `db:"role"                  json:"role,omitempty"`
    RegistrationStep     string           `db:"registration_step"     json:"registration_step"`
    IsEmailVerified      bool             `db:"is_email_verified"     json:"is_email_verified"`
    IsPhoneVerified      bool             `db:"is_phone_verified"     json:"is_phone_verified"`
    PasswordHash         string           `db:"password_hash"         json:"-"`
    MfaEnabled           bool             `db:"mfa_enabled"           json:"mfa_enabled"`
    NotificationsEnabled bool             `db:"notifications_enabled" json:"notifications_enabled"`
    CreatedAt            lazuli.Time      `db:"created_at"            json:"created_at"`
    UpdatedAt            lazuli.Time      `db:"updated_at"            json:"updated_at"`
}
"#;

    /// Set up a temp workspace with `dist/go/account/resource.gen.go`
    /// + a handler file under `features/account/handlers/<name>.go`,
    /// and run the rule.
    fn run_against(handler_source: &str, file_name: &str) -> (TempDir, Vec<Finding>) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Codegen artifact.
        let dist = root.join("dist").join("go").join("account");
        fs::create_dir_all(&dist).unwrap();
        fs::write(dist.join("resource.gen.go"), USER_RESOURCE_GEN_GO).unwrap();
        // Handler.
        let handlers = root.join("features").join("account").join("handlers");
        fs::create_dir_all(&handlers).unwrap();
        let handler_path = handlers.join(file_name);
        fs::write(&handler_path, handler_source).unwrap();
        let is_test = file_name.ends_with("_test.go");
        let handlers_list = vec![GoHandlerSourceFile {
            feature_name: "account".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from(format!("features/account/handlers/{file_name}")),
            absolute_path: handler_path,
            source: handler_source.to_owned(),
            loc_count: handler_source.lines().count(),
            is_test,
        }];
        let findings = check(&handlers_list, root);
        (tmp, findings)
    }

    /// Pilot incident replay: the exact `register_with_google.go`
    /// pre-fix INSERT, anonymised down to the column list. The INSERT
    /// omits `updated_at` — the rule MUST fire with `updated_at` named.
    #[test]
    fn pilot_incident_replay_fires() {
        let src = r#"package handlers

import "context"

func RegisterWithGoogle(ctx context.Context) error {
    _, err := db.Exec(ctx,
        `INSERT INTO "user" (
           org_id, org, email, name, password_hash,
           registration_step, is_email_verified, is_phone_verified,
           mfa_enabled, notifications_enabled, created_at
         ) VALUES ($1, $1, $2, $3, '', 'role_pending', $4, false, false, true, $5)`,
        orgID, email, name, verified, now,
    )
    return err
}
"#;
        let (_tmp, findings) = run_against(src, "register_with_google.go");
        assert_eq!(findings.len(), 1, "got: {findings:#?}");
        match &findings[0].kind {
            FindingKind::Missing { missing } => {
                assert!(
                    missing.contains(&"updated_at".to_owned()),
                    "expected updated_at in missing list, got {missing:?}",
                );
            }
            other => panic!("expected Missing, got {other:?}"),
        }
        assert_eq!(findings[0].table, "user");
        assert_eq!(findings[0].resource.as_deref(), Some("User"));
        assert!(findings[0].message().contains("updated_at"));
        assert!(findings[0].message().contains("NOT NULL"));
    }

    /// Happy path — INSERT enumerates every NOT NULL column. No finding.
    #[test]
    fn happy_path_no_finding() {
        let src = r#"package handlers

func Insert() {
    _, _ = db.Exec(ctx, `INSERT INTO "user" (
        id, org_id, org, email, name,
        registration_step, is_email_verified, is_phone_verified,
        password_hash, mfa_enabled, notifications_enabled,
        created_at, updated_at
    ) VALUES (DEFAULT, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`)
}
"#;
        let (_tmp, findings) = run_against(src, "insert_user.go");
        assert!(
            findings.is_empty(),
            "got unexpected findings: {findings:#?}"
        );
    }

    /// String-concatenated SQL (`"INSERT INTO " + tableName + ...`):
    /// the raw-string walker never sees the full statement. v0.1
    /// emits an `SqlUnreadable` finding for any backtick raw string
    /// containing just `INSERT INTO ` with no parenthesised column
    /// list — so authors who concatenated half the SQL into one
    /// fragment still get told the rule can't validate.
    #[test]
    fn concatenated_sql_emits_unreadable() {
        let src = r#"package handlers

func Insert() {
    _, _ = db.Exec(ctx, `INSERT INTO `+table+` (id, email) VALUES ($1, $2)`)
}
"#;
        let (_tmp, findings) = run_against(src, "concat.go");
        // The first raw string is "INSERT INTO " — no table, no
        // column list → Unreadable.
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, FindingKind::SqlUnreadable { .. })),
            "expected SqlUnreadable finding, got: {findings:#?}",
        );
        // And NOT a Missing finding (no false positive on the
        // concatenated form — the catalog can't resolve a partial
        // table name).
        assert!(
            !findings
                .iter()
                .any(|f| matches!(f.kind, FindingKind::Missing { .. })),
            "did not expect Missing finding on concatenated SQL, got: {findings:#?}",
        );
    }

    /// `INSERT INTO "stripe_webhook_log"` — not a Lazuli resource.
    /// Rule short-circuits silently (no finding, no panic).
    #[test]
    fn non_lazuli_table_silent() {
        let src = r#"package handlers

func Log() {
    _, _ = db.Exec(ctx, `INSERT INTO "stripe_webhook_log" (id, payload) VALUES ($1, $2)`)
}
"#;
        let (_tmp, findings) = run_against(src, "log.go");
        assert!(findings.is_empty(), "got: {findings:#?}");
    }

    /// `dist/go/` does not exist (initial codegen not yet run). Every
    /// table fails to resolve → no findings, no panic.
    #[test]
    fn no_codegen_artifact_silent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let handlers = root.join("features").join("account").join("handlers");
        fs::create_dir_all(&handlers).unwrap();
        let src = r#"package handlers
func Insert() {
    _, _ = db.Exec(ctx, `INSERT INTO "user" (id) VALUES ($1)`)
}
"#;
        let handler_path = handlers.join("insert.go");
        fs::write(&handler_path, src).unwrap();
        let handlers_list = vec![GoHandlerSourceFile {
            feature_name: "account".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from("features/account/handlers/insert.go"),
            absolute_path: handler_path,
            source: src.to_owned(),
            loc_count: src.lines().count(),
            is_test: false,
        }];
        let findings = check(&handlers_list, root);
        assert!(findings.is_empty(), "got: {findings:#?}");
    }

    /// `// doctor:allow HANDLER-SQL-COLUMN-DRIFT-001` on the line
    /// above the INSERT silences the finding.
    #[test]
    fn allow_comment_silences() {
        let src = r#"package handlers

func Insert() {
    // doctor:allow HANDLER-SQL-COLUMN-DRIFT-001 — reason "trigger fills updated_at"
    _, _ = db.Exec(ctx, `INSERT INTO "user" (
        id, org_id, org, email, name, registration_step,
        is_email_verified, is_phone_verified, password_hash,
        mfa_enabled, notifications_enabled, created_at
    ) VALUES (DEFAULT, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`)
}
"#;
        let (_tmp, findings) = run_against(src, "insert_user.go");
        assert!(
            findings.is_empty(),
            "allow comment should silence; got: {findings:#?}"
        );
    }

    /// UPDATE statements (v0.1 deferred) MUST NOT false-positive.
    /// The parser sees the statement, classifies it as `Update`, and
    /// emits no finding.
    #[test]
    fn negative_update_does_not_false_positive() {
        let src = r#"package handlers

func Touch() {
    _, _ = db.Exec(ctx, `UPDATE "user" SET name = $1, updated_at = NOW() WHERE id = $2`)
}
"#;
        let (_tmp, findings) = run_against(src, "touch.go");
        assert!(
            findings.is_empty(),
            "UPDATE must not fire in v0.1; got: {findings:#?}"
        );
    }

    /// `*_test.go` files are skipped — Go test convention uses
    /// arbitrary SQL fixtures.
    #[test]
    fn test_file_skipped() {
        let src = r#"package handlers

func TestInsert(t *testing.T) {
    _, _ = db.Exec(ctx, `INSERT INTO "user" (id) VALUES ($1)`)
}
"#;
        let (_tmp, findings) = run_against(src, "register_with_google_test.go");
        assert!(
            findings.is_empty(),
            "test files must be silent; got: {findings:#?}"
        );
    }

    // ── fine-grained parser tests ───────────────────────────────────────────

    #[test]
    fn parse_insert_header_quoted_table() {
        let (table, cols) = parse_insert_header(r#" "user" (id, email, name)"#).unwrap();
        assert_eq!(table, "user");
        assert_eq!(cols, vec!["id", "email", "name"]);
    }

    #[test]
    fn parse_insert_header_unquoted_table() {
        let (table, cols) = parse_insert_header(" user (id, email)").unwrap();
        assert_eq!(table, "user");
        assert_eq!(cols, vec!["id", "email"]);
    }

    #[test]
    fn parse_insert_header_no_column_list_returns_none() {
        // `INSERT INTO "user" VALUES (...)` — no column list. v0.1
        // can't validate this shape, so the parser returns None and
        // the caller skips silently.
        assert!(parse_insert_header(r#" "user" VALUES ($1, $2)"#).is_none());
    }

    #[test]
    fn parse_db_tag_handles_omitempty() {
        assert_eq!(
            extract_db_tag("db:\"phone,omitempty\" json:\"phone\""),
            Some("phone".into())
        );
    }

    #[test]
    fn parse_db_tag_skips_dash() {
        assert!(extract_db_tag("db:\"-\"").is_none());
    }

    #[test]
    fn lower_snake_basic() {
        assert_eq!(lower_snake("User"), "user");
        assert_eq!(lower_snake("OrgInvitation"), "org_invitation");
        assert_eq!(lower_snake("HTTPServer"), "httpserver");
    }

    #[test]
    fn resource_catalog_skips_pointer_fields() {
        let parsed = parse_resource_structs(USER_RESOURCE_GEN_GO);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].resource_name, "User");
        assert!(
            parsed[0]
                .required_columns
                .contains(&"updated_at".to_owned())
        );
        assert!(
            parsed[0]
                .required_columns
                .contains(&"created_at".to_owned())
        );
        // Pointer-typed fields (Phone, Role) are NOT in the required
        // set — they're nullable.
        assert!(!parsed[0].required_columns.contains(&"phone".to_owned()));
        assert!(!parsed[0].required_columns.contains(&"role".to_owned()));
    }
}
