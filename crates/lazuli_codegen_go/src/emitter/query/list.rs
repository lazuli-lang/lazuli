//! Cell E4 — `query.list` emission. Walks a `ListQuery` and emits the
//! typed `<List><Resource>Args` struct, the `lazuli.Query[Args, Resource]`
//! value (header, filters, order, paginate, cache), and the exported Go
//! wrapper (see `list_wrapper.rs`).
//!
//! Naming axis: default `list` queries → `List<Resources>Args` /
//! `list<Resources>` (plural Rails shape); named queries
//! (`query.list active_users`) → `<Name>Args` / `<lowerCamelName>`.

use lazuli_ir::{Feature, ListQuery};

use super::super::error_resolver::emit_operation_error_keys;
use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_QUERY_PGX_LIST, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::TypeCtx;
use super::args::{emit_args_struct, emit_cache, query_error_keys_var};
use super::filters::{emit_filters, emit_order};
use super::header::{
    emit_gate_annotations, emit_query_header, emit_scope_gaps, query_policy_denied_key_for_parts,
};
use super::list_wrapper::emit_list_query_wrapper;
use super::util::{
    lower_camel, pascal_case, plural_pascal, resource_for_query, write_section_banner,
};

/// Emit a single `query.list` (args struct + Query value + Go wrapper).
pub(super) fn emit_list_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &ListQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let resource = resource_for_query(feature, &query.name);
    let resource_type = resource
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| "struct{}".to_owned());
    let resource_name_axis = resource
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| "Result".to_owned());
    // Wire registry key: `<feature>.<query_name>`. The `/q/` HTTP prefix
    // already disambiguates kind, so the historical `.query.` infix was
    // dropped (cell B1, codegen-correctness-cycle-2026-05-21).
    let qualified_name = format!("{}.{}", feature.name, query.name);
    let args_struct = list_args_struct_name(&query.name, &resource_name_axis);
    let var_name = list_var_name(&query.name, &resource_name_axis);
    let error_keys_var = query_error_keys_var(&var_name);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.list {}", query.name),
        ],
    );

    emit_args_struct(p, &args_struct, &query.params, ctx);
    p.blank();

    if let Some(key_ref) = query_policy_denied_key_for_parts(
        feature,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
    ) {
        emit_operation_error_keys(p, &error_keys_var, &qualified_name, &feature.name, key_ref);
        p.blank();
    }

    emit_pattern_header(p, PATTERN_QUERY_PGX_LIST);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, query.span_ref);
    p.line(&format!(
        "var {var_name} = lazuli.Query[{args_struct}, {resource_type}]{{"
    ));
    p.indent();
    emit_query_header(
        p,
        feature,
        &qualified_name,
        resource,
        "lazuli.QueryList",
        emit_ctx,
        &query.name,
        query.span_ref,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
        &error_keys_var,
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.list", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    if query.modifier.is_some() {
        p.line("// TODO(runtime): ListQuery.modifier is not yet in Lazuli Go lib.");
    }
    emit_filters(
        p,
        feature,
        resource,
        &query.filters,
        query.owner_scope_sql.as_ref(),
    );
    emit_order(p, feature, resource, &query.order);
    if let Some(page_size) = query.paginate {
        p.line(&format!("Paginate: {page_size},"));
    }
    if let Some(cache) = &query.cache {
        emit_cache(p, cache);
    }
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
    p.blank();
    emit_list_query_wrapper(p, &query.name, &var_name, &args_struct, &resource_type);
}

fn list_args_struct_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name == "list" {
        format!("List{}Args", plural_pascal(resource_pascal))
    } else {
        format!("{}Args", pascal_case(query_name))
    }
}

/// Package-private var name for a `query.list`. `list` → `list<Resources>`
/// (plural lower-camel); otherwise the query name lower-cameled.
pub(in crate::emitter) fn list_var_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name == "list" {
        format!("list{}", plural_pascal(resource_pascal))
    } else {
        lower_camel(query_name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{base_feature, emit, field, qname, resource, slot};
    use lazuli_ir::{
        BuiltinType, CacheTtl, CacheTtlLiteral, CompareOp, Expr, Filter, ListQuery, OrderDir,
        Policies, PolicyRef, Predicate, Query, QueryCache, Tenancy, TypeRef,
    };

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
        // SECURITY (POLICY-REF-UNRESOLVED): `@policy.read` is not declared in
        // this feature's `policies` block, so it is unresolvable and the query
        // fails CLOSED with a deny atom (not a Name-only empty-atoms bypass).
        assert!(out.contains(
            "Policy:   lazuli.Policy{Name: \"@policy.read\", Atoms: []lazuli.PolicyAtom{{Namespace: \"predicate\", Name: \"deny\"}}},"
        ));
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
                conditional_atoms: Vec::new(),
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
        // Feature-level default kept its precedence when the query
        // is silent.
        // The feature-default policy still applies (precedence preserved); it
        // resolves to no declared category here, so it fails CLOSED with a deny
        // atom rather than the former empty-atoms bypass.
        assert!(
            out.contains("Policy:   lazuli.Policy{Name: \"@policy.read\", Atoms: []lazuli.PolicyAtom{{Namespace: \"predicate\", Name: \"deny\"}}},"),
            "feature-default policy should still apply (fail-closed) when query is silent; got:\n{out}"
        );
    }

    // Owner-scope projection (cell `codegen-os-projection`) — analyzer
    // composes `Query::List.owner_scope_sql` per
    // `ir-resource-conventions-owner-scope.md` §7.3; codegen appends the
    // carrier as a `FromCtxOwnedVia` FilterRule (matches §8.4 verbatim).
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
