//! Cell E4 — `query.lookup` emission. Walks a `LookupQuery` and emits
//! the typed args struct (params + lifted lookup keys), the
//! `lazuli.Query[Args, Resource]` value (with header + LookupBy +
//! filters), and the exported Go wrapper (see `lookup_wrapper.rs`).

use lazuli_ir::{Feature, LookupQuery};

use super::super::error_resolver::emit_operation_error_keys;
use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_QUERY_PGX_LOOKUP, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::TypeCtx;
use super::args::{emit_args_struct, query_error_keys_var};
use super::filters::emit_lookup_by_with_filters;
use super::header::{
    emit_gate_annotations, emit_query_header, emit_scope_gaps, query_policy_denied_key_for_parts,
};
use super::lookup_args::lookup_args;
use super::lookup_wrapper::{emit_lookup_query_wrapper, is_actor_keyed_lookup};
use super::util::{lower_camel, pascal_case, resource_for_query, write_section_banner};

/// Emit a single `query.lookup` (args struct + Query value + Go wrapper).
pub(super) fn emit_lookup_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &LookupQuery,
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
    // Wire registry key: `<feature>.<query_name>`. See list.rs for rationale.
    let qualified_name = format!("{}.{}", feature.name, query.name);
    let args_struct = lookup_args_struct_name(&query.name, &resource_name_axis);
    let var_name = lookup_var_name(&query.name, &resource_name_axis);
    let error_keys_var = query_error_keys_var(&var_name);
    let args = lookup_args(query, resource);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.lookup {}", query.name),
        ],
    );

    emit_args_struct(p, &args_struct, &args, ctx);
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

    emit_pattern_header(p, PATTERN_QUERY_PGX_LOOKUP);
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
        "lazuli.QueryLookup",
        emit_ctx,
        &query.name,
        query.span_ref,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
        &error_keys_var,
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.lookup", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    emit_lookup_by_with_filters(
        p,
        feature,
        resource,
        &query.keys,
        &query.filters,
        query.owner_scope_sql.as_ref(),
    );
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
    p.blank();
    let actor_keyed = is_actor_keyed_lookup(query);
    emit_lookup_query_wrapper(
        p,
        &query.name,
        &var_name,
        &args_struct,
        &resource_type,
        actor_keyed,
    );
}

fn lookup_args_struct_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name.starts_with("by_") || query_name == "by" {
        format!("{}{}Args", resource_pascal, pascal_case(query_name))
    } else {
        format!("{}Args", pascal_case(query_name))
    }
}

/// Package-private var name for a `query.lookup`. The naming axis
/// mirrors `list_var_name` — `by_*` queries get a resource prefix
/// (`hostBySlug`) because the verb already encodes "look up by";
/// every other query keeps its bare lower-camel name.
pub(in crate::emitter) fn lookup_var_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name.starts_with("by_") || query_name == "by" {
        format!(
            "{}{}",
            lower_camel(resource_pascal),
            pascal_case(query_name)
        )
    } else {
        lower_camel(query_name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{base_feature, emit, field, qname, resource};
    use lazuli_ir::{
        BuiltinType, Expr, KeyClause, LookupQuery, Policies, PolicyRef, Query, Tenancy, TypeRef,
    };

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

    /// QUERY-POLICY-001 — `query.lookup` authoring `policy
    /// @policy.<name>` must emit a non-empty `lazuli.Policy{Name,
    /// Atoms}` literal that resolves through the feature's `Policies`
    /// catalog.
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
    fn lookup_query_with_owner_scope_sql_appends_owned_via_lookup_by() {
        // Spec §8.3: synth `lookup_property` lowers to
        //   SELECT ... FROM "property" WHERE id = $1 AND org_id = $2
        //     AND host IN (SELECT id FROM "host" WHERE "user" = $3)
        // Codegen projects the analyzer's owner_scope_sql carrier as a
        // `LookupBy` entry with `FromCtxOwnedVia`.
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
            owner_scope_sql: Some(lazuli_ir::OwnerScopeSql {
                field_name: "host".to_owned(),
                fk_target: "Host".to_owned(),
                through_column: "user".to_owned(),
                where_predicate: "host IN (SELECT id FROM \"host\" WHERE \"user\" = ctx.User.ID)"
                    .to_owned(),
                cte_owner_check: None,
            }),
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
