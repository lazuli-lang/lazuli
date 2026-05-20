//! Diagnostics for plugin-backed semantic scalar fixture coverage.
//!
//! The check stays filesystem-local: it reads the active plugin set from
//! `Lazurite.toml`, then inspects only each declared plugin root. There is no
//! global plugin discovery or network resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::SpanRef;
use serde::Deserialize;
use serde_json::Value;

const WORKSPACE_MANIFEST: &str = "Lazurite.toml";
const LEGACY_WORKSPACE_MANIFEST: &str = "lazurite.toml";
const PLUGIN_MANIFEST: &str = "manifest.toml";
const PACKAGE_JSON: &str = "package.json";
const FIXTURES_EXPORT: &str = "./fixtures";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub path: PathBuf,
    pub span: Option<SpanRef>,
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

fn load_package_json(plugin_root: &Path) -> Option<Value> {
    let raw = fs::read_to_string(plugin_root.join(PACKAGE_JSON)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn fixture_keys_for_plugin(
    plugin_root: &Path,
    fixtures_export: Option<&Value>,
) -> BTreeSet<String> {
    for candidate in fixture_source_candidates(plugin_root, fixtures_export) {
        let Ok(source) = fs::read_to_string(candidate) else {
            continue;
        };
        let keys = parse_fixtures_const_keys(&source);
        if !keys.is_empty() {
            return keys;
        }
    }
    BTreeSet::new()
}

fn fixture_source_candidates(plugin_root: &Path, fixtures_export: Option<&Value>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = fixtures_export {
        let mut export_paths = Vec::new();
        collect_export_paths(value, &mut export_paths);
        for export_path in export_paths {
            if export_path.ends_with(".ts") && export_path.contains("fixtures") {
                candidates.push(plugin_root.join(export_path.trim_start_matches("./")));
            }
        }
    }

    candidates.push(plugin_root.join("fixtures.ts"));
    candidates.push(plugin_root.join("src").join("fixtures.ts"));
    dedupe_paths(candidates)
}

fn collect_export_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(path) => out.push(path.clone()),
        Value::Array(values) => {
            for value in values {
                collect_export_paths(value, out);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_export_paths(value, out);
            }
        }
        _ => {}
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn parse_fixtures_const_keys(source: &str) -> BTreeSet<String> {
    let Some((body_start, body_end)) = find_fixtures_object_body(source) else {
        return BTreeSet::new();
    };
    parse_object_keys(&source[body_start..body_end])
}

fn find_fixtures_object_body(source: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("fixtures") {
        let name_start = offset + relative;
        let name_end = name_start + "fixtures".len();
        if !identifier_boundary(source, name_start, name_end) {
            offset = name_end;
            continue;
        }
        if !line_before(source, name_start).contains("const") {
            offset = name_end;
            continue;
        }

        let after_name = &source[name_end..];
        let eq_relative = after_name.find('=')?;
        if after_name[..eq_relative].contains(';') {
            offset = name_end;
            continue;
        }
        let eq = name_end + eq_relative;
        let open_relative = source[eq + 1..].find('{')?;
        let open = eq + 1 + open_relative;
        let close = matching_brace(source, open)?;
        return Some((open + 1, close));
    }
    None
}

fn identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = start
        .checked_sub(1)
        .and_then(|idx| source.as_bytes().get(idx))
        .copied();
    let after = source.as_bytes().get(end).copied();
    !is_ident_byte(before) && !is_ident_byte(after)
}

fn is_ident_byte(byte: Option<u8>) -> bool {
    byte.map(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        .unwrap_or(false)
}

fn line_before(source: &str, offset: usize) -> &str {
    let start = source[..offset].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    &source[start..offset]
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => i = skip_string(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_object_keys(body: &str) -> BTreeSet<String> {
    let bytes = body.as_bytes();
    let mut keys = BTreeSet::new();
    let mut i = 0;

    while i < bytes.len() {
        i = skip_ws_and_comments(bytes, i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b',' {
            i += 1;
            continue;
        }
        if bytes.get(i..i + 3) == Some(b"...") {
            i = skip_value(bytes, i + 3);
            continue;
        }

        let Some((key, after_key)) = parse_key(bytes, i) else {
            i += 1;
            continue;
        };
        let after_key = skip_ws_and_comments(bytes, after_key);
        if bytes.get(after_key) == Some(&b':') {
            keys.insert(key);
            i = skip_value(bytes, after_key + 1);
        } else if after_key >= bytes.len() || bytes.get(after_key) == Some(&b',') {
            keys.insert(key);
            i = after_key.saturating_add(1);
        } else {
            i = after_key.saturating_add(1);
        }
    }

    keys
}

fn parse_key(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    match bytes.get(i).copied()? {
        b'\'' | b'"' => parse_string_key(bytes, i),
        byte if byte == b'_' || byte.is_ascii_alphabetic() => {
            let mut end = i + 1;
            while bytes
                .get(end)
                .map(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .unwrap_or(false)
            {
                end += 1;
            }
            Some((String::from_utf8_lossy(&bytes[i..end]).into_owned(), end))
        }
        _ => None,
    }
}

fn parse_string_key(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(i)?;
    let mut out = String::new();
    let mut j = i + 1;
    while j < bytes.len() {
        let byte = bytes[j];
        if byte == b'\\' {
            if let Some(next) = bytes.get(j + 1).copied() {
                out.push(next as char);
                j += 2;
                continue;
            }
            return None;
        }
        if byte == quote {
            return Some((out, j + 1));
        }
        out.push(byte as char);
        j += 1;
    }
    None
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while bytes
            .get(i)
            .map(|b| b.is_ascii_whitespace())
            .unwrap_or(false)
        {
            i += 1;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'/') {
            i = skip_line_comment(bytes, i);
            continue;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
            i = skip_block_comment(bytes, i);
            continue;
        }
        return i;
    }
}

fn skip_value(bytes: &[u8], mut i: usize) -> usize {
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => i = skip_string(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            b'(' => {
                paren += 1;
                i += 1;
            }
            b')' => {
                paren = paren.saturating_sub(1);
                i += 1;
            }
            b'[' => {
                bracket += 1;
                i += 1;
            }
            b']' => {
                bracket = bracket.saturating_sub(1);
                i += 1;
            }
            b'{' => {
                brace += 1;
                i += 1;
            }
            b'}' => {
                if paren == 0 && bracket == 0 && brace == 0 {
                    return i;
                }
                brace = brace.saturating_sub(1);
                i += 1;
            }
            b',' if paren == 0 && bracket == 0 && brace == 0 => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_string(bytes: &[u8], i: usize) -> usize {
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j = (j + 2).min(bytes.len());
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 2;
    while j < bytes.len() && bytes[j] != b'\n' {
        j += 1;
    }
    j
}

fn skip_block_comment(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == b'*' && bytes[j + 1] == b'/' {
            return j + 2;
        }
        j += 1;
    }
    bytes.len()
}

fn span_key(span: Option<SpanRef>) -> (usize, usize) {
    span.map(|span| (span.start, span.end)).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }

    fn write_lazurite(root: &Path, plugin_path: &Path) {
        write(
            &root.join(WORKSPACE_MANIFEST),
            &format!(
                r#"
[project]
name = "fixture-app"
module = "example.com/fixture-app"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@plugin/scalars-br" = {{ path = "{}" }}
"#,
                plugin_path.display().to_string().replace('\\', "\\\\")
            ),
        );
    }

    fn write_plugin_manifest(plugin: &Path, semantic_type: &str) {
        write(
            &plugin.join(PLUGIN_MANIFEST),
            &format!(
                r#"
[plugin]
name = "scalars-br"
namespace = "@plugin/scalars-br"

[[semantic_types]]
name = "{semantic_type}"
alias = "@semantic.{semantic_type}"
carrier_type = "String"
validator = "Validate{semantic_type}"
"#
            ),
        );
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn diagnostic_001_unknown_semantic_reference() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("features/customer/customer.lzi"),
            r#"
feature customer
  domain
    resource Customer
      cpf: @semantic.BrazilianCPF
"#,
        );

        let diagnostics = check(tmp.path());

        assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-001"]);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert_eq!(
            &tmp.path().join("features/customer/customer.lzi"),
            &diagnostics[0].path
        );
        let span = diagnostics[0].span.expect("reference span");
        assert_eq!(
            &fs::read_to_string(&diagnostics[0].path).unwrap()[span.start..span.end],
            "@semantic.BrazilianCPF"
        );
    }

    #[test]
    fn diagnostic_002_manifest_semantic_types_without_fixtures_export() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugins/scalars-br");
        write_lazurite(tmp.path(), &plugin);
        write_plugin_manifest(&plugin, "BrazilianCPF");
        write(
            &plugin.join(PACKAGE_JSON),
            r#"{ "name": "@lazuli/scalars-br", "exports": { ".": "./index.ts" } }"#,
        );

        let diagnostics = check(tmp.path());

        assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-002"]);
        assert_eq!(diagnostics[0].path, plugin.join(PLUGIN_MANIFEST));
    }

    #[test]
    fn diagnostic_003_manifest_type_missing_from_fixtures_map() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugins/scalars-br");
        write_lazurite(tmp.path(), &plugin);
        write_plugin_manifest(&plugin, "BrazilianCPF");
        write(
            &plugin.join(PACKAGE_JSON),
            r#"{ "name": "@lazuli/scalars-br", "exports": { "./fixtures": "./fixtures.ts" } }"#,
        );
        write(
            &plugin.join("fixtures.ts"),
            r#"
export const fixtures = {
  BrazilianCNPJ: {
    valid: ["11222333000181"],
  },
};
"#,
        );

        let diagnostics = check(tmp.path());

        assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-003"]);
        assert!(diagnostics[0].message.contains("BrazilianCPF"));
    }

    #[test]
    fn happy_path_has_no_scalar_fixture_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugins/scalars-br");
        write_lazurite(tmp.path(), &plugin);
        write_plugin_manifest(&plugin, "BrazilianCPF");
        write(
            &plugin.join(PACKAGE_JSON),
            r#"{ "name": "@lazuli/scalars-br", "exports": { "./fixtures": "./fixtures.ts" } }"#,
        );
        write(
            &plugin.join("fixtures.ts"),
            r#"
export const fixtures = {
  BrazilianCPF: {
    valid: ["11144477735"],
  },
};
"#,
        );
        write(
            &tmp.path().join("features/customer/customer.lzi"),
            r#"
feature customer
  domain
    resource Customer
      cpf: @semantic.BrazilianCPF
      email: @semantic.Email
"#,
        );

        let diagnostics = check(tmp.path());

        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }
}
