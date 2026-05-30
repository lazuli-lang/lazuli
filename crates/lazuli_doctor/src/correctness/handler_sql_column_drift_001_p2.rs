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
