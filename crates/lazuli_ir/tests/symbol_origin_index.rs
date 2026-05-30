use std::collections::BTreeMap;

use lazuli_ir::{
    Defaults, Feature, ImportEdge, Policies, SourceLocation, SpanRef, SymbolKind, SymbolOrigin,
    SymbolOriginIndex,
};

fn file_location(file: &str, line: u32, column: u32) -> SourceLocation {
    SourceLocation::File {
        file: file.to_owned(),
        line,
        column,
    }
}

/// Format the canonical `<feature>.<name>` key used by the index map.
/// When `feature` is None (e.g. built-in semantic types), the key is the
/// bare symbol name.
fn qualified_key(feature: Option<&str>, name: &str) -> String {
    match feature {
        Some(f) => format!("{}.{}", f, name),
        None => name.to_owned(),
    }
}

fn origin(name: &str, kind: SymbolKind, defined_at: SourceLocation) -> SymbolOrigin {
    SymbolOrigin {
        feature: "account".to_owned(),
        name: name.to_owned(),
        kind,
        defined_at,
        previous_names: Vec::new(),
        contract_version: None,
    }
}

fn minimal_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults::default(),
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies::default(),
        errors: None,
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        pollers: Vec::new(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: Vec::new(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

#[test]
fn empty_index_round_trips() {
    let index = SymbolOriginIndex::default();
    let json = serde_json::to_string(&index).expect("serialize");
    let parsed: SymbolOriginIndex = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, index);
    assert!(json.contains("\"symbols\":{}"));
    assert!(json.contains("\"imports\":{}"));
}

#[test]
fn file_defined_symbol_round_trips() {
    let mut index = SymbolOriginIndex::default();
    index.symbols.insert(
        qualified_key(Some("account"), "Gender"),
        origin(
            "Gender",
            SymbolKind::Enum,
            file_location("features/account/account.lzi", 12, 3),
        ),
    );

    let json = serde_json::to_string(&index).expect("serialize");
    assert!(json.contains("\"source\":\"file\""));
    assert!(json.contains("\"file\":\"features/account/account.lzi\""));
    assert!(json.contains("\"kind\":\"enum\""));

    let parsed: SymbolOriginIndex = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, index);
}

#[test]
fn builtin_symbol_uses_typed_discriminator() {
    let mut index = SymbolOriginIndex::default();
    index.symbols.insert(
        qualified_key(None, "Money"),
        SymbolOrigin {
            feature: "core".to_owned(),
            name: "Money".to_owned(),
            kind: SymbolKind::Semantic,
            defined_at: SourceLocation::Builtin,
            previous_names: Vec::new(),
            contract_version: None,
        },
    );

    let json = serde_json::to_string(&index).expect("serialize");
    assert!(json.contains("\"source\":\"builtin\""));
    assert!(!json.contains("<builtin>"));

    let parsed: SymbolOriginIndex = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, index);
}

#[test]
fn previous_names_skipped_when_empty() {
    let mut symbol = origin(
        "Gender",
        SymbolKind::Enum,
        file_location("features/account/account.lzi", 12, 3),
    );

    let json = serde_json::to_string(&symbol).expect("serialize without previous names");
    assert!(!json.contains("\"previous_names\""));

    symbol.previous_names = vec!["OldName".to_owned()];
    let json = serde_json::to_string(&symbol).expect("serialize with previous names");
    assert!(json.contains("\"previous_names\""));
}

#[test]
fn symbol_kind_enum_exhaustiveness() {
    let cases = [
        (SymbolKind::Enum, "enum"),
        (SymbolKind::Resource, "resource"),
        (SymbolKind::Record, "record"),
        (SymbolKind::Scalar, "scalar"),
        (SymbolKind::Semantic, "semantic"),
        (SymbolKind::Command, "command"),
        (SymbolKind::Query, "query"),
        (SymbolKind::Event, "event"),
        (SymbolKind::Aggregate, "aggregate"),
    ];

    for (kind, expected) in cases {
        let symbol = origin(
            "Thing",
            kind,
            file_location("features/account/account.lzi", 1, 1),
        );
        let json = serde_json::to_string(&symbol).expect("serialize symbol kind");
        assert!(
            json.contains(&format!("\"kind\":\"{expected}\"")),
            "expected kind discriminator {expected}, got {json}"
        );
    }
}

#[test]
fn import_edge_and_imports_map_shape() {
    let edge = ImportEdge {
        importer: "host".to_owned(),
        imported: "account".to_owned(),
        uses_at: file_location("features/host/host.lzi", 4, 1),
    };
    let mut index = SymbolOriginIndex {
        symbols: BTreeMap::new(),
        imports: BTreeMap::new(),
    };
    index.imports.insert("host".to_owned(), vec![edge.clone()]);

    let json = serde_json::to_string(&index).expect("serialize imports");
    assert!(json.contains("\"imports\":{\"host\":["));
    assert!(json.contains("\"importer\":\"host\""));
    assert!(json.contains("\"imported\":\"account\""));
    assert!(json.contains("\"uses_at\":{\"source\":\"file\""));
    assert!(json.contains("\"file\":\"features/host/host.lzi\""));

    let parsed: SymbolOriginIndex = serde_json::from_str(&json).expect("deserialize imports");
    assert_eq!(parsed.imports["host"], vec![edge]);
}

#[test]
fn feature_uses_spans_round_trips() {
    let mut feature = minimal_feature("host");
    feature.uses = vec!["account".to_owned()];
    feature.uses_spans = vec![SpanRef { start: 10, end: 20 }];

    let json = serde_json::to_string(&feature).expect("serialize feature with uses_spans");
    assert!(json.contains("\"uses\":[\"account\"]"));
    assert!(json.contains("\"uses_spans\":[{\"start\":10,\"end\":20}]"));

    let parsed: Feature = serde_json::from_str(&json).expect("deserialize feature with uses_spans");
    assert_eq!(parsed, feature);

    let empty = minimal_feature("host");
    let json = serde_json::to_string(&empty).expect("serialize feature without uses_spans");
    assert!(!json.contains("\"uses_spans\""));
}

#[test]
fn feature_without_uses_spans_deserializes_for_back_compat() {
    let feature = minimal_feature("host");
    let mut value = serde_json::to_value(&feature).expect("serialize feature as value");
    value
        .as_object_mut()
        .expect("feature serializes as object")
        .remove("uses_spans");
    let json = serde_json::to_string(&value).expect("serialize old feature JSON");

    let parsed: Feature = serde_json::from_str(&json).expect("deserialize old feature JSON");
    assert_eq!(parsed.uses_spans, Vec::<SpanRef>::new());
}
