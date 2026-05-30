//! Plugin catalog emitter — `dist/plugin-catalog.json`.
//!
//! Walks every plugin declared in `Lazurite.toml [plugins]` (skipping
//! remote plugins without a local dev override) and emits a single
//! JSON file that consolidates each plugin's manifest + README excerpt
//! + parsed Go/TS exports. Apps, docs sites, the LSP, and the (future)
//! `lazuli plugins` CLI all consume the same file.
//!
//! Spec: `docs/proposals/plugin-catalog-file-2026-05-23.md` (lazuli-ops).
//!
//! Wire-thin: no extra dependencies (no regex crate, no markdown
//! parser). Export extraction uses simple line-prefix matching — good
//! enough for the v1 catalog surface; if a plugin's exports are
//! mis-detected the README remains authoritative.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::lazurite_manifest::Manifest;
use crate::plugin_manifest::{
    self, PLUGIN_MANIFEST_FILENAME, PluginManifest, PluginSemanticTypeDecl,
};

const CATALOG_SCHEMA_VERSION: u32 = 1;

/// Emit `dist/plugin-catalog.json` content for the given project.
/// Returns `None` when no plugins are declared (or all are skipped
/// because they're remote-without-override). Output is deterministic
/// (alphabetical namespace order, alphabetical semantic-type order,
/// alphabetical export order); regenerating with no plugin changes
/// produces byte-identical output.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::plugin_catalog::emit_plugin_catalog;
///
/// // let json = emit_plugin_catalog(&manifest, Path::new("."));
/// ```
pub fn emit_plugin_catalog(manifest: &Manifest, project_root: &Path) -> Option<String> {
    let mut entries: Vec<CatalogPlugin> = Vec::new();
    for namespace in manifest.plugins.keys() {
        let Some(root) = plugin_manifest::resolve_plugin_root(manifest, project_root, namespace)
        else {
            continue;
        };
        let manifest_path = root.join(PLUGIN_MANIFEST_FILENAME);
        if !manifest_path.exists() {
            // PLUGIN-MANIFEST-MISSING already flags this at doctor
            // time; catalog quietly skips so a missing manifest
            // doesn't break the build.
            continue;
        }
        let Ok(Some(plugin)) = plugin_manifest::load_plugin_manifest(&root) else {
            continue;
        };
        let Some(identity) = plugin.plugin.as_ref() else {
            continue;
        };
        let entry = build_entry(namespace, &root, &plugin, identity, project_root);
        entries.push(entry);
    }
    if entries.is_empty() {
        return None;
    }
    entries.sort_by(|a, b| a.namespace.cmp(&b.namespace));

    let doc = json!({
        "schema": CATALOG_SCHEMA_VERSION,
        "plugins": entries,
    });
    let mut s = serde_json::to_string_pretty(&doc).ok()?;
    s.push('\n');
    Some(s)
}

fn build_entry(
    namespace: &str,
    root: &Path,
    plugin: &PluginManifest,
    identity: &plugin_manifest::PluginIdentity,
    project_root: &Path,
) -> CatalogPlugin {
    let manifest_path = root
        .join(PLUGIN_MANIFEST_FILENAME)
        .strip_prefix(project_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| {
            root.join(PLUGIN_MANIFEST_FILENAME)
                .to_string_lossy()
                .replace('\\', "/")
        });

    let readme_excerpt = read_readme_excerpt(root);
    let description = identity
        .description
        .clone()
        .or_else(|| readme_first_paragraph(&readme_excerpt));

    let semantic_types: Vec<Value> = plugin
        .semantic_types
        .iter()
        .map(render_semantic_type)
        .collect::<Vec<_>>()
        .into_iter()
        .collect();

    CatalogPlugin {
        namespace: namespace.to_owned(),
        name: identity.name.clone(),
        description,
        go_module: identity.go_module.clone(),
        ts_package: identity.ts_package.clone(),
        manifest_path,
        semantic_types,
        exports: CatalogExports {
            go: extract_go_exports(root),
            ts: extract_ts_exports(root),
        },
        readme_excerpt,
    }
}

fn render_semantic_type(decl: &PluginSemanticTypeDecl) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), Value::String(decl.name.clone()));
    obj.insert("alias".into(), Value::String(decl.alias.clone()));
    obj.insert(
        "carrier_type".into(),
        Value::String(decl.carrier_type.clone()),
    );
    obj.insert("validator".into(), Value::String(decl.validator.clone()));
    if let Some(fmt) = &decl.formatter {
        obj.insert("formatter".into(), Value::String(fmt.clone()));
    }
    if let Some(code) = &decl.error_code {
        obj.insert("error_code".into(), Value::String(code.clone()));
    }
    if let Some(key) = &decl.message_key {
        obj.insert("message_key".into(), Value::String(key.clone()));
    }
    if let Some(ts) = &decl.ts_validator {
        obj.insert("ts_validator".into(), Value::String(ts.clone()));
    }
    Value::Object(obj)
}

/// Read README.md and return the first 3 paragraphs (joined with
/// blank-line separators), stripped of fenced code blocks. Empty
/// when README is missing or unreadable.
fn read_readme_excerpt(root: &Path) -> Option<String> {
    let readme_path = root.join("README.md");
    let raw = fs::read_to_string(&readme_path).ok()?;
    let stripped = strip_fenced_blocks(&raw);
    let paragraphs: Vec<&str> = stripped
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .take(3)
        .collect();
    if paragraphs.is_empty() {
        return None;
    }
    Some(paragraphs.join("\n\n"))
}

/// Pull the first paragraph (sans leading `#` heading) from a
/// pre-stripped README excerpt. Used as a `description` fallback
/// when the manifest didn't author one.
fn readme_first_paragraph(excerpt: &Option<String>) -> Option<String> {
    let s = excerpt.as_deref()?;
    for paragraph in s.split("\n\n").map(|p| p.trim()) {
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.starts_with('#') {
            continue;
        }
        return Some(paragraph.to_owned());
    }
    None
}

/// Drop ```fenced``` code blocks from markdown source so the
/// catalog's excerpt doesn't carry raw code/imports.
fn strip_fenced_blocks(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_fence = false;
    for line in src.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Walk plugin root for `*.go` files (excluding `_test.go`) and
/// collect every `^func <Name>` and `^type <Name>` that starts with
/// an upper-case letter (Go export discipline). Sorted, dedup'd.
fn extract_go_exports(root: &Path) -> Vec<String> {
    let mut exports = Vec::new();
    walk_dir(root, &mut |path| {
        if path.extension().and_then(|e| e.to_str()) != Some("go") {
            return;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_test.go"))
        {
            return;
        }
        let Ok(src) = fs::read_to_string(path) else {
            return;
        };
        for line in src.lines() {
            if let Some(name) = parse_go_export(line) {
                exports.push(name);
            }
        }
    });
    finalize_export_list(exports)
}

fn parse_go_export(line: &str) -> Option<String> {
    let line = line.trim_start();
    let rest = line
        .strip_prefix("func ")
        .or_else(|| line.strip_prefix("type "))?;
    // Drop receiver clause `(c Client) Foo(...)` — pick the token after `)`.
    let rest = if let Some(stripped) = rest.strip_prefix('(') {
        let close = stripped.find(')')?;
        stripped[close + 1..].trim_start()
    } else {
        rest
    };
    let name = rest
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()?;
    if name.is_empty() {
        return None;
    }
    if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some(name.to_owned())
}

/// Walk plugin's `src/` for `.ts` and `.tsx` files (excluding
/// `.test.ts`) and collect every `^export (?:declare |async )?(?:function|const|class|interface|type|enum) <Name>`
/// plus `^export { A, B as C }` re-exports.
fn extract_ts_exports(root: &Path) -> Vec<String> {
    let mut exports = Vec::new();
    let src_dir = root.join("src");
    if !src_dir.exists() {
        return Vec::new();
    }
    walk_dir(&src_dir, &mut |path| {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        if !(name.ends_with(".ts") || name.ends_with(".tsx")) {
            return;
        }
        if name.ends_with(".test.ts") || name.ends_with(".test.tsx") {
            return;
        }
        let Ok(src) = fs::read_to_string(path) else {
            return;
        };
        for name in parse_ts_exports(&src) {
            exports.push(name);
        }
    });
    finalize_export_list(exports)
}

fn parse_ts_exports(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("export ") else {
            continue;
        };
        // `export type { Foo }` / `export { Foo }` re-export forms
        // — extract the names between braces.
        let after_optional = rest
            .strip_prefix("type ")
            .or_else(|| rest.strip_prefix("declare "))
            .or_else(|| rest.strip_prefix("async "))
            .unwrap_or(rest);
        if let Some(brace_start) = after_optional.strip_prefix('{') {
            let inner = brace_start.split('}').next().unwrap_or("");
            for part in inner.split(',') {
                let token = part.trim();
                let name = token
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    out.push(name.to_owned());
                }
            }
            continue;
        }
        // `export (function|const|class|interface|type|enum) <name>`
        for kw in [
            "function ",
            "const ",
            "let ",
            "var ",
            "class ",
            "interface ",
            "type ",
            "enum ",
        ] {
            if let Some(after) = after_optional.strip_prefix(kw) {
                let name = after
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    out.push(name.to_owned());
                }
                break;
            }
        }
    }
    out
}

fn finalize_export_list(mut exports: Vec<String>) -> Vec<String> {
    exports.sort();
    exports.dedup();
    exports
}

fn walk_dir(root: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(read) = fs::read_dir(root) else { return };
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        // Skip nested vendor/node_modules/dist directories so we
        // don't pick up un-related Go/TS files.
        if entry.is_dir() {
            if name == "node_modules" || name == "vendor" || name == "dist" || name.starts_with('.')
            {
                continue;
            }
            walk_dir(&entry, visit);
        } else {
            visit(&entry);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CatalogPlugin {
    namespace: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    go_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ts_package: Option<String>,
    manifest_path: String,
    semantic_types: Vec<Value>,
    exports: CatalogExports,
    #[serde(skip_serializing_if = "Option::is_none")]
    readme_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CatalogExports {
    go: Vec<String>,
    ts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_go_export_skips_lowercase() {
        assert_eq!(parse_go_export("func helper() {}"), None);
        assert_eq!(
            parse_go_export("func PublicAPI()"),
            Some("PublicAPI".to_owned())
        );
    }

    #[test]
    fn parse_ts_exports_captures_const_and_braces() {
        let names = parse_ts_exports("export const Foo = 1;\nexport { Bar, Baz };\n");
        assert!(names.contains(&"Foo".to_owned()));
        assert!(names.contains(&"Bar".to_owned()));
        assert!(names.contains(&"Baz".to_owned()));
    }
}
