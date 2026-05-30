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
    if let Some(rest) = s.strip_prefix('"')
        && let Some(end) = rest.find('"') {
            return rest[..end].to_owned();
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
    include!("migration_alter_missing_001_tests.rs");
}
