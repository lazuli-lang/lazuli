//! Wave B.4 — `query.view` is a typed SQL-backed screen-read primitive.
//!
//! The analyzer lowers `source @file.<name>.sql` into the canonical
//! project-relative file path; doctor owns the filesystem and
//! best-effort unsafe-SQL checks. Two diagnostics:
//!
//! * `QUERY-VIEW-SQL-FILE-001` — the SQL file does not exist on disk.
//! * `QUERY-VIEW-SQL-UNSAFE-001` — best-effort heuristic for
//!   user-influenced SQL text instead of bound parameters.
//!
//! Lifted from the parent `report_storage` god-file in the rails-style
//! split.

use std::fs;
use std::path::{Path, PathBuf};

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(crate) fn query_view_sql_file_diagnostics(
    facts: &[Tier3FeatureFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for feature in facts {
        for query in &feature.queries {
            let lazuli_ir::Query::Sql(query) = query else {
                continue;
            };
            if query.sql_kind != lazuli_ir::SqlQueryKind::View {
                continue;
            }

            let line = feature
                .query_lines
                .get(query.name.as_str())
                .copied()
                .unwrap_or(feature.feature_line);
            let sql_path = resolve_query_view_sql_path(project_root, &query.sql_path);
            if !sql_path.is_file() {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-FILE-001".to_owned(),
                    message: format!(
                        "`query.view {}` references SQL source `{}` but the file does not exist.",
                        query.name, query.sql_path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            }

            let Ok(sql) = fs::read_to_string(&sql_path) else {
                continue;
            };
            if let Some((sql_line, reason)) = query_view_unsafe_sql_line(&sql) {
                diagnostics.push(DoctorDiagnostic {
                    path: sql_path,
                    line: sql_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-UNSAFE-001".to_owned(),
                    message: format!(
                        "`query.view {}` SQL looks like it builds user-influenced text instead of binding parameters: {reason}.",
                        query.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

fn resolve_query_view_sql_path(project_root: &Path, sql_path: &str) -> PathBuf {
    let path = Path::new(sql_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn query_view_unsafe_sql_line(sql: &str) -> Option<(usize, &'static str)> {
    for (idx, line) in sql.lines().enumerate() {
        if line.contains("'%s'") || line.contains("\"%s\"") || line.contains("%s") {
            return Some((idx + 1, "`%s` formatting marker"));
        }
        if plus_near_dollar_placeholder(line) {
            return Some((idx + 1, "`+` near a `$<n>` placeholder"));
        }
    }
    None
}

fn plus_near_dollar_placeholder(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte != b'$'
            || !bytes
                .get(idx + 1)
                .map(|b| b.is_ascii_digit())
                .unwrap_or(false)
        {
            continue;
        }
        let start = idx.saturating_sub(48);
        let end = (idx + 48).min(bytes.len());
        let window = &bytes[start..end];
        if window.contains(&b'+') && window.contains(&b'\'') {
            return true;
        }
    }
    false
}
