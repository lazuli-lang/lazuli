//! Doctor rule `type-duplicate`.
//!
//! Flags user-authored `.ts(x)` files that declare an `interface X {}`
//! or `type X = ...` whose name `X` is already exported by a generated
//! `dist/ts-web/<feature>/<feature>.gen.ts` or
//! `dist/ts-mobile/<feature>/<feature>.gen.ts` module. Duplicating the
//! generated type is drift-prone: the user copy does not update when
//! `lazuli generate ts` regenerates.
//!
//! Detection is intentionally regex-free: generated and user-authored files
//! are scanned line-by-line for leading `interface` / `type` declarations.
//!
//! The line-walker is **import-block aware**: lines that appear inside a
//! multi-line `import { ... } from "..."` block are skipped so that named
//! type imports written as `type Foo` (TS 4.5+ syntax) are not mistaken
//! for local re-declarations. Wave F01 surfaced this false positive on
//! the the canonical pilot canonical pilot: 20 of 31 reported diagnostics were
//! framework noise from multi-line SDK imports of generated types.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One `type-duplicate` finding — a user-authored type that shadows a
/// generated one.
pub struct Finding {
    /// User file containing the duplicating declaration.
    pub user_file: PathBuf,
    /// Type name that collides.
    pub type_name: String,
    /// Generated file that already exports the type.
    pub generated_origin: PathBuf,
    /// Pre-rendered diagnostic message.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "type-duplicate";

    /// Render the diagnostic naming the offending file and the
    /// generated origin to import from instead.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// // let msg = Finding::message("Foo", Path::new("a.ts"), Path::new("b.gen.ts"));
    /// ```
    pub fn message(type_name: &str, user_file: &Path, generated_origin: &Path) -> String {
        format!(
            "`{}` redeclares type `{}` which is already exported from `{}`. \
             Import the generated type instead of duplicating; the user copy \
             will drift on next `lazuli generate ts`.",
            user_file.display(),
            type_name,
            generated_origin.display(),
        )
    }
}

/// Walk `root`, collect generated type names from `dist/ts-web/` and
/// `dist/ts-mobile/`, scan user code for redeclarations, emit Findings.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::folder::type_duplicate::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let generated = collect_generated_types(root);
    if generated.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for user_file in collect_user_ts_files(root) {
        let Ok(source) = fs::read_to_string(&user_file) else {
            continue;
        };

        for type_name in extract_declared_type_names(&source) {
            if type_name.len() < 2 {
                continue;
            }
            let Some(generated_origin) = generated.get(&type_name) else {
                continue;
            };

            findings.push(Finding {
                message: Finding::message(&type_name, &user_file, generated_origin),
                user_file: user_file.clone(),
                type_name,
                generated_origin: generated_origin.clone(),
            });
        }
    }

    findings.sort_by(|a, b| {
        a.user_file
            .cmp(&b.user_file)
            .then_with(|| a.type_name.cmp(&b.type_name))
    });
    findings
}

// --- private helpers ---

fn collect_generated_types(root: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for target in ["ts-mobile", "ts-web"] {
        let base = root.join("dist").join(target);
        let mut files = Vec::new();
        collect_generated_files(&base, &mut files);
        files.sort();

        for path in files {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };

            for name in extract_declared_type_names(&source) {
                if name.len() >= 2 {
                    out.entry(name).or_insert_with(|| path.clone());
                }
            }
        }
    }
    out
}

fn collect_generated_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_generated_files(&path, out);
        } else if is_feature_gen_file(&path) {
            out.push(path);
        }
    }
}

fn is_feature_gen_file(path: &Path) -> bool {
    if matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("routes.gen.ts" | "routes.gen.tsx")
    ) {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(feature_dir) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };

    file_name == format!("{feature_dir}.gen.ts")
}

fn collect_user_ts_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_user_ts_files_under(&root.join("features"), true, &mut out);
    collect_user_ts_files_under(&root.join("app"), false, &mut out);
    out.sort();
    out
}

fn collect_user_ts_files_under(dir: &Path, require_platform_dir: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(entry.file_name().to_str().unwrap_or("")) {
                continue;
            }
            collect_user_ts_files_under(&path, require_platform_dir, out);
        } else if is_ts_source(&path)
            && (!require_platform_dir || has_web_or_mobile_component(&path))
        {
            out.push(path);
        }
    }
}

fn sorted_entries(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

fn is_ts_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts") | Some("tsx")
    )
}

fn has_web_or_mobile_component(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_str().unwrap_or("");
        value == "web" || value == "mobile"
    })
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "dist"
            | "node_modules"
            | "target"
            | ".git"
            | ".lazuli"
            | ".next"
            | ".expo"
            // 2026-05-27 — Playwright/Vitest/Cypress test trees may locally
            // redeclare types for typed mocks etc; skip to avoid false-
            // positive type-duplicate findings.
            | "e2e"
            | "tests"
            | "__tests__"
            | "playwright"
            | "cypress"
    )
}

/// Extract exported type/interface names from a TS source string.
/// Handles `export interface X`, `export type X =`, `interface X`,
/// `type X =`. Stops at the identifier; does not parse the body.
///
/// Import-block aware: lines inside `import { ... } from "..."` blocks
/// (whether single-line or multi-line) are skipped so that the TS 4.5+
/// inline-`type` named-import syntax (`import { type Foo, ... }`) is not
/// misread as a local re-declaration. See Wave F01 on the the canonical pilot.
fn extract_declared_type_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_import_block = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        // Single-line imports start and end on the same line.
        // Multi-line imports span lines from `import {` through `} from "..."`.
        if !in_import_block {
            if is_import_block_open(trimmed) {
                // Skip this line entirely; if the block doesn't close on
                // this line, enter multi-line mode until the close.
                if !is_import_block_close(trimmed) {
                    in_import_block = true;
                }
                continue;
            }
        } else {
            if is_import_block_close(trimmed) {
                in_import_block = false;
            }
            continue;
        }

        let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        for prefix in &["interface ", "type "] {
            if let Some(rest) = after_export.strip_prefix(prefix)
                && let Some(name) = first_identifier(rest)
                && !name.is_empty()
            {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// True if `trimmed` (already left-trimmed) begins a `import { ... }` block.
/// Matches `import {`, `import type {`, with optional whitespace between
/// the keyword and the brace. Bare `import "..."` (side-effect imports)
/// and `import Default from "..."` (default-only, no brace) do not need
/// type-line filtering and return false.
fn is_import_block_open(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("import") else {
        return false;
    };
    // Require word boundary after `import` to avoid matching e.g. `importance`.
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if !first.is_whitespace() {
        return false;
    }
    // Drop optional `type ` modifier: `import type { ... }`.
    let rest = rest.trim_start();
    let after_optional_type = match rest.strip_prefix("type") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    };
    after_optional_type.starts_with('{')
}

/// True if `trimmed` contains the closing `} from "..."` (or single-quoted)
/// of an `import { ... } from "..."` statement. Tolerates trailing
/// whitespace/semicolons; only checks that the close-brace and `from`
/// clause coexist on the line.
fn is_import_block_close(trimmed: &str) -> bool {
    let Some(brace_idx) = trimmed.find('}') else {
        return false;
    };
    let after = &trimmed[brace_idx + 1..];
    let after = after.trim_start();
    // Allow `} from "X"` or just `}` followed eventually by `from "X"`.
    // Also accept lone `}` on its own line where `from` is on the next
    // line — but in real TS that is rare; we conservatively also treat
    // a lone trailing `}` as closing the block to avoid getting stuck.
    if after.is_empty() || after.starts_with(';') || after.starts_with(',') {
        return true;
    }
    let after = after.strip_prefix("from").unwrap_or(after);
    let after = after.trim_start();
    after.starts_with('"') || after.starts_with('\'')
}

/// Walk leading characters of `s` while they match `[A-Za-z_][A-Za-z0-9_]*`.
fn first_identifier(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = 1;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() || b == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    Some(&s[..end])
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_import_blocks;
#[cfg(test)]
mod tests_wave_h;
