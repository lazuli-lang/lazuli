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

}
