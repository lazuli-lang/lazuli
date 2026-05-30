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
fn column_diff(ir: &BTreeSet<String>, migration: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
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
