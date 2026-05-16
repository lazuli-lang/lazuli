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
pub fn build_symbol_origin_index(
    module: &ir::Module,
    source_map: &SourceMap,
) -> SymbolOriginIndex {
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

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, EnumDecl, Feature, Module, Policies, Resource, SourceFile, Command,
        CommandEffect, CommandInput, CommandKind, PolicyRef,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            pollers: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn empty_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    fn empty_source_map() -> SourceMap {
        SourceMap { files: vec![] }
    }

    fn source_map_for(feature_name: &str, source: &str) -> SourceMap {
        SourceMap {
            files: vec![SourceMap::build_source_file(
                FileId::from(1u8),
                format!("features/{}/{}.lzi", feature_name, feature_name),
                source,
            )],
        }
    }

    fn make_resource(name: &str) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
        }
    }

    fn make_enum(name: &str) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            public_contract: None,
            variants: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn make_command(name: &str) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn empty_module_yields_empty_index() {
        let module = empty_module(vec![]);
        let index = build_symbol_origin_index(&module, &empty_source_map());
        assert!(index.symbols.is_empty());
        assert!(index.imports.is_empty());
    }

    #[test]
    fn feature_with_enum_emits_one_symbol() {
        let mut feature = empty_feature("account");
        feature.enums.push(make_enum("Gender"));
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());
        assert_eq!(index.symbols.len(), 1);
        let origin = index
            .symbols
            .get("account.Gender")
            .expect("Gender indexed");
        assert_eq!(origin.feature, "account");
        assert_eq!(origin.name, "Gender");
        assert!(matches!(origin.kind, SymbolKind::Enum));
    }

    #[test]
    fn feature_with_multiple_symbol_kinds_emits_one_per_kind() {
        let mut feature = empty_feature("billing");
        feature.enums.push(make_enum("InvoiceStatus"));
        feature.resources.push(make_resource("Invoice"));
        feature.commands.push(make_command("archive_invoice"));

        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        assert_eq!(index.symbols.len(), 3);
        let kinds: Vec<SymbolKind> = index.symbols.values().map(|o| o.kind).collect();
        assert!(kinds.contains(&SymbolKind::Enum));
        assert!(kinds.contains(&SymbolKind::Resource));
        assert!(kinds.contains(&SymbolKind::Command));
    }

    #[test]
    fn feature_uses_clause_emits_import_edge() {
        let mut feature = empty_feature("host");
        feature.uses.push("account".into());
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        let edges = index.imports.get("host").expect("host imports indexed");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].importer, "host");
        assert_eq!(edges[0].imported, "account");
    }

    #[test]
    fn no_uses_means_no_imports_entry() {
        let feature = empty_feature("solo");
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());
        assert!(index.imports.is_empty());
    }

    #[test]
    fn span_ref_resolves_to_file_source_location() {
        let mut feature = empty_feature("account");
        let mut r#enum = make_enum("Gender");
        // line 3 starts at byte 14 in "line1\nline2\nline3\nline4"; resolve via SourceMap.
        r#enum.span_ref = Some(SpanRef { start: 14, end: 20 });
        feature.enums.push(r#enum);

        let source = "line1\nline2\nline3\nline4";
        let source_map = source_map_for("account", source);
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &source_map);

        let origin = index
            .symbols
            .get("account.Gender")
            .expect("Gender indexed");
        match &origin.defined_at {
            SourceLocation::File { file, line, column } => {
                assert_eq!(file, "features/account/account.lzi");
                assert_eq!(*line, 3);
                assert!(*column >= 1);
            }
            SourceLocation::Builtin => panic!("expected File source location"),
        }
    }

    #[test]
    fn missing_span_ref_yields_unresolved_sentinel() {
        let mut feature = empty_feature("account");
        feature.enums.push(make_enum("Gender")); // span_ref: None
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        let origin = index.symbols.get("account.Gender").unwrap();
        match &origin.defined_at {
            SourceLocation::File { file, line, column } => {
                assert_eq!(file, "<unresolved>");
                assert_eq!(*line, 0);
                assert_eq!(*column, 0);
            }
            SourceLocation::Builtin => panic!("Builtin is reserved for compiler types only"),
        }
    }

    #[test]
    fn previous_names_propagate_to_symbol_origin() {
        let mut feature = empty_feature("account");
        let mut r#enum = make_enum("Gender");
        r#enum.previous_names = vec!["LegacyGender".into()];
        feature.enums.push(r#enum);
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        let origin = index.symbols.get("account.Gender").unwrap();
        assert_eq!(origin.previous_names, vec!["LegacyGender".to_string()]);
    }

    #[test]
    fn public_contract_propagates_to_symbol_origin() {
        // Per docs/proposals/cross-feature-contracts.md §5.1 — when an
        // enum/resource/record/command/query carries `public contract X as
        // v<N>`, the IR's PublicContract.version flows into the index's
        // SymbolOrigin.contract_version field.
        let mut feature = empty_feature("account");
        let mut r#enum = make_enum("Gender");
        r#enum.public_contract = Some(lazuli_ir::PublicContract {
            version: 3,
            span_ref: None,
        });
        feature.enums.push(r#enum);
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        let origin = index.symbols.get("account.Gender").unwrap();
        assert_eq!(origin.contract_version, Some(3));
    }

    #[test]
    fn missing_public_contract_yields_none_contract_version() {
        let mut feature = empty_feature("account");
        feature.enums.push(make_enum("Gender")); // public_contract: None
        let module = empty_module(vec![feature]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        let origin = index.symbols.get("account.Gender").unwrap();
        assert_eq!(origin.contract_version, None);
    }

    #[test]
    fn two_features_index_with_qualified_names() {
        let mut account = empty_feature("account");
        account.enums.push(make_enum("Gender"));
        let mut host = empty_feature("host");
        host.resources.push(make_resource("Listing"));
        host.uses.push("account".into());

        let module = empty_module(vec![account, host]);
        let index = build_symbol_origin_index(&module, &empty_source_map());

        // Two symbols, one per feature.
        assert_eq!(index.symbols.len(), 2);
        // host imports account.
        assert_eq!(index.imports.get("host").unwrap().len(), 1);
        // account has no imports.
        assert!(!index.imports.contains_key("account"));
    }
}
