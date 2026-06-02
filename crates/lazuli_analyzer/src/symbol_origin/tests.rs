//! Tests for `symbol_origin` — extracted from `mod.rs` (Rails-style R9
//! split). Production code stays in the parent module.

use super::*;
use lazuli_ir::{
    Command, CommandEffect, CommandInput, CommandKind, Defaults, EnumDecl, Feature, Module,
    Policies, PolicyRef, Resource,
};

fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.into(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        knowledge: None,
        defaults: Defaults::default(),
        uses: vec![],
        uses_spans: vec![],
        uses_versions: vec![],
        requirements: vec![],
        enums: vec![],
        resources: vec![],
        events: vec![],
        rules: vec![],
        policies: Policies::default(),
        errors: None,
        commands: vec![],
        apis: vec![],
        records: vec![],
        queries: vec![],
        resume_routers: vec![],
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
        mcp_servers: vec![],
        previous_names: vec![],
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
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
        doctor_allows: Vec::new(),
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
        soft_delete_actor: false,
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
        conventions: vec![],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
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
        policy_when_denied: None,
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
        handler: None,
        tests: None,
        triggers: vec![],
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: vec![],
        span_ref: None,
        derived_from: None,
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
    let origin = index.symbols.get("account.Gender").expect("Gender indexed");
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

    let origin = index.symbols.get("account.Gender").expect("Gender indexed");
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
