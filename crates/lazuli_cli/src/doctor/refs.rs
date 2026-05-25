//! `@`-reference + plugin-reference scanners.
//!
//! Two passes the doctor + lazurite-manifest aggregator share:
//!
//! 1. **`@<namespace>.<name>` scanning** —
//!    [`collect_at_references_in_source`] walks a source string and
//!    yields one [`AtReferenceFact`] per syntactic reference. The
//!    namespace allowlist for doctor-level cross-checks lives in
//!    [`is_allowed_reference_namespace_for_doctor`] (kept narrow so
//!    new namespaces stay co-located with the `lazuli_lsp` parser-side
//!    catalog).
//!
//! 2. **`@lazuli/plugin-<name>` scanning** —
//!    [`collect_package_plugin_references`] + the source-level
//!    [`collect_plugin_references_in_source`] yield one
//!    [`PluginReferenceFact`] per `@lazuli/plugin-…` literal. Used by
//!    the lazurite-manifest aggregator to cross-check actually-used
//!    plugin slots against the manifest's declared `plugins`.
//!
//! Plus the path-reference walker [`path_references`] (env / route
//! reference extraction) and the `go.mod` reader
//! [`go_mod_lazuli_runtime_version`] for `lazuli.dev/runtime` pinning.
//!
//! Extracted from `doctor/mod.rs` in rails-style R6-2.

use std::path::{Path, PathBuf};

use crate::doctor::helpers::line_col_for_offset;

/// Walk `source` for `<prefix><name>` tokens and yield every captured
/// `<name>` slice. Reference names are `_` + alphanumerics, with one
/// extension: `{axis}` segments are captured verbatim so callers can
/// recognise tenant-keyed env references like
/// `env.CRYPT_KEY_TENANT_{tenant_id}` as a single token instead of
/// truncating at the `{`.
pub(crate) fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after_prefix = &rest[start + prefix.len()..];
        let bytes = after_prefix.as_bytes();
        let mut end = 0;
        let mut in_brace = false;
        while end < bytes.len() {
            let ch = bytes[end] as char;
            if in_brace {
                if ch == '}' {
                    in_brace = false;
                    end += 1;
                    continue;
                }
                end += 1;
                continue;
            }
            if ch == '{' {
                in_brace = true;
                end += 1;
                continue;
            }
            if ch == '_' || ch.is_ascii_alphanumeric() {
                end += 1;
                continue;
            }
            break;
        }
        if end > 0 {
            references.push(&after_prefix[..end]);
        }
        rest = &after_prefix[end..];
    }

    references
}

#[derive(Debug, Clone)]
pub(crate) struct PluginReferenceFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) reference: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AtReferenceFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) reference: String,
    pub(crate) namespace: String,
    pub(crate) name: String,
}

pub(crate) fn collect_package_plugin_references(
    package: &crate::doctor::DoctorPackage,
) -> Vec<PluginReferenceFact> {
    package
        .files
        .iter()
        .filter(|file| crate::doctor::parsers::is_lzi_path(&file.path))
        .flat_map(|file| collect_plugin_references_in_source(&file.path, &file.source))
        .collect()
}

pub(crate) fn collect_plugin_references_in_source(
    path: &Path,
    source: &str,
) -> Vec<PluginReferenceFact> {
    let mut references = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = source[offset..].find("@lazuli/plugin-") {
        let start = offset + relative_start;
        let after_prefix = &source[start + "@lazuli/plugin-".len()..];
        let name_len = plugin_reference_name_len(after_prefix);
        if name_len > 0 {
            let (line, column) = line_col_for_offset(source, start);
            references.push(PluginReferenceFact {
                path: path.to_path_buf(),
                line,
                column,
                reference: source[start..start + "@lazuli/plugin-".len() + name_len].to_owned(),
            });
        }
        offset = start + "@lazuli/plugin-".len() + name_len.max(1);
    }
    references
}

pub(crate) fn collect_at_references_in_source(path: &Path, source: &str) -> Vec<AtReferenceFact> {
    let mut references = Vec::new();
    let bytes = source.as_bytes();
    let mut offset = 0;

    while let Some(relative_start) = source[offset..].find('@') {
        let start = offset + relative_start;
        if start > 0 {
            let previous = bytes[start - 1];
            if previous.is_ascii_alphanumeric() || previous == b'_' {
                offset = start + 1;
                continue;
            }
        }

        let after_at = &source[start + 1..];
        let namespace_len = after_at
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if namespace_len == 0 {
            offset = start + 1;
            continue;
        }

        let namespace = &after_at[..namespace_len];
        let separator = after_at.as_bytes().get(namespace_len).copied();
        if separator != Some(b'.') && separator != Some(b'/') {
            offset = start + 1 + namespace_len;
            continue;
        }

        let name_start = start + 1 + namespace_len + 1;
        let name_len = reference_name_len(&source[name_start..]);
        if name_len == 0 {
            offset = name_start;
            continue;
        }

        let (line, column) = line_col_for_offset(source, start);
        references.push(AtReferenceFact {
            path: path.to_path_buf(),
            line,
            column,
            reference: source[start..name_start + name_len].to_owned(),
            namespace: namespace.to_owned(),
            name: source[name_start..name_start + name_len].to_owned(),
        });
        offset = name_start + name_len;
    }

    references
}

pub(crate) fn plugin_reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| {
            byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.' | b'/')
        })
        .count()
}

pub(crate) fn reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'/'))
        .count()
}

pub(crate) fn reference_namespace(reference: &str) -> Option<&str> {
    let after_at = reference.strip_prefix('@')?;
    let end = after_at.find(['.', '/']).unwrap_or(after_at.len());
    (end > 0).then_some(&after_at[..end])
}

/// Doctor-side namespace catalog. Mirrors `lazuli_lsp`'s
/// `is_allowed_reference_namespace`, but lives here so the doctor
/// aggregators can cross-check `@<ns>.<name>` references without
/// reaching into the LSP crate's private items.
pub(crate) fn is_allowed_reference_namespace_for_doctor(namespace: &str) -> bool {
    matches!(
        namespace,
        "role"
            | "scope"
            | "actor"
            | "policy"
            | "semantic"
            | "cap"
            | "pii"
            | "key"
            | "fn"
            | "hook"
            | "validator"
            | "adapter"
            | "client"
            | "query_modifier"
            | "anchor"
            | "llm"
            | "tool"
            | "trace"
    )
}

/// Best-effort `lazuli.dev/runtime <version>` reader for a `go.mod`
/// source. Returns the trimmed (unquoted) version literal when found;
/// `None` otherwise.
pub(crate) fn go_mod_lazuli_runtime_version(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || !trimmed.contains("lazuli.dev/runtime") {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        while let Some(part) = parts.next() {
            if part == "lazuli.dev/runtime" {
                return parts
                    .next()
                    .map(|version| version.trim_matches('"').to_owned());
            }
        }
    }
    None
}
