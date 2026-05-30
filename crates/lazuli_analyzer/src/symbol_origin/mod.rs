//! `build_symbol_origin_index` — analyzer walker that builds the
//! `SymbolOriginIndex` sidecar from a lowered `Module` + `SourceMap`.
//!
//! Per `docs/proposals/lsp-symbol-origin.md` §6.3.
//!
//! The walker is **side-effect-free** — it inspects an already-lowered IR
//! and produces a parallel index of symbol origins + import edges. The
//! consumer (LSP hover, `lazuli inspect <qualified-symbol>`, future
//! audit-skill v2) reads the index without re-resolving anything.
//!
//! Symbol kinds covered (matches `SymbolKind` closed catalog):
//!   - `Enum` from `Feature.enums`
//!   - `Resource` from `Feature.resources`
//!   - `Record` from `Feature.records`
//!   - `Command` from `Feature.commands`
//!   - `Query` from `Feature.queries` (all three sub-kinds collapse to `Query`)
//!   - `Event` from `Feature.events`
//!   - `Aggregate` from `Feature.aggregates`
//!
//! `Scalar` is reserved per L0 #4 scalar aliases; `Semantic` is reserved
//! for built-in `@semantic.*` types and is populated by a future pass that
//! seeds the index with the closed semantic catalog (not this cell).
//!
//! Span resolution: each symbol carries `span_ref: Option<SpanRef>`. When
//! present, the walker resolves it to `SourceLocation::File { file, line,
//! column }` via the `SourceMap`. When absent (test fixtures, manual IR
//! construction), the walker emits a sentinel
//! `SourceLocation::File { file: "<unresolved>", line: 0, column: 0 }` —
//! `Builtin` is reserved for compiler-provided types only.

use std::collections::BTreeMap;

use lazuli_ir::{
    self as ir, FileId, ImportEdge, Query, SourceLocation, SourceMap, SpanRef, SymbolKind,
    SymbolOrigin, SymbolOriginIndex,
};

use crate::source_map::SourceMapResolver;

/// Format a fully-qualified symbol key as `<feature>.<name>`.
fn qualified_key(feature: &str, name: &str) -> String {
    format!("{}.{}", feature, name)
}

/// Build the symbol origin index for `module`.
///
/// `source_map` is used to resolve `SpanRef` → `(file, line, column)`. The
/// walker scans `source_map.files` to find the file id matching each feature
/// (by matching the file path against the feature name); if no match exists,
/// the walker uses `FileId(1)` as a best-effort fallback and resolution may
/// produce sentinel `<unresolved>` locations.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::symbol_origin::build_symbol_origin_index;
/// use lazuli_ir::{Module, SourceMap};
///
/// let module: Module = unimplemented!();
/// let source_map: SourceMap = unimplemented!();
/// let index = build_symbol_origin_index(&module, &source_map);
/// assert!(index.symbols.iter().all(|(k, _)| !k.is_empty()));
/// ```
pub fn build_symbol_origin_index(module: &ir::Module, source_map: &SourceMap) -> SymbolOriginIndex {
    let mut symbols: BTreeMap<String, SymbolOrigin> = BTreeMap::new();
    let mut imports: BTreeMap<String, Vec<ImportEdge>> = BTreeMap::new();

    for feature in &module.features {
        let file_id = feature_file_id(feature, source_map);

        // -- Symbols ----------------------------------------------------------
        for r#enum in &feature.enums {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &r#enum.name,
                SymbolKind::Enum,
                r#enum.span_ref,
                &r#enum.previous_names,
                r#enum.public_contract.as_ref().map(|c| c.version),
            );
        }
        for resource in &feature.resources {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &resource.name,
                SymbolKind::Resource,
                resource.span_ref,
                &resource.previous_names,
                resource.public_contract.as_ref().map(|c| c.version),
            );
        }
        for record in &feature.records {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &record.name,
                SymbolKind::Record,
                record.span_ref,
                &[], // Record doesn't currently carry previous_names; add when it does
                record.public_contract.as_ref().map(|c| c.version),
            );
        }
        for command in &feature.commands {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &command.name,
                SymbolKind::Command,
                command.span_ref,
                &command.previous_names,
                command.public_contract.as_ref().map(|c| c.version),
            );
        }
        for query in &feature.queries {
            let (name, span_ref, previous_names, contract_version) = query_facts(query);
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                name,
                SymbolKind::Query,
                span_ref,
                previous_names,
                contract_version,
            );
        }
        for event in &feature.events {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &event.name,
                SymbolKind::Event,
                event.span_ref,
                &event.previous_names,
                None, // Events don't carry public_contract yet (proposal §3.4 / §5.3 row 6 — follow-up cell)
            );
        }
        for aggregate in &feature.aggregates {
            insert_symbol(
                &mut symbols,
                feature,
                file_id,
                source_map,
                &aggregate.name,
                SymbolKind::Aggregate,
                aggregate.span_ref,
                &[],
                None, // Aggregates may not span features under microservices (proposal §3 non-goal 9)
            );
        }

        // -- Imports ----------------------------------------------------------
        // `feature.uses` is the list of imported feature names; `uses_spans`
        // is the parallel span list populated by the analyzer when lowering
        // from source. When uses_spans is empty (manual IR construction),
        // each ImportEdge gets an unresolved location.
        let edges: Vec<ImportEdge> = feature
            .uses
            .iter()
            .enumerate()
            .map(|(i, imported)| ImportEdge {
                importer: feature.name.clone(),
                imported: imported.clone(),
                uses_at: resolve_or_unresolved(
                    feature.uses_spans.get(i).copied(),
                    file_id,
                    source_map,
                ),
            })
            .collect();
        if !edges.is_empty() {
            imports.insert(feature.name.clone(), edges);
        }
    }

    SymbolOriginIndex { symbols, imports }
}

// -- Internals ----------------------------------------------------------------

fn insert_symbol(
    symbols: &mut BTreeMap<String, SymbolOrigin>,
    feature: &ir::Feature,
    file_id: FileId,
    source_map: &SourceMap,
    name: &str,
    kind: SymbolKind,
    span_ref: Option<SpanRef>,
    previous_names: &[String],
    contract_version: Option<u16>,
) {
    let key = qualified_key(&feature.name, name);
    let origin = SymbolOrigin {
        feature: feature.name.clone(),
        name: name.to_owned(),
        kind,
        defined_at: resolve_or_unresolved(span_ref, file_id, source_map),
        previous_names: previous_names.to_vec(),
        contract_version,
    };
    symbols.insert(key, origin);
}

fn query_facts(query: &Query) -> (&str, Option<SpanRef>, &[String], Option<u16>) {
    match query {
        Query::List(q) => (
            q.name.as_str(),
            q.span_ref,
            q.previous_names.as_slice(),
            q.public_contract.as_ref().map(|c| c.version),
        ),
        Query::Lookup(q) => (
            q.name.as_str(),
            q.span_ref,
            q.previous_names.as_slice(),
            q.public_contract.as_ref().map(|c| c.version),
        ),
        Query::Sql(q) => (
            q.name.as_str(),
            q.span_ref,
            q.previous_names.as_slice(),
            q.public_contract.as_ref().map(|c| c.version),
        ),
        // query.compose: W2/W3 — symbol-origin facts are real (same
        // name/span/previous_names/contract fields as the other kinds).
        Query::Compose(q) => (
            q.name.as_str(),
            q.span_ref,
            q.previous_names.as_slice(),
            q.public_contract.as_ref().map(|c| c.version),
        ),
    }
}

/// Find the `FileId` whose path looks like it owns this feature, by matching
/// `features/<feature>/<feature>.lzi` or `<feature>.lzi` against the
/// `SourceMap.files` path strings. Falls back to `FileId(1)` (the
/// conventional first-file id) when no match is found.
fn feature_file_id(feature: &ir::Feature, source_map: &SourceMap) -> FileId {
    for file in &source_map.files {
        let path = file.path.as_str();
        let basename = path.rsplit(['/', '\\']).next().unwrap_or(path);
        if basename == format!("{}.lzi", feature.name) {
            return file.id;
        }
    }
    FileId::from(1u8)
}

/// Resolve a `SpanRef` to `SourceLocation::File` via the source map, or fall
/// back to a sentinel `<unresolved>` file when the span is absent OR
/// resolution fails (file id missing, span out of bounds).
fn resolve_or_unresolved(
    span: Option<SpanRef>,
    file_id: FileId,
    source_map: &SourceMap,
) -> SourceLocation {
    if let Some(span) = span
        && let Some(loc) = source_map.resolve(file_id, span)
    {
        return SourceLocation::File {
            file: loc.file,
            line: loc.line,
            column: loc.column,
        };
    }
    SourceLocation::File {
        file: "<unresolved>".to_owned(),
        line: 0,
        column: 0,
    }
}


#[cfg(test)]
mod tests;
