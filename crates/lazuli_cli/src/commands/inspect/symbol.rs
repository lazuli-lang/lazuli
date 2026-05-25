//! Symbol-mode inspect: `lazuli inspect symbol:<name>`.
//!
//! When the inspect CLI is invoked with a bare or dotted symbol name
//! (no path), the dispatcher in `inspect_command` routes here instead
//! of the path-mode projector. The flow:
//!
//! 1. `inspect_symbol_arg` decides whether the input is a symbol
//!    (heuristic: no path separators, no `.lzi` suffix, doesn't
//!    exist on disk).
//! 2. `inspect_symbol_command` builds the project module + symbol
//!    origin index, then dispatches to `inspect_symbol_lookup`.
//! 3. `inspect_symbol_lookup` walks every feature for a matching
//!    symbol, collects candidates, and emits one of three shapes:
//!    found, not-found, or ambiguous (multi-candidate).
//! 4. `render_inspect_symbol_lazuli` renders the JSON payload into
//!    a human-friendly `--format=lazuli` view (used by the LSP for
//!    go-to-definition hover).
//!
//! This module exposes only one entry — `inspect_symbol_command`
//! (kept private to the inspect tree, dispatched from `mod.rs`) —
//! and the `render_inspect_symbol_lazuli` formatter, which the LSP
//! reaches through a re-export in `crate::commands::inspect`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::InspectFormat;
use crate::build_module_from_path;

pub(super) fn inspect_symbol_arg(input: &Path) -> Option<&str> {
    let s = input.to_str()?;
    if s.is_empty() || s == "." || s == ".." {
        return None;
    }
    if s.contains('/') || s.contains('\\') {
        return None;
    }
    if s.ends_with(".lzi") {
        return None;
    }
    if input.exists() {
        return None;
    }
    Some(s)
}

/// Symbol-mode dispatch: build the SymbolOriginIndex from the project root
/// and emit JSON for the requested symbol per §5.2 / §5.4.
pub(super) fn inspect_symbol_command(symbol: &str, format: InspectFormat) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    let project_root = inspect_symbol_project_root(&cwd);
    let module = build_module_from_path(&project_root)?;
    let source_map = lazuli_ir::SourceMap { files: Vec::new() };
    let index = lazuli_analyzer::build_symbol_origin_index(&module, &source_map);

    let output = inspect_symbol_lookup(symbol, &module, &index);
    match format {
        InspectFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        InspectFormat::Lazuli => {
            println!("{}", render_inspect_symbol_lazuli(symbol, &output));
        }
    }
    Ok(())
}

/// Render a `lazuli inspect <symbol>` JSON result as compact
/// human-readable lines for terminal viewers (closes the
/// `--format=lazuli for symbol-mode` next-checklist item). The JSON
/// shape stays normative; this is a one-screen view that surfaces
/// the four facts a reader usually wants: kind + feature + path:line
/// + previous names (when present).
pub(crate) fn render_inspect_symbol_lazuli(symbol: &str, output: &serde_json::Value) -> String {
    if let Some(error) = output.get("error") {
        let code = error
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("ERROR");
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(no message)");
        let mut lines = vec![format!("{code}: {message}")];
        if let Some(candidates) = error.get("candidates").and_then(|v| v.as_array()) {
            for c in candidates {
                if let Some(s) = c.as_str() {
                    lines.push(format!("  - {s}"));
                }
            }
        }
        return lines.join("\n");
    }

    let name = output
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or(symbol);
    let feature = output
        .get("feature")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let kind = output
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("symbol");
    let defined_in = output
        .get("defined_in")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let location = match (
        defined_in.get("file").and_then(|v| v.as_str()),
        defined_in.get("line").and_then(|v| v.as_u64()),
    ) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.to_owned(),
        _ => match defined_in.get("source").and_then(|v| v.as_str()) {
            Some("builtin") => "builtin".to_owned(),
            _ => "?".to_owned(),
        },
    };

    let mut lines = vec![format!(
        "{name} ({kind}) — feature `{feature}`, defined in: {location}"
    )];

    if let Some(prev) = output.get("previous_names").and_then(|v| v.as_array()) {
        if !prev.is_empty() {
            let names: Vec<&str> = prev.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() {
                lines.push(format!("  previously: {}", names.join(", ")));
            }
        }
    }

    if let Some(imported) = output.get("imported_via").and_then(|v| v.as_object()) {
        if let Some(feat) = imported.get("feature").and_then(|v| v.as_str()) {
            // Optional line/file anchor for the `uses <feat>` clause.
            let uses_anchor = imported
                .get("uses_at")
                .and_then(|v| v.as_object())
                .and_then(|obj| {
                    let file = obj.get("file").and_then(|v| v.as_str());
                    let line = obj.get("line").and_then(|v| v.as_u64());
                    match (file, line) {
                        (Some(f), Some(l)) => Some(format!(" at {f}:{l}")),
                        (Some(f), None) => Some(format!(" at {f}")),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            lines.push(format!("  imported via: uses {feat}{uses_anchor}"));
        }
    }

    lines.join("\n")
}

/// Find the project root by walking up from `start` for a directory that
/// contains `Lazurite.toml`. Falls back to `start` itself when no manifest
/// is found — `build_module_from_path` will still produce something useful
/// from a single-feature dir.
fn inspect_symbol_project_root(start: &Path) -> PathBuf {
    let mut cursor: Option<&Path> = Some(start);
    while let Some(dir) = cursor {
        if dir.join("Lazurite.toml").is_file() || dir.join("lazurite.toml").is_file() {
            return dir.to_path_buf();
        }
        cursor = dir.parent();
    }
    start.to_path_buf()
}

/// Resolve a symbol query against the index. Returns a `serde_json::Value`
/// matching the JSON shapes in `docs/proposals/lsp-symbol-origin.md` §5.2,
/// §5.4 (error shapes).
fn inspect_symbol_lookup(
    symbol: &str,
    module: &lazuli_ir::Module,
    index: &lazuli_ir::SymbolOriginIndex,
) -> serde_json::Value {
    // Step 1: parse the symbol into (qualifier, name).
    let (qualifier, name) = match symbol.split_once('.') {
        Some((q, n)) => (Some(q.to_owned()), n.to_owned()),
        None => (None, symbol.to_owned()),
    };

    // Step 2: find candidate keys in the index.
    let candidates: Vec<&str> = match &qualifier {
        Some(feature_or_alias) => {
            // Qualified: look up `<qualifier>.<name>` directly. The qualifier
            // is the FEATURE that contains the symbol, regardless of which
            // feature triggered the inspect (uses-clause resolution would
            // need an analyzer pass; out of scope for the bare lookup).
            let key = format!("{}.{}", feature_or_alias, name);
            if index.symbols.contains_key(&key) {
                vec![index
                    .symbols
                    .get_key_value(&key)
                    .map(|(k, _)| k.as_str())
                    .unwrap()]
            } else {
                Vec::new()
            }
        }
        None => {
            // Bare name: walk all symbols matching `*.<name>`.
            index
                .symbols
                .iter()
                .filter(|(_, origin)| origin.name == name)
                .map(|(k, _)| k.as_str())
                .collect()
        }
    };

    // Step 3: when the qualifier is provided AND the symbol is NOT
    // defined in the qualified feature itself, check whether the
    // qualified feature imports a feature that defines it. This is
    // the `imported_via: uses account` case from
    // `docs/proposals/lsp-symbol-origin.md` §5.2 — a feature can
    // re-export a type by `uses`-ing the feature that owns it. The
    // qualified key lookup at step 2 already returns the direct
    // match (e.g. `account.Gender`); here we additionally consider
    // the cross-feature `host.Gender → uses account` resolution.
    let imported_via = qualifier
        .as_ref()
        .and_then(|consumer| resolve_imported_via(consumer, &name, index));

    // Step 4: re-resolve candidates against the imported edge when
    // the direct qualified lookup yielded nothing.
    let candidates = if candidates.is_empty() {
        if let Some((owning_feature, _)) = imported_via.as_ref() {
            let key = format!("{}.{}", owning_feature, name);
            if index.symbols.contains_key(&key) {
                vec![key]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        candidates.into_iter().map(|s| s.to_owned()).collect()
    };

    // Step 5: branch on candidate count.
    match candidates.len() {
        0 => inspect_symbol_not_found(&qualifier, &name, module, index),
        1 => inspect_symbol_found(&candidates[0], &qualifier, &name, index, imported_via.as_ref()),
        _ => inspect_symbol_ambiguous(
            &name,
            &candidates.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ),
    }
}

/// When the consumer feature `<consumer>` `uses <other>` and `<other>`
/// defines `<name>`, return `(other_feature, ImportEdge)` so the
/// caller can populate `imported_via` in the inspect output. Returns
/// `None` when the consumer doesn't import the owning feature.
fn resolve_imported_via(
    consumer: &str,
    name: &str,
    index: &lazuli_ir::SymbolOriginIndex,
) -> Option<(String, lazuli_ir::ImportEdge)> {
    let edges = index.imports.get(consumer)?;
    for edge in edges {
        let candidate_key = format!("{}.{}", edge.imported, name);
        if index.symbols.contains_key(&candidate_key) {
            return Some((edge.imported.clone(), edge.clone()));
        }
    }
    None
}

fn inspect_symbol_found(
    key: &str,
    qualifier: &Option<String>,
    name: &str,
    index: &lazuli_ir::SymbolOriginIndex,
    imported_via: Option<&(String, lazuli_ir::ImportEdge)>,
) -> serde_json::Value {
    let origin = index.symbols.get(key).expect("key exists by construction");
    let imported_via_json = match imported_via {
        Some((owning, edge)) => serde_json::json!({
            "feature": owning,
            "uses_at": match &edge.uses_at {
                lazuli_ir::SourceLocation::File { file, line, column } => serde_json::json!({
                    "source": "file",
                    "file": file,
                    "line": line,
                    "column": column,
                }),
                lazuli_ir::SourceLocation::Builtin => serde_json::json!({"source": "builtin"}),
            },
        }),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "symbol": name,
        "feature": qualifier.clone().unwrap_or_else(|| origin.feature.clone()),
        "defined_in": {
            "source": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { .. } => "file",
                lazuli_ir::SourceLocation::Builtin => "builtin",
            },
            "file": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { file, .. } => Some(file.clone()),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "line": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { line, .. } => Some(*line),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "column": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { column, .. } => Some(*column),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "kind": symbol_kind_str(&origin.kind),
        },
        "imported_via": imported_via_json,
        "type": symbol_kind_str(&origin.kind),
        "previous_names": origin.previous_names,
    })
}

fn inspect_symbol_not_found(
    qualifier: &Option<String>,
    name: &str,
    _module: &lazuli_ir::Module,
    _index: &lazuli_ir::SymbolOriginIndex,
) -> serde_json::Value {
    let message = match qualifier {
        Some(q) => format!(
            "no declaration named `{}` in feature `{}` or any imported feature",
            name, q
        ),
        None => format!("no declaration named `{}` in any feature of this project", name),
    };
    serde_json::json!({
        "error": {
            "code": "SYMBOL_NOT_FOUND",
            "message": message,
        }
    })
}

fn inspect_symbol_ambiguous(name: &str, candidates: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "error": {
            "code": "AMBIGUOUS_SYMBOL",
            "message": format!("`{}` is declared in multiple features; qualify the lookup as `<feature>.{}`", name, name),
            "candidates": candidates,
        }
    })
}

fn symbol_kind_str(kind: &lazuli_ir::SymbolKind) -> &'static str {
    match kind {
        lazuli_ir::SymbolKind::Enum => "enum",
        lazuli_ir::SymbolKind::Resource => "resource",
        lazuli_ir::SymbolKind::Record => "record",
        lazuli_ir::SymbolKind::Scalar => "scalar",
        lazuli_ir::SymbolKind::Semantic => "semantic",
        lazuli_ir::SymbolKind::Command => "command",
        lazuli_ir::SymbolKind::Query => "query",
        lazuli_ir::SymbolKind::Event => "event",
        lazuli_ir::SymbolKind::Aggregate => "aggregate",
    }
}
