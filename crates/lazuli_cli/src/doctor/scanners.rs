//! Filesystem walkers + text-level scanning leaves used by the
//! doctor pipeline.
//!
//! Three clusters, all `pub(super)`:
//!
//! 1. **Generated-output walkers** — `walk_gen_ts_files` and
//!    `walk_frontend_ts_files` (+ its recursive helper) traverse
//!    `dist/ts-{web,mobile}/**/*.gen.{ts,tsx}` and
//!    `app/clients/<frontend>/src/**/*.{ts,tsx}` respectively. They
//!    skip `node_modules`, `dist`, `build`, generated artifacts,
//!    and test files. Used by `import_deprecated_alias_diagnostics`
//!    and the deprecated-export scanner.
//! 2. **Lazuli-source walkers** — `collect_lazuli_paths_recursive`
//!    enumerates every `.lzi` / `.lzx` under a project root;
//!    `collect_recipe_dirs` finds `recipe.toml`-bearing directories
//!    for the release-migration diagnostics; `package_stem` strips
//!    the canonical "feature stem" from a `.lzi` / `.lzx` filename;
//!    `derive_feature_name` reads the first `feature <name>` header
//!    inside a `.lzi` source.
//! 3. **Text predicates** — `leading_spaces`, `is_identifier`,
//!    `is_type_name`, `is_ident_char`, `matches_word`. Indent-aware
//!    parsers throughout the doctor module compose over these.
//!    `leading_spaces` in particular is re-exported through the
//!    `doctor::` surface so the `returns_list_*` rule modules can
//!    keep calling `super::leading_spaces`.
//!
//! Plus three doctor-specific text scanners: `parse_export_name`
//! (extracts the identifier from a `.gen.ts` `export …` line),
//! `lazuli_version_line` (locates the `lazuli_version "x.y.z"` line
//! inside `Lazurite.toml`), and `extract_lzir_schema` (lifts the
//! `pub const LZIR_SCHEMA = "…"` literal out of
//! `crates/lazuli_ir/src/lib.rs`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::parsers::{is_lzi_path, is_lzx_path};

/// Walks `dist/ts-{web,mobile}/**/*.{gen.ts, gen.tsx}` invoking
/// `visit(path, contents)` for every matched file. Skips
/// `node_modules` and `go` subtrees.
pub(super) fn walk_gen_ts_files(root: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name == "node_modules" || name == "go" {
            continue;
        }
        if path.is_dir() {
            walk_gen_ts_files(&path, visit);
            continue;
        }
        if !name.ends_with(".gen.ts") && !name.ends_with(".gen.tsx") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        visit(&path, &contents);
    }
}

/// Walks `app/clients/<frontend>/src/**/*.{ts,tsx}` invoking
/// `visit(path, contents)` for every matched file. Skips
/// generated artifacts (`*.gen.ts`, `*.gen.tsx`), tests
/// (`*.test.*`, `*.spec.*`), and `node_modules`/`dist` subtrees.
pub(super) fn walk_frontend_ts_files(
    clients_root: &Path,
    visit: &mut dyn FnMut(&Path, &str),
) {
    let Ok(entries) = std::fs::read_dir(clients_root) else {
        return;
    };
    for entry in entries.flatten() {
        let client_root = entry.path();
        let src = client_root.join("src");
        if !src.is_dir() {
            continue;
        }
        walk_frontend_ts_files_recursive(&src, visit);
    }
}

/// Inner recursion for `walk_frontend_ts_files`. Filters
/// node_modules / dist / build subdirectories and the
/// `.gen.` / `.test.` / `.spec.` filename infixes.
pub(super) fn walk_frontend_ts_files_recursive(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if file_name == "node_modules" || file_name == "dist" || file_name == "build" {
            continue;
        }
        if path.is_dir() {
            walk_frontend_ts_files_recursive(&path, visit);
            continue;
        }
        if !file_name.ends_with(".ts") && !file_name.ends_with(".tsx") {
            continue;
        }
        if file_name.contains(".gen.")
            || file_name.contains(".test.")
            || file_name.contains(".spec.")
        {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        visit(&path, &contents);
    }
}

/// Walks `dist/ts-{web,mobile}/**/*.gen.ts` collecting export names
/// preceded by a `/** @deprecated ... */` jsdoc block.
pub(super) fn collect_deprecated_exports(
    dist_root: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) {
    walk_gen_ts_files(dist_root, &mut |path, contents| {
        let lines: Vec<&str> = contents.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("export ") {
                continue;
            }
            // Look at the previous 1-3 lines for a @deprecated marker.
            let mut found_deprecated = false;
            for prev in (idx.saturating_sub(3)..idx).rev() {
                let p = lines.get(prev).map(|s| s.trim()).unwrap_or("");
                if p.contains("@deprecated") {
                    found_deprecated = true;
                    break;
                }
                if p.starts_with("export ")
                    || (!p.starts_with("*") && !p.starts_with("//") && !p.is_empty())
                {
                    break;
                }
            }
            if !found_deprecated {
                continue;
            }
            // Extract the export name. Handles `export const X =`,
            // `export function X(`, `export type X =`, `export interface X {`.
            if let Some(name) = parse_export_name(trimmed) {
                out.insert(name, path.to_path_buf());
            }
        }
    });
}

/// Extract the identifier from a `.gen.ts` `export <keyword> NAME …`
/// line. Returns `None` when the line doesn't open with a recognised
/// declaration keyword.
pub(super) fn parse_export_name(line: &str) -> Option<String> {
    let after_export = line.strip_prefix("export ")?;
    for keyword in [
        "const ",
        "let ",
        "var ",
        "function ",
        "type ",
        "interface ",
        "class ",
    ] {
        if let Some(rest) = after_export.strip_prefix(keyword) {
            let ident: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                .collect();
            if ident.is_empty() {
                return None;
            }
            return Some(ident);
        }
    }
    None
}

/// `true` when `needle` appears in `haystack` with non-identifier
/// boundaries on both sides (whole-word match). Used by the
/// deprecated-alias scanner to avoid `Foo` matching `FooBar`.
pub(super) fn matches_word(haystack: &str, needle: &str) -> bool {
    let mut start = 0usize;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let end = abs + needle.len();
        let before_ok = abs == 0 || !is_ident_char(haystack.as_bytes()[abs - 1] as char);
        let after_ok = end >= haystack.len() || !is_ident_char(haystack.as_bytes()[end] as char);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Identifier-character predicate: ASCII alphanumeric plus `_` and `$`
/// (JS/TS identifier vocabulary). Used by `matches_word`.
pub(super) fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Recursively enumerate every `.lzi` and `.lzx` under `root`.
/// Errors propagate so the caller can surface a precise context;
/// the typical caller is `project_uses_plugin_refs` which falls back
/// to `false` on IO failure.
pub(super) fn collect_lazuli_paths_recursive(
    root: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_lazuli_paths_recursive(&path, paths)?;
        } else if path.is_file() && (is_lzi_path(&path) || is_lzx_path(&path)) {
            paths.push(path);
        }
    }
    Ok(())
}

/// Recursively enumerate every directory under `root` that contains
/// a `recipe.toml` file. Used by `check_migration_recipe_*` to walk
/// the `recipes/` tree from any nested starting point.
pub(super) fn collect_recipe_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("recipe.toml").is_file() {
            out.push(path);
        } else {
            collect_recipe_dirs(&path, out);
        }
    }
}

/// Strip the canonical "feature stem" from a `.lzi` / `.lzx` filename.
/// `customer.lzi` → `customer`; `customer.web.lzx` → `customer`
/// (the surface modifier is dropped).
pub(super) fn package_stem(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if let Some(stem) = file_name.strip_suffix(".lzi") {
        return Some(stem.to_owned());
    }

    let stem = file_name.strip_suffix(".lzx")?;
    Some(stem.split('.').next().unwrap_or(stem).to_owned())
}

/// PG.B — read the first `feature <name>` header from a `.lzi` source.
/// Returns `None` for app.lzi / registry.lzi / contracts that don't
/// declare a feature.
pub(super) fn derive_feature_name(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("feature ") {
            return rest.split_whitespace().next().map(|s| s.to_owned());
        }
    }
    None
}

/// Locate the `lazuli_version "x.y.z"` line inside `Lazurite.toml` so
/// `LAZULI-VERSION-001` / `_002` can anchor their diagnostics to the
/// exact authoring site.
pub(super) fn lazuli_version_line(source: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| {
            leading_spaces(line) == 2 && line.trim_start().starts_with("lazuli_version ")
        })
        .map(|line| line + 1)
}

/// Lift the `LZIR_SCHEMA = "x.y.z"` literal out of
/// `crates/lazuli_ir/src/lib.rs`. Used by the release migration
/// recipe diagnostics to compare HEAD~1 vs current schema.
pub(super) fn extract_lzir_schema(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("pub const LZIR_SCHEMA") {
            return None;
        }
        let (_, rest) = trimmed.split_once('"')?;
        let (value, _) = rest.split_once('"')?;
        Some(value.to_owned())
    })
}

/// Count leading ASCII spaces on a line. The doctor uses an
/// indent-aware text parser throughout — every "is this at depth 2 /
/// 4 / 6?" check composes over this helper.
///
/// `pub(crate)` (not `pub(super)`) so `doctor/mod.rs` can re-export
/// it back into the `doctor::` surface for the `returns_list_*` rule
/// modules that still call `super::leading_spaces`.
pub(crate) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

/// Lazuli identifier predicate: `[A-Za-z_][A-Za-z0-9_]*`. Distinct
/// from `is_ident_char` (which is the JS/TS vocabulary used by the
/// deprecated-alias scanner).
pub(super) fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Lazuli type-name predicate: `[A-Z][A-Za-z0-9_]*`. Constrains the
/// initial character to uppercase so the doctor can distinguish
/// `Customer` (type) from `customer` (identifier).
pub(super) fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
