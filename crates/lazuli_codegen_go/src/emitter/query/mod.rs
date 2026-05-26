//! Cell E4 - `Query` kind emission. Walks every `Query` declared on a
//! feature and emits typed args structs plus `lazuli.Query[A, R]`
//! values into `<feature>/query.gen.go`.
//!
//! Proposal references:
//! - section 3.3 - `Query.List` / `Query.Lookup` / `Query.Sql` value shape.
//! - section 11 - boundary discipline: every typed arg / SQL return value
//!   flows through `types::go_type_for` so imports stay centralised.
//!
//! Determinism: queries are sorted by `(name, kind)` before emission.
//! Query child vectors (`params`, `filters`, `order`, `keys`) preserve
//! IR order because that order mirrors source order and is meaningful
//! for SQL predicates / order clauses.

use lazuli_ir::{Feature, Query};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::TypeCtx;

mod util;
use util::query_kind_rank;
pub(super) use util::resource_for_query;

mod args;
use args::register_imports_for_query;

mod header;
use header::{query_callable_kind, query_policy_denied_key};

mod filters;

mod sql;
use sql::emit_sql_query;

mod list;
use list::emit_list_query;
pub(super) use list::list_var_name;

mod lookup;
use lookup::emit_lookup_query;
pub(super) use lookup::lookup_var_name;

#[cfg(test)]
mod test_support;

/// Emit `<feature>/query.gen.go` for a feature, or `None` when the
/// feature declares no queries.
pub fn emit_query_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.queries.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut queries: Vec<&Query> = feature.queries.iter().collect();
    queries.sort_by(|a, b| {
        a.name()
            .cmp(b.name())
            .then_with(|| query_kind_rank(a).cmp(&query_kind_rank(b)))
    });

    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    for query in &queries {
        register_imports_for_query(query, feature, &type_ctx, &mut imports);
    }
    // PG.C.2 — gated queries import `billing` (GateRef / GateBehind /
    // GateQuota) and `<module>/plan` (the package-wide Catalog;
    // unused here but kept in scope for handler-authored
    // `billing.CheckFeature(ctx, plan.Catalog, ...)` calls).
    let any_gated = queries.iter().any(|q| {
        !emit_ctx
            .gates_for(query_callable_kind(q), q.name())
            .is_empty()
    });
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }
    if queries
        .iter()
        .any(|q| query_policy_denied_key(feature, q).is_some())
    {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for query in &queries {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_query(&mut p, feature, query, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

fn emit_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &Query,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    match query {
        Query::List(q) => emit_list_query(p, feature, q, ctx, emit_ctx),
        Query::Lookup(q) => emit_lookup_query(p, feature, q, ctx, emit_ctx),
        Query::Sql(q) => emit_sql_query(p, feature, q, ctx, emit_ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{
        base_feature, emit, emit_from_module, field, module_with_features, qname, record, resource,
        slot,
    };
    use lazuli_ir::{
        BuiltinType, CacheTtl, CacheTtlLiteral, CompareOp, Expr, Filter, Gate, KeyClause,
        ListQuery, LookupQuery, OrderDir, Policies, PolicyRef, Predicate, QueryCache, SqlQuery,
        Tenancy, TypeRef,
    };

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn list_query_emits_args_filters_order_paginate_and_cache() {
        let mut feature = base_feature("customer");
        feature.defaults.policy = Some(PolicyRef::Local("read".to_owned()));
        feature.resources.push(resource(
            "Customer",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: vec![
                slot(
                    "lifecycle_stage",
                    TypeRef::Builtin(BuiltinType::Text),
                    false,
                ),
                slot("search", TypeRef::Builtin(BuiltinType::Text), false),
            ],
            scope: Vec::new(),
            scope_override: false,
            filters: vec![Filter {
                predicate: Predicate::Comparison {
                    left: Expr::Path(lazuli_ir::Path::from_segments(["lifecycle_stage"])),
                    op: CompareOp::Eq,
                    right: Expr::Path(lazuli_ir::Path::from_segments([
                        "params",
                        "lifecycle_stage",
                    ])),
                },
                when: Some("lifecycle_stage".to_owned()),
            }],
            order: vec![lazuli_ir::OrderBy {
                field: "created_at".to_owned(),
                direction: OrderDir::Desc,
            }],
            paginate: Some(50),
            modifier: None,
            cache: Some(QueryCache {
                key: "customer.list(params)".to_owned(),
                ttl: CacheTtl::Literal(CacheTtlLiteral::Minutes(5)),
                tags: vec!["customer-list".to_owned(), "customer-summary".to_owned()],
                namespace: Some("customer".to_owned()),
                profile_ref: None,
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"time\""));
        assert!(out.contains("type ListCustomersArgs struct {"));
        assert!(out.contains("LifecycleStage *string `json:\"lifecycle_stage,omitempty\"`"));
        assert!(out.contains("Search         *string `json:\"search,omitempty\"`"));
        assert!(out.contains("var listCustomers = lazuli.Query[ListCustomersArgs, Customer]{"));
        assert!(out.contains("Policy:   lazuli.Policy{Name: \"@policy.read\"},"));
        assert!(out.contains(
            "{Column: \"lifecycle_stage\", When: lazuli.FromInput(\"LifecycleStage\")},"
        ));
        assert!(out.contains("{Column: \"created_at\", Desc: true},"));
        assert!(out.contains("Paginate: 50,"));
        assert!(out.contains("TTL: 5 * time.Minute,"));
        assert!(out.contains("// TODO(runtime): QueryCache.tags not yet in Lazuli Go lib"));
        assert!(out.contains("// TODO(runtime): QueryCache.namespace not yet in Lazuli Go lib"));
    }

    #[test]
    fn list_query_normalizes_semantic_fk_id_filter_to_declared_fk_column() {
        let mut feature = base_feature("account");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource("Org", Vec::new()));
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "UserSession",
            vec![
                field("org", TypeRef::UserDefined(qname("Org")), true),
                field("user", TypeRef::UserDefined(qname("User")), true),
                field("token_hash", TypeRef::Builtin(BuiltinType::Text), true),
            ],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "mine_sessions".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: vec![
                Filter {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["org_id"])),
                        op: CompareOp::Eq,
                        right: Expr::Path(lazuli_ir::Path::from_segments([
                            "ctx", "actor", "org_id",
                        ])),
                    },
                    when: None,
                },
                Filter {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["user_id"])),
                        op: CompareOp::Eq,
                        right: Expr::Path(lazuli_ir::Path::from_segments([
                            "ctx", "actor", "user_id",
                        ])),
                    },
                    when: None,
                },
            ],
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("{Column: \"org_id\", When: lazuli.FromCtx(\"actor.org_id\")},"),
            "tenant org_id must stay on the implicit tenancy column:\n{out}"
        );
        assert!(
            out.contains("{Column: \"user\", When: lazuli.FromCtx(\"actor.user_id\")},"),
            "semantic user_id filter must target the declared FK column:\n{out}"
        );
        assert!(
            !out.contains("{Column: \"user_id\""),
            "user_id must not leak as a physical column for `user: User`:\n{out}"
        );
    }

    #[test]
    fn list_query_args_resolve_unresolved_cross_feature_ref() {
        let mut customer = base_feature("customer");
        customer.resources.push(resource("Customer", Vec::new()));
        customer.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: vec![slot("user", TypeRef::Unresolved("User".to_owned()), false)],
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
            owner_scope_sql: None,
        }));
        let mut account = base_feature("account");
        account.resources.push(resource("User", Vec::new()));
        let module = module_with_features(vec![customer, account]);

        let out = emit_from_module(&module, "customer").expect("must emit");
        // Resource refs collapse to `lazuli.ID` — query args are FK
        // ids on the wire, never the embedded resource row.
        assert!(out.contains("User *lazuli.ID `json:\"user,omitempty\"`"));
    }

    #[test]
    fn list_query_args_resolve_many_unresolved_cross_feature_ref() {
        let mut customer = base_feature("customer");
        customer.resources.push(resource("Customer", Vec::new()));
        customer.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: vec![slot(
                "reviewers",
                TypeRef::Many(Box::new(TypeRef::Unresolved("User".to_owned()))),
                true,
            )],
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
            owner_scope_sql: None,
        }));
        let mut account = base_feature("account");
        account.resources.push(resource("User", Vec::new()));
        let module = module_with_features(vec![customer, account]);

        let out = emit_from_module(&module, "customer").expect("must emit");
        // []Resource collapses to []lazuli.ID — see the singular case
        // above for the rationale.
        assert!(out.contains("Reviewers []lazuli.ID `json:\"reviewers\"`"));
    }

    #[test]
    fn lookup_query_synthesises_args_from_keys() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                true,
            )],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "by_email".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["email"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["email"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerByEmailArgs struct {"));
        assert!(out.contains("Email lazuli.Email `json:\"email\"`"));
        assert!(out.contains("var customerByEmail = lazuli.Query[CustomerByEmailArgs, Customer]{"));
        assert!(out.contains("Kind:     lazuli.QueryLookup,"));
        assert!(out.contains("{Column: \"email\", Source: lazuli.FromInput(\"Email\")},"));
    }

    #[test]
    fn lookup_query_normalizes_semantic_fk_id_key_to_declared_fk_column() {
        let mut feature = base_feature("traveler");
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "Traveler",
            vec![
                field("user", TypeRef::UserDefined(qname("User")), true),
                field("name", TypeRef::Builtin(BuiltinType::Text), true),
            ],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "my_traveler".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["user_id"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "user_id"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type MyTravelerArgs struct {"));
        assert!(
            out.contains("{Column: \"user\", Source: lazuli.FromCtx(\"actor.user_id\")},"),
            "semantic lookup user_id key must target the declared FK column:\n{out}"
        );
        assert!(
            !out.contains("{Column: \"user_id\""),
            "user_id must not leak as a physical lookup column for `user: User`:\n{out}"
        );
    }

    /// Gap A — every emitted `query.lookup` value carries an exported
    /// Go wrapper so Go-internal callers (other handlers, helpers,
    /// tests) can invoke the lookup without going through the HTTP
    /// router. Wrapper shape mirrors commands' `Handle<Name>` (here:
    /// PascalCase of the query name).
    #[test]
    fn lookup_query_emits_exported_go_wrapper() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                true,
            )],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "by_email".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["email"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["email"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        // The args struct still carries the route-shaped Email key; the
        // wrapper just delegates to `RunLookup` with the same args.
        assert!(
            out.contains(
                "func ByEmail(ctx *lazuli.Ctx, args CustomerByEmailArgs) (Customer, error) {"
            ),
            "exported lookup wrapper missing:\n{out}"
        );
        assert!(
            out.contains("return customerByEmail.RunLookup(ctx, args)"),
            "wrapper must delegate to RunLookup:\n{out}"
        );
    }

    /// Gap A — `lookup_my_*` queries authored by the `conventions [me]`
    /// synth carry no params and resolve every LookupBy from ctx. The
    /// wrapper drops the `args` parameter so callers write
    /// `LookupMyHost(ctx)`; the args literal is zero-constructed inline.
    #[test]
    fn actor_keyed_lookup_query_emits_args_less_go_wrapper() {
        let mut feature = base_feature("host");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource("Org", Vec::new()));
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "Host",
            vec![field("user", TypeRef::UserDefined(qname("User")), true)],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_my_host".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["org"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "org_id"])),
                },
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["user"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "user_id"])),
                },
            ],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("func LookupMyHost(ctx *lazuli.Ctx) (Host, error) {"),
            "actor-keyed wrapper signature must drop args:\n{out}"
        );
        assert!(
            out.contains("return lookupMyHost.RunLookup(ctx, LookupMyHostArgs{})"),
            "actor-keyed wrapper must zero-construct args inline:\n{out}"
        );
    }

    /// Gap A — `query.list` values also carry an exported wrapper so
    /// Go-internal callers can drive the list through the runtime.
    #[test]
    fn list_query_emits_exported_go_wrapper() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
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
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "func List(ctx *lazuli.Ctx, args ListCustomersArgs) ([]Customer, error) {"
            ),
            "exported list wrapper missing:\n{out}"
        );
        assert!(
            out.contains("return listCustomers.RunList(ctx, args)"),
            "wrapper must delegate to RunList:\n{out}"
        );
    }

    /// Gap B — when a `conventions [me]` synth emits a `LookupKey` with
    /// bare path `org` and the resource has `tenancy: Org` (no
    /// authored `org` field), the emitted Go column MUST be the
    /// implicit `org_id` tenancy column. Without this translation the
    /// runtime SELECT would target a non-existent `org` column and
    /// every `lookup_my_<r>` call would 500.
    #[test]
    fn lookup_my_query_translates_org_path_to_org_id_under_tenancy_org() {
        let mut feature = base_feature("host");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource("Org", Vec::new()));
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "Host",
            vec![field("user", TypeRef::UserDefined(qname("User")), true)],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_my_host".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["org"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "org_id"])),
                },
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["user"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "user_id"])),
                },
            ],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("{Column: \"org_id\", Source: lazuli.FromCtx(\"actor.org_id\")},"),
            "bare `org` path on a tenancy-Org resource must lower to `org_id`:\n{out}"
        );
        // The `user` field exists literally — no `_id` suffix expected.
        assert!(
            out.contains("{Column: \"user\", Source: lazuli.FromCtx(\"actor.user_id\")},"),
            "authored `user: User` FK keeps its literal column name:\n{out}"
        );
        assert!(
            !out.contains("{Column: \"org\","),
            "bare `org` must not leak as a physical column when tenancy adds org_id:\n{out}"
        );
    }

    /// Gap B inverse — when the resource has an authored `org: Org`
    /// FK field (no `tenancy: Org`), the DDL keeps the literal `org`
    /// column. The codegen translation MUST NOT add a spurious `_id`
    /// suffix in that case; the field-form wins.
    #[test]
    fn lookup_query_preserves_literal_fk_field_column_without_tenancy() {
        let mut feature = base_feature("memberships");
        feature.resources.push(resource("Org", Vec::new()));
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "Membership",
            vec![
                field("org", TypeRef::UserDefined(qname("Org")), true),
                field("user", TypeRef::UserDefined(qname("User")), true),
            ],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_my_membership".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["org"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "org_id"])),
                },
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["user"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "user_id"])),
                },
            ],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("{Column: \"org\", Source: lazuli.FromCtx(\"actor.org_id\")},"),
            "authored `org: Org` FK column keeps its literal name:\n{out}"
        );
        assert!(
            out.contains("{Column: \"user\", Source: lazuli.FromCtx(\"actor.user_id\")},"),
            "authored `user: User` FK column keeps its literal name:\n{out}"
        );
        assert!(
            !out.contains("{Column: \"org_id\","),
            "no spurious _id suffix when the FK field is literal:\n{out}"
        );
    }

    #[test]
    fn deterministic_across_runs_and_sorted_by_name() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource("Customer", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "zebra".to_owned(),
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
            owner_scope_sql: None,
        }));
        feature.queries.push(Query::List(ListQuery {
            name: "alpha".to_owned(),
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
            owner_scope_sql: None,
        }));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
        let alpha_pos = a.find("Query: customer.alpha").expect("alpha banner");
        let zebra_pos = a.find("Query: customer.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn gated_query_emits_real_prelude_field_and_billing_imports() {
        // PG.C.2 — gated queries lift the wave-4 comment annotation
        // into a real `Prelude: []billing.GateRef{...}` field that
        // `RunList` / `RunLookup` consult via `lazuli.RunPrelude`.
        let mut feature = base_feature("billing");
        feature.resources.push(resource("Invoice", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: Some(50),
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/query.list:list".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "list_invoices".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "queries_per_month".to_owned(),
                },
            ],
        );
        let module = module_with_features(vec![feature]);
        let cross_index = CrossFeatureIndex::build(&module);
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/query.gen.go")
                .with_gates(Some(&gates));
        let out = emit_query_file(
            "examples/billing.lzi",
            &module.features[0],
            "billing-app",
            &cross_index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"billing-app/plan\""),
            "plan import missing:\n{out}"
        );
        assert!(
            out.contains("Prelude: []billing.GateRef{"),
            "Prelude field missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateBehind, Name: \"list_invoices\"},"),
            "behind-gate row missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateQuota, Name: \"queries_per_month\"},"),
            "quota-gate row missing:\n{out}"
        );
    }

    #[test]
    fn ungated_query_emits_no_prelude_or_billing_import() {
        // PG.C.2 backward-compat — queries without gates emit
        // byte-equivalent wave-3 output.
        let mut feature = base_feature("customer");
        feature.resources.push(resource("Customer", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
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
            owner_scope_sql: None,
        }));
        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("Prelude:"),
            "no Prelude when no gates:\n{out}"
        );
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates:\n{out}"
        );
    }

    /// QUERY-POLICY-001 — `query.lookup` authoring `policy
    /// @policy.<name>` must emit a non-empty `lazuli.Policy{Name,
    /// Atoms}` literal that resolves through the feature's `Policies`
    /// catalog. The runtime rejects any registered command/query with
    /// an empty policy (`command/query registered with empty policy`),
    /// so a regression here panics every hostpoint API call.
    #[test]
    fn lookup_query_with_authored_policy_emits_resolved_atoms() {
        let mut feature = base_feature("traveler");
        feature.resources.push(resource(
            "Traveler",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "traveler_only".to_owned(),
                atoms: vec!["@actor.traveler".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "my_traveler".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["id"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["id"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::Local("traveler_only".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Policy:   lazuli.Policy{Name: \"@policy.traveler_only\", Atoms: []lazuli.PolicyAtom{{Namespace: \"actor\", Name: \"traveler\"}}},"
            ),
            "per-lookup-query policy should resolve to feature catalog atoms; got:\n{out}"
        );
        assert!(
            !out.contains("Policy:   lazuli.Policy{},"),
            "no empty-policy literal should leak through:\n{out}"
        );
    }

    #[test]
    fn query_with_policy_category_when_denied_emits_error_keys() {
        let mut feature = base_feature("account");
        feature.resources.push(resource(
            "Account",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "authenticated".to_owned(),
                atoms: vec!["@scope.authenticated".to_owned()],
                previous_names: Vec::new(),
                when_denied: Some(lazuli_ir::TranslationKeyRef {
                    key: "account_signin".to_owned(),
                    span_ref: None,
                }),
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        feature.queries.push(Query::List(ListQuery {
            name: "me".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::Local("authenticated".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/i18n\""),
            "i18n import missing:\n{out}"
        );
        assert!(
            out.contains("var meErrorKeys = lazuli.ErrorKeys{"),
            "query ErrorKeys var missing:\n{out}"
        );
        assert!(
            out.contains(
                "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"account_signin\"},"
            ),
            "policy-category when_denied should lower to query ErrorKeys:\n{out}"
        );
        assert!(
            out.contains("ErrorKeys: &meErrorKeys,"),
            "query should point runtime at ErrorKeys:\n{out}"
        );
    }

    /// QUERY-POLICY-001 — when the query authors NO `policy` and the
    /// feature has a `defaults.policy`, the feature default wins
    /// (back-compat with the pre-fix behavior).
    #[test]
    fn list_query_without_authored_policy_falls_back_to_feature_default() {
        let mut feature = base_feature("customer");
        feature.defaults.policy = Some(PolicyRef::Local("read".to_owned()));
        feature.resources.push(resource("Customer", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
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
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        // The pre-fix golden assertion at line 1182 still matches —
        // feature-level default kept its precedence when the query
        // is silent.
        assert!(
            out.contains("Policy:   lazuli.Policy{Name: \"@policy.read\"},"),
            "feature-default policy should still apply when query is silent; got:\n{out}"
        );
    }

}

#[cfg(test)]
mod feature_emit {
    use super::*;
    use super::test_support::{base_feature, emit, field, resource, slot};
    use lazuli_ir::{BuiltinType, Expr, KeyClause, ListQuery, LookupQuery, PolicyRef, TypeRef};

    #[test]
    fn entry_point_emits_representative_query_file_shape() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: vec![slot("search", TypeRef::Builtin(BuiltinType::Text), false)],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: Some(25),
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("query feature entry point must emit non-empty output");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("type ListCustomersArgs struct {"));
        assert!(out.contains("Search *string `json:\"search,omitempty\"`"));
        assert!(out.contains("var listCustomers = lazuli.Query[ListCustomersArgs, Customer]{"));
        assert!(out.contains("Paginate: 25,"));
    }

    // -------------------------------------------------------------------------
    // Owner-scope projection — cell `codegen-os-projection`. The analyzer
    // composes `Query::Lookup.owner_scope_sql` / `Query::List.owner_scope_sql`
    // per spec `ir-resource-conventions-owner-scope.md` §7.3; this codegen
    // cell appends the carrier to the runtime's existing FilterRule (LIST)
    // and LookupBy (LOOKUP) pipelines as a `FromCtxOwnedVia` entry so the
    // emitted SQL matches §8.3 / §8.4 verbatim.
    // -------------------------------------------------------------------------

    fn owner_scope_sql_property() -> lazuli_ir::OwnerScopeSql {
        lazuli_ir::OwnerScopeSql {
            field_name: "host".to_owned(),
            fk_target: "Host".to_owned(),
            through_column: "user".to_owned(),
            where_predicate: "host IN (SELECT id FROM \"host\" WHERE \"user\" = ctx.User.ID)"
                .to_owned(),
            cte_owner_check: None,
        }
    }

    #[test]
    fn lookup_query_with_owner_scope_sql_appends_owned_via_lookup_by() {
        // Spec §8.3: synth `lookup_property` lowers to
        //   SELECT ... FROM "property" WHERE id = $1 AND org_id = $2
        //     AND host IN (SELECT id FROM "host" WHERE "user" = $3)
        // Codegen projects the analyzer's owner_scope_sql carrier as a
        // `LookupBy` entry with `FromCtxOwnedVia`. The runtime's
        // `whereConditionFragment` (updated alongside this cell) lifts
        // the entry to the IN-subquery shape.
        let mut feature = base_feature("catalog");
        feature.resources.push(resource(
            "Property",
            vec![
                field("host", TypeRef::Unresolved("Host".to_owned()), true),
                field("name", TypeRef::Builtin(BuiltinType::Text), true),
            ],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_property".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["id"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["id"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: Some(owner_scope_sql_property()),
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("{Column: \"id\", Source: lazuli.FromInput(\"ID\")},"),
            "canonical id LookupBy entry must remain:\n{out}"
        );
        assert!(
            out.contains(
                "{Column: \"host\", Source: lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\")},"
            ),
            "owner_scope_sql must project to FromCtxOwnedVia in LookupBy:\n{out}"
        );
    }

    #[test]
    fn list_query_with_owner_scope_sql_appends_owned_via_filter() {
        // Spec §8.4: synth `list_propertys` lowers to
        //   SELECT ... FROM "property"
        //   WHERE org_id = $1 AND host IN (SELECT id FROM "host" WHERE "user" = $2)
        //   ORDER BY created_at DESC LIMIT $3 OFFSET $4
        // Codegen projects the analyzer's owner_scope_sql carrier as
        // a `FilterRule` entry with `FromCtxOwnedVia`. The author's
        // existing filters remain untouched.
        let mut feature = base_feature("catalog");
        feature.resources.push(resource(
            "Property",
            vec![
                field("host", TypeRef::Unresolved("Host".to_owned()), true),
                field("name", TypeRef::Builtin(BuiltinType::Text), true),
            ],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list_propertys".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: Some(50),
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: Some(owner_scope_sql_property()),
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("Filters: []lazuli.FilterRule{"),
            "Filters block must be emitted when owner_scope_sql is present even if author filters are empty:\n{out}"
        );
        assert!(
            out.contains(
                "{Column: \"host\", When: lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\")},"
            ),
            "owner_scope_sql must project to FromCtxOwnedVia in Filters:\n{out}"
        );
        assert!(
            out.contains("// owner-scope: synth from @owner_axis"),
            "filter block should be annotated for traceability:\n{out}"
        );
    }

    #[test]
    fn lookup_query_without_owner_scope_sql_unchanged() {
        // Tenant-only resources (no `@owner_axis`) carry
        // `owner_scope_sql: None` and emit exactly today's shape.
        let mut feature = base_feature("billing");
        feature.resources.push(resource(
            "Charge",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_charge".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["id"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["id"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("FromCtxOwnedVia"),
            "tenant-only LOOKUP must NOT emit owned-via:\n{out}"
        );
    }

    #[test]
    fn list_query_owner_scope_snake_cases_pascal_fk_target() {
        // PascalCase `OwnerScopeSql.fk_target` (e.g. `BookingProposal`)
        // must be snake-cased when projected to FromCtxOwnedVia so the
        // runtime's `quoteIdent` round-trips with the migrated SQL
        // table identifier.
        let mut feature = base_feature("operations");
        feature.resources.push(resource(
            "Transaction",
            vec![field(
                "proposal",
                TypeRef::Unresolved("BookingProposal".to_owned()),
                true,
            )],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list_transactions".to_owned(),
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
            owner_scope_sql: Some(lazuli_ir::OwnerScopeSql {
                field_name: "proposal".to_owned(),
                fk_target: "BookingProposal".to_owned(),
                through_column: "user".to_owned(),
                where_predicate:
                    "proposal IN (SELECT id FROM \"booking_proposal\" WHERE \"user\" = ctx.User.ID)"
                        .to_owned(),
                cte_owner_check: None,
            }),
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "{Column: \"proposal\", When: lazuli.FromCtxOwnedVia(\"booking_proposal\", \"user\", \"user.id\")},"
            ),
            "PascalCase fk_target must be snake-cased in FromCtxOwnedVia:\n{out}"
        );
    }
}
