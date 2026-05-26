//! Diagnostics for plugin-backed semantic scalar fixture coverage.
//!
//! The check stays filesystem-local: it reads the active plugin set from
//! `Lazurite.toml`, then inspects only each declared plugin root. There is no
//! global plugin discovery or network resolution.
//!
//! Rails-style R9 layout: the JS-flavored fixture-file lexer + object key
//! extractor lives in [`object_lex`]; tests live alongside in
//! [`tests`]. The slice you're reading owns workspace manifest loading,
//! plugin resolution, `.lzi` traversal, and the public [`check`] entry.

mod object_lex;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::SpanRef;
use serde::Deserialize;

use object_lex::{fixture_keys_for_plugin, load_package_json};

const WORKSPACE_MANIFEST: &str = "Lazurite.toml";
const LEGACY_WORKSPACE_MANIFEST: &str = "lazurite.toml";
const PLUGIN_MANIFEST: &str = "manifest.toml";
#[cfg(test)]
const PACKAGE_JSON: &str = "package.json";
const FIXTURES_EXPORT: &str = "./fixtures";

/// Severity bucket — scalar-fixtures findings are advisory and surface
/// as warnings (never block the build).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Visible in diagnostics-only mode; does not block the build.
    Warning,
}

/// One scalar-fixtures finding (plugin-coverage hint).
///
/// Carries the file path the finding belongs to so doctor / LSP can
/// route it to the right document — most other check passes use a
/// `SpanRef` alone, but here the source is a JS package, not a `.lzi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Severity bucket — currently always [`Severity::Warning`].
    pub severity: Severity,
    /// File the finding belongs to (plugin manifest, `.lzi`, fixture).
    pub path: PathBuf,
    /// Source span within `path` for IDE underlining; may be `None`.
    pub span: Option<SpanRef>,
    /// Human-readable message — already formatted, no interpolation.
    pub message: String,
}

#[derive(Debug, Deserialize, Default)]
struct WorkspaceManifest {
    #[serde(default)]
    plugins: BTreeMap<String, Plugin>,
    #[serde(default)]
    dev: DevOverrides,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Plugin {
    Local { path: String },
    Remote { module: String, version: String },
}

#[derive(Debug, Deserialize, Default)]
struct DevOverrides {
    #[serde(default)]
    plugin_paths: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Default)]
struct PluginManifest {
    #[serde(default)]
    semantic_types: Vec<SemanticTypeDecl>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SemanticTypeDecl {
    #[serde(default)]
    name: String,
    #[serde(default)]
    alias: Option<String>,
}

struct ActivePlugin {
    namespace: String,
    root: PathBuf,
    manifest_path: PathBuf,
    semantic_types: Vec<SemanticTypeDecl>,
}

/// Walk `project_root` for scalar-fixture coverage gaps in active
/// plugins declared by `Lazurite.toml`.
///
/// Reads the workspace manifest, resolves the active plugin set (with
/// `[dev.plugin_paths]` overrides), and emits one [`Diagnostic`] per
/// fixture file / plugin gap. The pass is filesystem-local: no
/// network resolution, no global plugin discovery.
///
/// ## Examples
///
/// ```no_run
/// use std::path::Path;
/// use lazuli_analyzer::checks::scalar_fixtures;
///
/// let diags = scalar_fixtures::check(Path::new("."));
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn check(project_root: &Path) -> Vec<Diagnostic> {
    let manifest = load_workspace_manifest(project_root);
    let active_plugins = manifest
        .as_ref()
        .map(|manifest| active_plugins(manifest, project_root))
        .unwrap_or_default();
    let declared_semantic_types = declared_semantic_types(&active_plugins);

    let mut diagnostics = semantic_reference_diagnostics(project_root, &declared_semantic_types);
    for plugin in &active_plugins {
        if plugin.semantic_types.is_empty() {
            continue;
        }

        let package = load_package_json(&plugin.root);
        let fixtures_export = package
            .as_ref()
            .and_then(|package| package.get("exports"))
            .and_then(|exports| exports.get(FIXTURES_EXPORT));

        if fixtures_export.is_none() {
            diagnostics.push(Diagnostic {
                code: "SCALAR-FIXTURES-002",
                severity: Severity::Warning,
                path: plugin.manifest_path.clone(),
                span: None,
                message: format!(
                    "plugin `{}` declares `[[semantic_types]]` but its `package.json` does not export `{}`.",
                    plugin.namespace, FIXTURES_EXPORT
                ),
            });
            continue;
        }

        let fixture_keys = fixture_keys_for_plugin(&plugin.root, fixtures_export);
        for claimed in claimed_type_names(&plugin.semantic_types) {
            if fixture_keys.contains(&claimed) {
                continue;
            }
            diagnostics.push(Diagnostic {
                code: "SCALAR-FIXTURES-003",
                severity: Severity::Warning,
                path: plugin.manifest_path.clone(),
                span: None,
                message: format!(
                    "plugin `{}` claims semantic type `{}` but `fixtures.ts` does not export it in the top-level `fixtures` map.",
                    plugin.namespace, claimed
                ),
            });
        }
    }

    diagnostics.sort_by(|a, b| {
        (&a.path, span_key(a.span), a.code, a.message.as_str()).cmp(&(
            &b.path,
            span_key(b.span),
            b.code,
            b.message.as_str(),
        ))
    });
    diagnostics
}

fn load_workspace_manifest(project_root: &Path) -> Option<WorkspaceManifest> {
    let canonical = project_root.join(WORKSPACE_MANIFEST);
    let legacy = project_root.join(LEGACY_WORKSPACE_MANIFEST);
    let path = if canonical.is_file() {
        canonical
    } else if legacy.is_file() {
        legacy
    } else {
        return None;
    };
    let raw = fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

fn active_plugins(manifest: &WorkspaceManifest, project_root: &Path) -> Vec<ActivePlugin> {
    let mut out = Vec::new();
    for namespace in manifest.plugins.keys() {
        let Some(root) = resolve_plugin_root(manifest, project_root, namespace) else {
            continue;
        };
        let manifest_path = root.join(PLUGIN_MANIFEST);
        let Ok(raw) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(plugin_manifest) = toml::from_str::<PluginManifest>(&raw) else {
            continue;
        };
        out.push(ActivePlugin {
            namespace: namespace.clone(),
            root,
            manifest_path,
            semantic_types: plugin_manifest.semantic_types,
        });
    }
    out
}

fn resolve_plugin_root(
    manifest: &WorkspaceManifest,
    project_root: &Path,
    namespace: &str,
) -> Option<PathBuf> {
    if let Some(path) = manifest.dev.plugin_paths.get(namespace) {
        return Some(absolutise(project_root, Path::new(path)));
    }

    match manifest.plugins.get(namespace)? {
        Plugin::Local { path } => Some(absolutise(project_root, Path::new(path))),
        Plugin::Remote { module, version } => {
            let _ = (module, version);
            None
        }
    }
}

fn absolutise(project_root: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

fn declared_semantic_types(plugins: &[ActivePlugin]) -> BTreeSet<String> {
    plugins
        .iter()
        .flat_map(|plugin| claimed_type_names(&plugin.semantic_types))
        .collect()
}

fn claimed_type_names(entries: &[SemanticTypeDecl]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for entry in entries {
        let name = entry.name.trim();
        if !name.is_empty() {
            names.insert(name.to_owned());
        }
        if let Some(alias_name) = entry.alias.as_deref().and_then(semantic_alias_terminal) {
            names.insert(alias_name.to_owned());
        }
    }
    names
}

fn semantic_alias_terminal(alias: &str) -> Option<&str> {
    let rest = alias.strip_prefix("@semantic.")?;
    let end = rest
        .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

fn semantic_reference_diagnostics(
    project_root: &Path,
    declared_semantic_types: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut files = Vec::new();
    collect_lzi_files(project_root, &mut files);

    let mut out = Vec::new();
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for reference in semantic_references(&source) {
            if is_builtin_semantic(&reference.name) {
                continue;
            }
            if declared_semantic_types.contains(&reference.name) {
                continue;
            }
            out.push(Diagnostic {
                code: "SCALAR-FIXTURES-001",
                severity: Severity::Warning,
                path: path.clone(),
                span: Some(SpanRef {
                    start: reference.start,
                    end: reference.end,
                }),
                message: format!(
                    "`@semantic.{}` is referenced but no active plugin in `Lazurite.toml` declares semantic type `{}`.",
                    reference.name, reference.name
                ),
            });
        }
    }
    out
}

struct SemanticReference {
    name: String,
    start: usize,
    end: usize,
}

fn semantic_references(source: &str) -> Vec<SemanticReference> {
    let mut references = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = source[offset..].find("@semantic.") {
        let start = offset + relative_start;
        if start > 0 {
            let previous = source.as_bytes()[start - 1];
            if previous.is_ascii_alphanumeric() || previous == b'_' {
                offset = start + 1;
                continue;
            }
        }

        let name_start = start + "@semantic.".len();
        let name_len = semantic_name_len(&source[name_start..]);
        if name_len == 0 {
            offset = name_start;
            continue;
        }

        let end = name_start + name_len;
        references.push(SemanticReference {
            name: source[name_start..end].to_owned(),
            start,
            end,
        });
        offset = end;
    }
    references
}

fn semantic_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count()
}

fn is_builtin_semantic(name: &str) -> bool {
    matches!(
        name,
        "Email"
            | "Phone"
            | "URL"
            | "Url"
            | "UUID"
            | "Uuid"
            | "Date"
            | "Currency"
            | "Money"
            | "JSON"
            | "Json"
            | "GeoPoint"
    )
}

fn collect_lzi_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if matches!(
                name,
                ".git" | ".lazuli" | "dist" | "node_modules" | "target"
            ) {
                continue;
            }
            collect_lzi_files(&path, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("lzi"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn span_key(span: Option<SpanRef>) -> (usize, usize) {
    span.map(|span| (span.start, span.end)).unwrap_or((0, 0))
}
