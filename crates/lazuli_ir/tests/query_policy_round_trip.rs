//! Round-trip test for the per-query `policy` / `policy_expr` slots
//! (QUERY-POLICY-001). Mirrors the Cell IR-1 round-trip test for
//! commands, but covers each `Query` variant
//! (`Query::List` / `Query::Lookup` / `Query::Sql`) authoring a
//! non-`None` `PolicyRef::Local(_)` and asserting:
//!
//!   1. `serde_json::to_string` keeps the `policy` slot on the wire.
//!   2. `serde_json::from_str` round-trips the value back equal to the
//!      original `Feature`.
//!   3. `PolicyRef::None` defaults still round-trip, so legacy fixtures
//!      stay backward-compatible.
//!
//! Field evidence — hostpoint pilot stack reported
//! `command/query registered with empty policy` for every authored
//! `policy @policy.traveler_only` line on `query.lookup` decls. Closing
//! the IR gap requires the field to exist *and* survive serialization.

use lazuli_ir::{
    BuiltinType, Defaults, Feature, KeyClause, ListQuery, LookupQuery, Path, Policies, PolicyRef,
    Query, SqlQuery, TypeRef,
};

fn base_feature() -> Feature {
    Feature {
        name: "traveler".to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies {
            categories: Vec::new(),
            fields: Vec::new(),
            span_ref: None,
        },
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
        span_ref: None,
    }
}

#[test]
fn list_query_with_policy_round_trips() {
    let mut feature = base_feature();
    feature.queries.push(Query::List(ListQuery {
        name: "my_traveler".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        policy: PolicyRef::Local("traveler_only".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let json = serde_json::to_string(&feature).expect("serialize");
    let back: Feature = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(feature, back);
    assert!(
        json.contains("\"traveler_only\""),
        "list-query policy name missing from JSON: {json}"
    );
}

#[test]
fn lookup_query_with_policy_round_trips() {
    let mut feature = base_feature();
    feature.queries.push(Query::Lookup(LookupQuery {
        name: "my_traveler".to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: vec![KeyClause {
            path: Path::from_segments(["id".to_owned()]),
            equals: lazuli_ir::Expr::Path(Path::from_segments(["id".to_owned()])),
        }],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: PolicyRef::Local("traveler_only".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let json = serde_json::to_string(&feature).expect("serialize");
    let back: Feature = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(feature, back);
    assert!(
        json.contains("\"traveler_only\""),
        "lookup-query policy name missing from JSON: {json}"
    );
}

#[test]
fn sql_query_with_policy_round_trips() {
    let mut feature = base_feature();
    feature.queries.push(Query::Sql(SqlQuery {
        name: "traveler_audit".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        returns: TypeRef::Builtin(BuiltinType::Boolean),
        sql_path: "./queries/traveler_audit.sql".to_owned(),
        cache: None,
        policy: PolicyRef::Atom("role.admin".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let json = serde_json::to_string(&feature).expect("serialize");
    let back: Feature = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(feature, back);
    assert!(
        json.contains("\"role.admin\""),
        "sql-query policy atom missing from JSON: {json}"
    );
}

#[test]
fn query_without_policy_omits_field_from_json() {
    // Backward-compat: an empty `PolicyRef::None` must skip on the
    // wire so existing fixtures keep round-tripping without diff.
    let mut feature = base_feature();
    feature.queries.push(Query::List(ListQuery {
        name: "list_all".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let json = serde_json::to_string(&feature).expect("serialize");
    // The `policy` slot uses `skip_serializing_if = "PolicyRef::is_none"`,
    // so an empty query authors no `"policy"` field at all.
    assert!(
        !json.contains("\"policy\":{\"kind\":\"None\"}"),
        "query policy=None should skip serialization: {json}"
    );
    let back: Feature = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(feature, back);
}
