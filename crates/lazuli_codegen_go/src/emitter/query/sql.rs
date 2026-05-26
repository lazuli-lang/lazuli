//! Cell E4 — `query.sql` and `query.view` emission. The IR carries
//! both kinds in `SqlQuery` with a `sql_kind` discriminator because
//! they share most of the surface (SQL path + params struct + return
//! type) and only diverge on `SQLMany` / `SQLArgs` (View-only).
//!
//! `query.sql` lowers to `lazuli.QuerySQL` with an embedded
//! `SQL: "<path>"` reference (the runtime loads the SQL body from
//! disk at boot). `query.view` lowers to `lazuli.QueryView` and adds
//! `SQLMany: <bool>` + a `SQLArgs:` closure that projects the typed
//! args struct to `[]any` for the runtime row scanner.

use lazuli_ir::{Feature, SqlQuery, TypeRef, TypedSlot};

use super::super::error_resolver::emit_operation_error_keys;
use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_QUERY_PGX_SQL, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};
use super::args::{emit_args_struct, emit_cache, query_error_keys_var};
use super::header::{
    emit_gate_annotations, emit_query_header, emit_scope_gaps, query_policy_denied_key_for_parts,
};
use super::util::{escape_string, lower_camel, pascal_case, return_name, write_section_banner};

/// Emit a single `query.sql` or `query.view` value (typed args struct
/// + the `lazuli.Query[A, R]` value + the kind-specific extras).
pub(super) fn emit_sql_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &SqlQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    // Wire registry key: `<feature>.<query_name>`. See list.rs for rationale.
    let qualified_name = format!("{}.{}", feature.name, query.name);
    let args_struct = format!("{}Args", pascal_case(&query.name));
    let var_name = lower_camel(&query.name);
    let error_keys_var = query_error_keys_var(&var_name);
    let return_ref = sql_query_row_type(query);
    let (return_type, _import) = types::go_type_for(return_ref, ctx);
    let returns_name = return_name(&query.returns, ctx);
    let query_kind = sql_query_callable_kind(query);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  {query_kind} {}", query.name),
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

    emit_pattern_header(p, PATTERN_QUERY_PGX_SQL);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, query.span_ref);
    p.line(&format!(
        "var {var_name} = lazuli.Query[{args_struct}, {return_type}]{{"
    ));
    p.indent();
    emit_query_header(
        p,
        feature,
        &qualified_name,
        None,
        if query.sql_kind == lazuli_ir::SqlQueryKind::View {
            "lazuli.QueryView"
        } else {
            "lazuli.QuerySQL"
        },
        emit_ctx,
        &query.name,
        query.span_ref,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
        &error_keys_var,
    );
    emit_gate_annotations(p, emit_ctx.gates_for(query_kind, &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    p.line(&format!("SQL:     \"{}\",", escape_string(&query.sql_path)));
    p.line(&format!("Returns: \"{}\",", escape_string(&returns_name)));
    if query.sql_kind == lazuli_ir::SqlQueryKind::View {
        p.line(&format!("SQLMany: {},", sql_query_returns_many(query)));
        emit_sql_args_fn(p, &args_struct, &query.params);
    }
    if let Some(cache) = &query.cache {
        emit_cache(p, cache);
    }
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

/// Canonical `query.sql` / `query.view` kind label used in the
/// section banner, gate map key, and runtime error envelopes.
pub(super) fn sql_query_callable_kind(query: &SqlQuery) -> &'static str {
    match query.sql_kind {
        lazuli_ir::SqlQueryKind::Sql => "query.sql",
        lazuli_ir::SqlQueryKind::View => "query.view",
    }
}

/// Does this view return a many-row result? Determines the
/// `SQLMany: <bool>` field on `lazuli.QueryView` values so the
/// runtime knows whether to scan one row or all rows.
pub(super) fn sql_query_returns_many(query: &SqlQuery) -> bool {
    matches!(query.returns, TypeRef::Many(_))
}

/// Row type for a `SqlQuery`. For views, `Many(T)` collapses to `T`
/// because `SQLMany` already carries the multiplicity axis; for
/// non-view `query.sql` the returns ref is used verbatim.
pub(super) fn sql_query_row_type(query: &SqlQuery) -> &TypeRef {
    if query.sql_kind == lazuli_ir::SqlQueryKind::View {
        if let TypeRef::Many(inner) = &query.returns {
            return inner;
        }
    }
    &query.returns
}

/// Emit `SQLArgs: func(args FooArgs) []any { ... }` — the runtime
/// row scanner calls this closure to materialize the SQL argument
/// vector from the typed args struct.
fn emit_sql_args_fn(p: &mut GoPrinter, args_struct: &str, params: &[TypedSlot]) {
    p.line(&format!("SQLArgs: func(args {args_struct}) []any {{"));
    p.indent();
    if params.is_empty() {
        p.line("return nil");
    } else {
        p.line("return []any{");
        p.indent();
        for param in params {
            p.line(&format!("args.{},", pascal_case(&param.name)));
        }
        p.dedent();
        p.line("}");
    }
    p.dedent();
    p.line("},");
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{base_feature, emit, qname, record, resource, slot};
    use lazuli_ir::{
        BuiltinType, CacheTtl, Policies, PolicyRef, Query, QueryCache, SqlQuery, TypeRef,
    };

    #[test]
    fn sql_query_emits_sql_path_returns_and_quoted_cache_todo() {
        let mut feature = base_feature("customer");
        feature.records.push(record("CustomerLtv"));
        feature.queries.push(Query::Sql(SqlQuery {
            name: "lifetime_value".to_owned(),
            sql_kind: lazuli_ir::SqlQueryKind::Sql,
            public_contract: None,
            params: vec![slot(
                "min_score",
                TypeRef::Builtin(BuiltinType::Integer),
                false,
            )],
            scope: Vec::new(),
            scope_override: false,
            returns: TypeRef::Many(Box::new(TypeRef::UserDefined(qname("CustomerLtv")))),
            sql_path: "./queries/customer_lifetime_value.sql".to_owned(),
            cache: Some(QueryCache {
                key: "customer.ltv(params)".to_owned(),
                ttl: CacheTtl::Quoted("5 minutes".to_owned()),
                tags: Vec::new(),
                namespace: None,
                profile_ref: None,
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type LifetimeValueArgs struct {"));
        assert!(out.contains("MinScore *int64 `json:\"min_score,omitempty\"`"));
        assert!(
            out.contains("var lifetimeValue = lazuli.Query[LifetimeValueArgs, []CustomerLtv]{")
        );
        assert!(out.contains("Kind:     lazuli.QuerySQL,"));
        assert!(out.contains("SQL:     \"./queries/customer_lifetime_value.sql\","));
        assert!(out.contains("Returns: \"CustomerLtv[]\","));
        assert!(out.contains("// TODO(runtime): quoted QueryCache.ttl"));
    }

    #[test]
    fn query_view_emits_typed_sql_runtime_binding() {
        let mut feature = base_feature("host");
        feature.records.push(record("HostHomeRow"));
        feature.queries.push(Query::Sql(SqlQuery {
            name: "host_home_view".to_owned(),
            sql_kind: lazuli_ir::SqlQueryKind::View,
            public_contract: None,
            params: vec![slot("user_id", TypeRef::Builtin(BuiltinType::Id), true)],
            scope: Vec::new(),
            scope_override: false,
            returns: TypeRef::Many(Box::new(TypeRef::UserDefined(qname("HostHomeRow")))),
            sql_path: "app/features/host/queries/host_home_view.sql".to_owned(),
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("//   query.view host_home_view"));
        assert!(out.contains("type HostHomeViewArgs struct {"));
        assert!(out.contains("UserID lazuli.ID `json:\"user_id\"`"));
        assert!(out.contains("var hostHomeView = lazuli.Query[HostHomeViewArgs, HostHomeRow]{"));
        assert!(out.contains("Kind:     lazuli.QueryView,"));
        assert!(out.contains("SQL:     \"app/features/host/queries/host_home_view.sql\","));
        assert!(out.contains("Returns: \"HostHomeRow[]\","));
        assert!(out.contains("SQLMany: true,"));
        assert!(out.contains("SQLArgs: func(args HostHomeViewArgs) []any {"));
        assert!(out.contains("args.UserID,"));
    }

    /// QUERY-POLICY-001 — when the query authors a `policy` AND the
    /// feature also has a `defaults.policy`, the per-query authoring
    /// wins (Command precedence parity).
    #[test]
    fn sql_query_authored_policy_overrides_feature_default() {
        let mut feature = base_feature("customer");
        feature.defaults.policy = Some(PolicyRef::Local("read".to_owned()));
        feature.policies = Policies {
            categories: vec![
                lazuli_ir::PolicyCategory {
                    name: "read".to_owned(),
                    atoms: vec!["@scope.same_org".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                    when_denied_route: None,
                },
                lazuli_ir::PolicyCategory {
                    name: "audit".to_owned(),
                    atoms: vec!["@role.admin".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                    when_denied_route: None,
                },
            ],
            fields: Vec::new(),
            span_ref: None,
        };
        feature.records.push(record("AuditRow"));
        feature.queries.push(Query::Sql(SqlQuery {
            name: "audit_dump".to_owned(),
            sql_kind: lazuli_ir::SqlQueryKind::Sql,
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            returns: TypeRef::Many(Box::new(TypeRef::UserDefined(qname("AuditRow")))),
            sql_path: "./queries/audit_dump.sql".to_owned(),
            cache: None,
            policy: PolicyRef::Local("audit".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Policy:   lazuli.Policy{Name: \"@policy.audit\", Atoms: []lazuli.PolicyAtom{{Namespace: \"role\", Name: \"admin\"}}},"
            ),
            "per-query authored `policy` should beat feature-default; got:\n{out}"
        );
        assert!(
            !out.contains("Name: \"@policy.read\""),
            "feature-default policy should NOT leak when query authored its own; got:\n{out}"
        );
    }
}
