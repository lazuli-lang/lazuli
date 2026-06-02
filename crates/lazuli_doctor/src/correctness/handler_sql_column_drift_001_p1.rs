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
                    // Framework-synthesized columns (`id` BIGSERIAL identity,
                    // `created_at` / `updated_at` TIMESTAMPTZ DEFAULT NOW(),
                    // soft-delete sentinels) are auto-supplied by Postgres on
                    // INSERT — a serial/identity PK value is generated by the
                    // sequence, and the timestamps carry a DB-side `DEFAULT
                    // NOW()` (see `lazuli_codegen_go::emitter::schema_diff::ir`
                    // — every name in `synthesized_columns` is emitted with a
                    // default or as BIGSERIAL). Omitting them from a handler
                    // INSERT is correct, not drift. We derive the omittable set
                    // from the IR single-source-of-truth
                    // (`lazuli_ir::synthesized_columns::is_synthesized_column`)
                    // rather than hardcoding `"id"`, so the doctor and the
                    // schema-diff emitter can never disagree on the synthesized
                    // names. Author-declared NOT NULL fields stay required.
                    .filter(|c| !lazuli_ir::synthesized_columns::is_synthesized_column(c))
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
        if let Some(line) = lines.get(*idx)
            && line.to_ascii_lowercase().contains(&needle) {
                return true;
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
