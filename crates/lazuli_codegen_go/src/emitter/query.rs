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

use lazuli_ir::{
    BuiltinType, CacheTtl, CacheTtlLiteral, CompareOp, Expr, Feature, Filter, Gate, KeyClause,
    ListQuery, LookupQuery, OrderDir, PolicyRef, Predicate, Query, QueryCache, Resource, SqlQuery,
    Tenancy, TypeRef, TypedSlot,
};

use super::cross_feature::CrossFeatureIndex;
use super::error_resolver::{emit_operation_error_keys, policy_denied_key_for_policy};
use super::imports::ImportSet;
use super::module::EmitContext;
use super::patterns::{
    PATTERN_QUERY_PGX_LIST, PATTERN_QUERY_PGX_LOOKUP, PATTERN_QUERY_PGX_SQL, emit_pattern_header,
};
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

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

fn emit_list_query(
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

fn emit_lookup_query(
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
    // Wire registry key: `<feature>.<query_name>`. See list_query for rationale.
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

/// Emit an exported Go wrapper that lets Go-internal callers invoke a
/// `query.lookup` value directly instead of going through the HTTP
/// router. Mirrors `command.rs`'s `Handle<Name>` wrapper convention —
/// the wrapper is byte-equivalent to manually writing
/// `lookupMyHost.RunLookup(ctx, LookupMyHostArgs{...})`, just discoverable
/// at the package level.
///
/// Two shapes:
/// - `lookup_my_*` queries with no `params` (singleton-per-actor)
///   compile to `func <Pascal>(ctx *lazuli.Ctx) (R, error)`. The args
///   value is constructed inline as the zero literal; runtime resolves
///   every `LookupBy` source from ctx.
/// - Every other lookup query keeps the `(ctx, args)` signature so
///   route params / typed inputs reach `RunLookup` verbatim.
fn emit_lookup_query_wrapper(
    p: &mut GoPrinter,
    query_name: &str,
    var_name: &str,
    args_struct: &str,
    resource_type: &str,
    actor_keyed: bool,
) {
    let func_name = query_wrapper_func_name(query_name);
    emit_pattern_header(p, PATTERN_QUERY_PGX_LOOKUP);
    if actor_keyed {
        p.line(&format!(
            "// {func_name} is the exported Go wrapper around the package-private"
        ));
        p.line(&format!(
            "// `{var_name}` value. Callers invoke {func_name}(ctx) without"
        ));
        p.line("// passing an args struct — the actor identity drives the LookupBy");
        p.line("// keys via ctx-sourced bindings.");
        p.line(&format!(
            "func {func_name}(ctx *lazuli.Ctx) ({resource_type}, error) {{"
        ));
        p.indent();
        p.line(&format!(
            "return {var_name}.RunLookup(ctx, {args_struct}{{}})"
        ));
        p.dedent();
        p.line("}");
    } else {
        p.line(&format!(
            "// {func_name} is the exported Go wrapper around the package-private"
        ));
        p.line(&format!(
            "// `{var_name}` value. Mirrors the command-side Handle<Name> shape"
        ));
        p.line("// so Go-internal callers (other handlers, helpers, tests) can");
        p.line("// invoke the lookup without going through the HTTP router.");
        p.line(&format!(
            "func {func_name}(ctx *lazuli.Ctx, args {args_struct}) ({resource_type}, error) {{"
        ));
        p.indent();
        p.line(&format!("return {var_name}.RunLookup(ctx, args)"));
        p.dedent();
        p.line("}");
    }
}

/// Emit an exported Go wrapper for a `query.list` value. Mirrors the
/// command-side `Handle<Name>` convention so Go-internal callers can
/// invoke the list without going through the HTTP router.
fn emit_list_query_wrapper(
    p: &mut GoPrinter,
    query_name: &str,
    var_name: &str,
    args_struct: &str,
    resource_type: &str,
) {
    let func_name = query_wrapper_func_name(query_name);
    emit_pattern_header(p, PATTERN_QUERY_PGX_LIST);
    p.line(&format!(
        "// {func_name} is the exported Go wrapper around the package-private"
    ));
    p.line(&format!(
        "// `{var_name}` value. Mirrors the command-side Handle<Name> shape"
    ));
    p.line("// so Go-internal callers (other handlers, helpers, tests) can");
    p.line("// invoke the list without going through the HTTP router.");
    p.line(&format!(
        "func {func_name}(ctx *lazuli.Ctx, args {args_struct}) ([]{resource_type}, error) {{"
    ));
    p.indent();
    p.line(&format!("return {var_name}.RunList(ctx, args)"));
    p.dedent();
    p.line("}");
}

/// PascalCase of the query name. Mirrors the command-side
/// `command_handler_func_name` convention but uses the bare query name
/// (no `Handle` prefix) — the spec's `lookup_my_host` -> `LookupMyHost`
/// reads naturally because the query name already encodes the verb.
fn query_wrapper_func_name(query_name: &str) -> String {
    pascal_case(query_name)
}

/// A `query.lookup` is actor-keyed when every `LookupBy` source resolves
/// from ctx (no route params or typed inputs needed). This is the shape
/// the `conventions [me]` synth produces and the only case where the
/// emitted wrapper can drop the `args` parameter — for every other
/// lookup, the caller MUST pass an args struct so route / input keys
/// reach the runtime.
fn is_actor_keyed_lookup(query: &LookupQuery) -> bool {
    if !query.params.is_empty() {
        return false;
    }
    if query.keys.is_empty() {
        return false;
    }
    query.keys.iter().all(|key| match &key.equals {
        Expr::Path(path) => path
            .segments
            .first()
            .map(|s| s.as_str() == "ctx")
            .unwrap_or(false),
        _ => false,
    })
}

fn emit_sql_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &SqlQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    // Wire registry key: `<feature>.<query_name>`. See list_query for rationale.
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

fn emit_query_header(
    p: &mut GoPrinter,
    feature: &Feature,
    qualified_name: &str,
    resource: Option<&Resource>,
    kind_const: &str,
    emit_ctx: &EmitContext<'_>,
    op: &str,
    span: Option<lazuli_ir::SpanRef>,
    query_policy: &PolicyRef,
    query_policy_expr: Option<&lazuli_ir::PolicyExpr>,
    query_policy_when_denied: Option<&lazuli_ir::TranslationKeyRef>,
    error_keys_var: &str,
) {
    let mut kv_rows: Vec<(String, String)> = Vec::new();
    kv_rows.push(("Name:".to_owned(), format!("\"{qualified_name}\",")));
    if let Some(resource) = resource {
        kv_rows.push((
            "Resource:".to_owned(),
            format!("&{}Resource,", lower_camel(&resource.name)),
        ));
    } else {
        kv_rows.push(("Resource:".to_owned(), "nil,".to_owned()));
    }
    kv_rows.push(("Kind:".to_owned(), format!("{kind_const},")));
    // QUERY-POLICY-001 — precedence mirrors `Command`: per-query
    // authored `policy @policy.<X>` (carried on the IR field) wins;
    // `PolicyRef::None` (the default) means "no override authored on
    // this query" and falls back to the feature-level default. Without
    // this branch, every authored `policy @policy.X` line on a query
    // was lost between parse and emit, and the runtime refused to run
    // the query with a `command/query registered with empty policy`
    // panic.
    let policy = if !query_policy.is_none() || query_policy_expr.is_some() {
        super::command::format_policy_with_expr_public(
            query_policy,
            query_policy_expr,
            Some(&feature.policies),
        )
    } else {
        match &feature.defaults.policy {
            Some(policy) => super::command::format_policy_with_expr_public(
                policy,
                None,
                Some(&feature.policies),
            ),
            None => super::command::format_policy_with_expr_public(
                &PolicyRef::None,
                None,
                Some(&feature.policies),
            ),
        }
    };
    kv_rows.push(("Policy:".to_owned(), policy));
    if query_policy_denied_key_for_parts(
        feature,
        query_policy,
        query_policy_expr,
        query_policy_when_denied,
    )
    .is_some()
    {
        kv_rows.push(("ErrorKeys:".to_owned(), format!("&{error_keys_var},")));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "query", op, span);
}

fn query_policy_denied_key<'a>(
    feature: &'a Feature,
    query: &'a Query,
) -> Option<&'a lazuli_ir::TranslationKeyRef> {
    match query {
        Query::List(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
        Query::Lookup(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
        Query::Sql(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
    }
}

fn query_policy_denied_key_for_parts<'a>(
    feature: &'a Feature,
    query_policy: &'a PolicyRef,
    query_policy_expr: Option<&'a lazuli_ir::PolicyExpr>,
    query_policy_when_denied: Option<&'a lazuli_ir::TranslationKeyRef>,
) -> Option<&'a lazuli_ir::TranslationKeyRef> {
    let effective_policy = effective_query_policy(feature, query_policy, query_policy_expr);
    policy_denied_key_for_policy(
        query_policy_when_denied,
        effective_policy,
        Some(&feature.policies),
    )
}

fn effective_query_policy<'a>(
    feature: &'a Feature,
    query_policy: &'a PolicyRef,
    query_policy_expr: Option<&'a lazuli_ir::PolicyExpr>,
) -> &'a PolicyRef {
    if !query_policy.is_none() || query_policy_expr.is_some() {
        return query_policy;
    }
    feature.defaults.policy.as_ref().unwrap_or(query_policy)
}

fn query_error_keys_var(var_name: &str) -> String {
    format!("{var_name}ErrorKeys")
}

fn emit_args_struct(p: &mut GoPrinter, name: &str, slots: &[TypedSlot], ctx: &TypeCtx<'_>) {
    p.line(&format!("type {name} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> = Vec::with_capacity(slots.len());
    for slot in slots {
        let type_ref = query_arg_type_ref(&slot.type_ref, ctx);
        let (go_type, _import) = types::go_type_for(&type_ref, ctx);
        let optional = !slot.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", slot.name)
        } else {
            slot.name.clone()
        };
        let tag = format!("`json:\"{}\"`", json_suffix);
        rows.push((pascal_case(&slot.name), final_type, tag));
    }
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(n, t, g)| (n.as_str(), t.as_str(), g.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

fn query_arg_type_ref(type_ref: &TypeRef, ctx: &TypeCtx<'_>) -> TypeRef {
    match type_ref {
        TypeRef::Unresolved(name) => match ctx.cross_index.owner(name) {
            Some(owner) => TypeRef::UserDefined(lazuli_ir::QualifiedName {
                feature: Some(owner.to_owned()),
                name: name.clone(),
            }),
            None => type_ref.clone(),
        },
        TypeRef::Many(inner) => TypeRef::Many(Box::new(query_arg_type_ref(inner, ctx))),
        _ => type_ref.clone(),
    }
}

fn emit_scope_gaps(p: &mut GoPrinter, scope: &[Predicate], scope_override: bool) {
    if !scope.is_empty() {
        p.line("// TODO(runtime): Query.scope is not yet in Lazuli Go lib.");
    }
    if scope_override {
        p.line("// TODO(runtime): Query.scope_override is not yet in Lazuli Go lib.");
    }
}

fn emit_filters(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    filters: &[Filter],
    owner_scope: Option<&lazuli_ir::OwnerScopeSql>,
) {
    let owner_scope_entry = owner_scope.map(owner_scope_filter_entry);
    if filters.is_empty() && owner_scope_entry.is_none() {
        return;
    }
    p.line("Filters: []lazuli.FilterRule{");
    p.indent();
    for filter in filters {
        match filter_rule(filter, feature, resource) {
            Ok((column, source)) => p.line(&format!(
                "{{Column: \"{}\", When: {source}}},",
                escape_string(&column)
            )),
            Err(message) => p.line(&format!("// TODO(ir): {message}")),
        }
    }
    // `ir-resource-conventions-owner-scope.md` §8.4 — synth-side
    // owner-scope predicate on `list_<resource>`. The analyzer
    // composed the chain in `query.owner_scope_sql` (cell O2); codegen
    // projects it through the existing `FromCtxOwnedVia` shape that the
    // author-side `mine_*` filters already use. The runtime expands
    // both to the same `<fk> IN (SELECT id FROM <fk_table>
    // WHERE <through> = $N)` form via `whereConditionFragment`, so the
    // emitted SQL matches spec §8.4 verbatim after the existing tenant
    // predicates.
    if let Some((column, source)) = owner_scope_entry {
        p.line(&format!(
            "// owner-scope: synth from @owner_axis (cell O2 + codegen-os projection)",
        ));
        p.line(&format!(
            "{{Column: \"{}\", When: {source}}},",
            escape_string(&column)
        ));
    }
    p.dedent();
    p.line("},");
}

/// Compose a `(column, FromCtxOwnedVia(...))` pair from the
/// analyzer's `OwnerScopeSql` carrier. The column is the FK column on
/// the resource (e.g. `host`); the source resolves to `ctx.User.ID`
/// at runtime and the SQL fragment expands to the IN-subquery shape.
///
/// The PascalCase `fk_target` (e.g. `Host`) is snake-cased here so the
/// emitted `relatedTable` matches the migrated schema's table name
/// (`host`), consistent with how `quoteResourceTable` lowers names on
/// the runtime side.
fn owner_scope_filter_entry(scope: &lazuli_ir::OwnerScopeSql) -> (String, String) {
    let fk_table = pascal_to_snake(&scope.fk_target);
    let source = format!(
        "lazuli.FromCtxOwnedVia(\"{}\", \"{}\", \"user.id\")",
        escape_string(&fk_table),
        escape_string(&scope.through_column),
    );
    (scope.field_name.clone(), source)
}

fn emit_order(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    order: &[lazuli_ir::OrderBy],
) {
    if order.is_empty() {
        return;
    }
    p.line("Order: []lazuli.OrderClause{");
    p.indent();
    for clause in order {
        let desc = matches!(clause.direction, OrderDir::Desc);
        let column = normalize_resource_column(
            feature,
            resource,
            &column_from_segments(&[clause.field.clone()]),
        );
        p.line(&format!(
            "{{Column: \"{}\", Desc: {desc}}},",
            escape_string(&column)
        ));
    }
    p.dedent();
    p.line("},");
}

/// Emits `LookupBy: [...]` merging the canonical `by ... = ...` keys
/// with each `filters` entry (which the LookupQuery shape parses but
/// the runtime can only consume through LookupBy). Filters are
/// rendered the same way `query.list` does — including the owned-via
/// FK lift — so a `query.lookup` filtered by `<FK>_id = ctx.actor.user_id`
/// stays consistent with its list-sibling.
fn emit_lookup_by_with_filters(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    keys: &[KeyClause],
    filters: &[Filter],
    owner_scope: Option<&lazuli_ir::OwnerScopeSql>,
) {
    let mut entries: Vec<(String, String)> =
        Vec::with_capacity(keys.len() + filters.len() + usize::from(owner_scope.is_some()));
    for key in keys {
        let column =
            normalize_resource_column(feature, resource, &column_from_segments(&key.path.segments));
        entries.push((column, format_source_expr(&key.equals)));
    }
    for filter in filters {
        match filter_rule(filter, feature, resource) {
            Ok(pair) => entries.push(pair),
            Err(message) => p.line(&format!("// TODO(ir): {message}")),
        }
    }
    // `ir-resource-conventions-owner-scope.md` §8.3 — synth-side
    // owner-scope predicate on `lookup_<resource>`. Mirrors §8.4 on
    // List: the analyzer composed the chain; we project to
    // `FromCtxOwnedVia` so the runtime's `whereConditionFragment` (cf.
    // the runtime update accompanying this cell) lifts the LookupBy
    // entry to `<fk> IN (SELECT id FROM <fk_table> WHERE <through> = $N)`
    // after the existing tenant predicates.
    if let Some(scope) = owner_scope {
        let (column, source) = owner_scope_filter_entry(scope);
        // Dedupe: if a hand-authored key already binds this column the
        // analyzer's scope is redundant (extremely rare; the synth's
        // canonical lookup keys are `id`).
        if !entries.iter().any(|(col, _)| col == &column) {
            entries.push((column, source));
        }
    }
    if entries.is_empty() {
        return;
    }
    p.line("LookupBy: []lazuli.LookupKey{");
    p.indent();
    for (column, source) in &entries {
        p.line(&format!(
            "{{Column: \"{}\", Source: {source}}},",
            escape_string(column)
        ));
    }
    p.dedent();
    p.line("},");
}

fn emit_cache(p: &mut GoPrinter, cache: &QueryCache) {
    p.line("Cache: &lazuli.CacheSpec{");
    p.indent();
    p.line(&format!("Key: \"{}\",", escape_string(&cache.key)));
    match &cache.ttl {
        CacheTtl::Literal(ttl) => p.line(&format!("TTL: {},", format_cache_ttl(*ttl))),
        CacheTtl::Quoted(prose) => {
            p.line("TTL: 0,");
            p.line(&format!(
                "// TODO(runtime): quoted QueryCache.ttl \"{}\" has no CacheSpec prose slot.",
                escape_string(prose)
            ));
        }
    }
    if !cache.tags.is_empty() {
        p.line(&format!(
            "// TODO(runtime): QueryCache.tags not yet in Lazuli Go lib: {}.",
            cache.tags.join(", ")
        ));
    }
    if let Some(namespace) = &cache.namespace {
        p.line(&format!(
            "// TODO(runtime): QueryCache.namespace not yet in Lazuli Go lib: {}.",
            escape_string(namespace)
        ));
    }
    p.dedent();
    p.line("},");
}

fn filter_rule(
    filter: &Filter,
    feature: &Feature,
    resource: Option<&Resource>,
) -> Result<(String, String), String> {
    let Predicate::Comparison { left, op, right } = &filter.predicate else {
        return Err("FilterRule only supports comparison predicates today.".to_owned());
    };
    if !matches!(op, CompareOp::Eq) {
        return Err("FilterRule only supports equality predicates today.".to_owned());
    }

    let left_col = column_from_expr(left, feature, resource);
    let right_col = column_from_expr(right, feature, resource);
    let column = left_col
        .clone()
        .or(right_col)
        .ok_or_else(|| "filter predicate has no column side.".to_owned())?;
    let value_expr = if left_col.is_some() { right } else { left };

    // Owner-via traversal: when a query filters by a FK column whose
    // target resource has a `user` field, and the RHS resolves to
    // `ctx.actor.user_id`, lift the comparison to
    // `<col> IN (SELECT id FROM <related> WHERE user = $N)` via
    // `FromCtxOwnedVia`. Closes the catalog `mine_*` filter shape
    // (host_id = ctx.actor.user_id) without a per-app handler.
    if filter.when.is_none() {
        if let Some(owned_via) = owned_via_source(&column, value_expr, feature, resource) {
            return Ok((column, owned_via));
        }
    }

    let source = match &filter.when {
        Some(param) => format!(
            "lazuli.FromInput(\"{}\")",
            input_source_path(&[param.clone()])
        ),
        None => format_source_expr(value_expr),
    };
    Ok((column, source))
}

/// Render `FromCtxOwnedVia(<related_table>, <owner_column>, <ctx_path>)`
/// when `column` is a FK on `resource` and the RHS is a `ctx.<path>`
/// expression. The owner column is conventionally `"user"` — the FK
/// from the related resource to the User identity — matching the
/// canonical Lazuli auth-identity shape. Returns `None` for any other
/// shape so the caller falls back to scalar `FromCtx(...)` emit.
fn owned_via_source(
    column: &str,
    rhs: &Expr,
    feature: &Feature,
    resource: Option<&Resource>,
) -> Option<String> {
    let resource = resource?;
    let field = resource.fields.iter().find(|f| f.name == column)?;
    let TypeRef::UserDefined(qname) = &field.type_ref else {
        return None;
    };
    let related_type = qname.name.clone();
    // Direct-FK to User collapses to scalar comparison (e.g.
    // `UserSession.user = ctx.actor.user_id`). Owned-via only applies
    // when the FK points to a related resource that itself joins back
    // to User via a `user` column.
    if related_type == "User" {
        return None;
    }
    let Expr::Path(path) = rhs else {
        return None;
    };
    if path.segments.first().map(|s| s.as_str()) != Some("ctx") {
        return None;
    }
    let ctx_path = path.segments[1..].join(".");
    if ctx_path != "actor.user_id" {
        return None;
    }
    let _ = feature;
    let related_table = pascal_to_snake(&related_type);
    Some(format!(
        "lazuli.FromCtxOwnedVia(\"{related_table}\", \"user\", \"{ctx_path}\")"
    ))
}

fn pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn format_source_expr(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => format_path_source(&path.segments),
        Expr::String(s) => format!("lazuli.FromConst(\"{}\")", escape_string(s)),
        Expr::Integer(n) => format!("lazuli.FromConst({n})"),
        Expr::Boolean(b) => format!("lazuli.FromConst({b})"),
        Expr::Enum(literal) => {
            let qualifier = literal
                .type_name
                .as_ref()
                .map(|q| pascal_case(&q.name))
                .unwrap_or_default();
            if qualifier.is_empty() {
                format!("lazuli.FromConst(\"{}\")", escape_string(&literal.variant))
            } else {
                format!(
                    "lazuli.FromConst({}{})",
                    qualifier,
                    pascal_case(&literal.variant)
                )
            }
        }
        Expr::Nil => "lazuli.FromConst(nil)".to_owned(),
        // Query filter RHS — extension fn invocation (`@fn.foo(args)`).
        // Mirrors the binding-source `FromFn` shape: runtime looks up
        // the registered fn by name and applies it to resolved args.
        Expr::FnCall(call) => {
            let arg_sources: Vec<String> = call.args.iter().map(format_source_expr).collect();
            let args_arr = if arg_sources.is_empty() {
                "nil".to_owned()
            } else {
                format!("[]lazuli.Source{{{}}}", arg_sources.join(", "))
            };
            format!(
                "lazuli.FromFn(\"{}\", {})",
                escape_string(&call.name.name),
                args_arr
            )
        }
    }
}

fn format_path_source(segments: &[String]) -> String {
    let head = segments.first().map(|s| s.as_str()).unwrap_or("");
    match head {
        "params" | "input" | "route" => {
            format!(
                "lazuli.FromInput(\"{}\")",
                input_source_path(&segments[1..])
            )
        }
        "ctx" => format!("lazuli.FromCtx(\"{}\")", segments[1..].join(".")),
        "target" => format!(
            "lazuli.FromTarget(\"{}\")",
            input_source_path(&segments[1..])
        ),
        _ => format!("lazuli.FromInput(\"{}\")", input_source_path(segments)),
    }
}

fn column_from_expr(expr: &Expr, feature: &Feature, resource: Option<&Resource>) -> Option<String> {
    match expr {
        Expr::Path(path) if !is_source_path(&path.segments) => Some(normalize_resource_column(
            feature,
            resource,
            &column_from_segments(&path.segments),
        )),
        _ => None,
    }
}

fn is_source_path(segments: &[String]) -> bool {
    matches!(
        segments.first().map(|s| s.as_str()),
        Some("params" | "input" | "ctx" | "target" | "route")
    )
}

fn column_from_segments(segments: &[String]) -> String {
    let trimmed = if segments.first().map(|s| s.as_str()) == Some("self") {
        &segments[1..]
    } else {
        segments
    };
    match trimmed {
        [] => String::new(),
        [one] => one.to_ascii_lowercase(),
        [head, tail] if tail.as_str() == "id" => format!("{}_id", head.to_ascii_lowercase()),
        many => many
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
    }
}

/// Query authors commonly write FK predicates in semantic id form
/// (`user_id = ctx.actor.user_id`) or path form (`user.id = ...`).
/// Lazuli's DDL keeps declared FK fields at their authored column
/// name (`user: User` -> `user`) because `tenancy org` already owns
/// the implicit `org_id` column. Normalize those semantic query
/// columns back to the actual resource column when the resource has
/// a matching FK field, but preserve real columns such as `id` and
/// tenant `org_id`.
///
/// The conventions `[me]` synth lowers its `WHERE` keys using bare
/// field-style segments (`path: ["org"]`, `path: ["user"]`). When
/// the resource has only `tenancy org` (no authored `org` field),
/// the actual DDL column is the implicit `org_id`. Translate
/// `<X>` -> `<X>_id` whenever the resource lacks a field `<X>` but
/// the suffixed `<X>_id` is a known column (e.g. the implicit
/// tenancy column). This is the inverse of the `<X>_id` -> `<X>`
/// translation above and reuses the same `resource_has_column`
/// oracle, so both directions stay consistent.
fn normalize_resource_column(
    feature: &Feature,
    resource: Option<&Resource>,
    column: &str,
) -> String {
    let Some(resource) = resource else {
        return column.to_owned();
    };
    if resource_has_column(feature, resource, column) {
        return column.to_owned();
    }
    if let Some(stem) = column.strip_suffix("_id") {
        if resource
            .fields
            .iter()
            .any(|field| field.name == stem && is_fk_field(field))
        {
            return stem.to_owned();
        }
    } else {
        // Inverse direction: bare path -> suffixed column when the
        // resource carries the suffixed column implicitly (e.g.
        // `tenancy org` -> `org_id`). Only translate when the field
        // form does not exist; explicit `org: Org required` fields
        // keep their literal `org` column per DDL convention.
        let suffixed = format!("{column}_id");
        if !resource.fields.iter().any(|field| field.name == column)
            && resource_has_column(feature, resource, &suffixed)
        {
            return suffixed;
        }
    }
    column.to_owned()
}

fn resource_has_column(feature: &Feature, resource: &Resource, column: &str) -> bool {
    if column == "id" {
        return true;
    }
    if matches!(effective_tenancy(feature, resource), Tenancy::Org) && column == "org_id" {
        return true;
    }
    resource.fields.iter().any(|field| field.name == column)
}

fn is_fk_field(field: &lazuli_ir::Field) -> bool {
    matches!(&field.type_ref, TypeRef::UserDefined(_))
}

fn effective_tenancy(feature: &Feature, resource: &Resource) -> Tenancy {
    resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone())
        .unwrap_or(Tenancy::None)
}

fn input_source_path(segments: &[String]) -> String {
    segments
        .iter()
        .map(|s| pascal_case(s))
        .collect::<Vec<_>>()
        .join(".")
}

fn lookup_args(query: &LookupQuery, resource: Option<&Resource>) -> Vec<TypedSlot> {
    let mut args = query.params.clone();
    for key in &query.keys {
        let name = lookup_arg_name(key);
        if args.iter().any(|slot| slot.name == name) {
            continue;
        }
        args.push(TypedSlot {
            name,
            type_ref: infer_lookup_type(key, resource),
            required: true,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        });
    }
    args
}

fn lookup_arg_name(key: &KeyClause) -> String {
    match &key.equals {
        Expr::Path(path) => {
            let segments = match path.segments.first().map(|s| s.as_str()) {
                Some("params" | "input" | "route") => &path.segments[1..],
                _ => path.segments.as_slice(),
            };
            if segments.is_empty() {
                key.path
                    .segments
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "value".to_owned())
            } else {
                segments.join("_")
            }
        }
        _ => key
            .path
            .segments
            .last()
            .cloned()
            .unwrap_or_else(|| "value".to_owned()),
    }
}

fn infer_lookup_type(key: &KeyClause, resource: Option<&Resource>) -> TypeRef {
    if key.path.segments.last().map(|s| s == "id").unwrap_or(false) {
        return TypeRef::Builtin(BuiltinType::Id);
    }
    if let Some(resource) = resource {
        if let Some(head) = key.path.segments.first() {
            if let Some(field) = resource.fields.iter().find(|field| &field.name == head) {
                return field.type_ref.clone();
            }
        }
    }
    TypeRef::Builtin(BuiltinType::Text)
}

fn register_imports_for_query(
    query: &Query,
    feature: &Feature,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    match query {
        Query::List(q) => {
            for slot in &q.params {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
            if q.cache.as_ref().map(cache_uses_time).unwrap_or(false) {
                imports.add("time");
            }
        }
        Query::Lookup(q) => {
            let resource = resource_for_query(feature, &q.name);
            for slot in lookup_args(q, resource) {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
        }
        Query::Sql(q) => {
            for slot in &q.params {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
            register_imports_for_type(&q.returns, ctx, imports);
            if q.cache.as_ref().map(cache_uses_time).unwrap_or(false) {
                imports.add("time");
            }
        }
    }
}

fn register_imports_for_query_arg_type(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let resolved = query_arg_type_ref(type_ref, ctx);
    register_imports_for_type(&resolved, ctx, imports);
}

fn register_imports_for_type(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}

fn sql_query_callable_kind(query: &SqlQuery) -> &'static str {
    match query.sql_kind {
        lazuli_ir::SqlQueryKind::Sql => "query.sql",
        lazuli_ir::SqlQueryKind::View => "query.view",
    }
}

fn sql_query_returns_many(query: &SqlQuery) -> bool {
    matches!(query.returns, TypeRef::Many(_))
}

fn sql_query_row_type(query: &SqlQuery) -> &TypeRef {
    if query.sql_kind == lazuli_ir::SqlQueryKind::View {
        if let TypeRef::Many(inner) = &query.returns {
            return inner;
        }
    }
    &query.returns
}

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

fn cache_uses_time(cache: &QueryCache) -> bool {
    matches!(cache.ttl, CacheTtl::Literal(_))
}

fn format_cache_ttl(ttl: CacheTtlLiteral) -> String {
    match ttl {
        CacheTtlLiteral::Seconds(n) => format!("{n} * time.Second"),
        CacheTtlLiteral::Minutes(n) => format!("{n} * time.Minute"),
        CacheTtlLiteral::Hours(n) => format!("{n} * time.Hour"),
        CacheTtlLiteral::Days(n) => format!("{} * time.Hour", n.saturating_mul(24)),
    }
}

pub(super) fn resource_for_query<'a>(
    feature: &'a Feature,
    query_name: &str,
) -> Option<&'a Resource> {
    let mut resources: Vec<&Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    if resources.len() <= 1 {
        return resources.into_iter().next();
    }

    let query_tokens = split_ident_tokens(query_name);
    resources
        .into_iter()
        .map(|resource| {
            let tokens = split_ident_tokens(&resource.name);
            let last = tokens.last().cloned().unwrap_or_default();
            let mut score = 0usize;
            for token in &tokens {
                if query_tokens
                    .iter()
                    .any(|q| q == token || q == &plural(token))
                {
                    score += 10;
                }
            }
            if !last.is_empty()
                && query_tokens
                    .iter()
                    .any(|q| q == &last || q == &plural(&last))
            {
                score += 50;
            }
            (score, resource)
        })
        .max_by(|(score_a, a), (score_b, b)| score_a.cmp(score_b).then_with(|| b.name.cmp(&a.name)))
        .map(|(_, resource)| resource)
}

fn query_kind_rank(query: &Query) -> u8 {
    match query {
        Query::List(_) => 0,
        Query::Lookup(_) => 1,
        Query::Sql(q) if q.sql_kind == lazuli_ir::SqlQueryKind::View => 2,
        Query::Sql(_) => 3,
    }
}

fn list_args_struct_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name == "list" {
        format!("List{}Args", plural_pascal(resource_pascal))
    } else {
        format!("{}Args", pascal_case(query_name))
    }
}

pub(super) fn list_var_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name == "list" {
        format!("list{}", plural_pascal(resource_pascal))
    } else {
        lower_camel(query_name)
    }
}

fn lookup_args_struct_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name.starts_with("by_") || query_name == "by" {
        format!("{}{}Args", resource_pascal, pascal_case(query_name))
    } else {
        format!("{}Args", pascal_case(query_name))
    }
}

pub(super) fn lookup_var_name(query_name: &str, resource_pascal: &str) -> String {
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

fn plural_pascal(s: &str) -> String {
    if let Some(stem) = s.strip_suffix('y') {
        return format!("{stem}ies");
    }
    if s.ends_with('s') {
        format!("{s}es")
    } else {
        format!("{s}s")
    }
}

fn return_name(type_ref: &TypeRef, ctx: &TypeCtx<'_>) -> String {
    match type_ref {
        TypeRef::Many(inner) => format!("{}[]", return_name(inner, ctx)),
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => match qname.feature.as_deref() {
            Some(feature) => format!("{}.{}", feature, qname.name),
            None => qname.name.clone(),
        },
        other => {
            let (go, _import) = types::go_type_for(other, ctx);
            go
        }
    }
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn pascal_case(s: &str) -> String {
    super::casing::pascal_case(s)
}

fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
}

/// Local helper used only by `resource_for_query` scoring: split an
/// identifier into lowercase tokens. The shared `casing::split_words`
/// preserves case (`fooBar` → `["foo", "Bar"]`); the scorer wants
/// lowercase tokens for direct equality + plural matching.
fn split_ident_tokens(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower_or_digit = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.to_ascii_lowercase());
                current.clear();
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !current.is_empty() {
            words.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        words.push(current.to_ascii_lowercase());
    }
    words
}

fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ies")
    } else if word.ends_with('s') {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `lazuli.Query[A, R]` value. The runtime dispatcher (`RunList` /
/// `RunLookup`) consults the slice via `lazuli.RunPrelude` before
/// invoking the handler and via `lazuli.RunIncrement` after a
/// successful return. Empty slice is the no-gate fast path — we
/// skip emission entirely so back-compat call sites stay
/// byte-equivalent.
fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
    if gates.is_empty() {
        return;
    }
    p.line("Prelude: []billing.GateRef{");
    p.indent();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => {
                p.line(&format!(
                    "{{Kind: billing.GateBehind, Name: {:?}}},",
                    feature
                ));
            }
            Gate::Quota { limit } => {
                p.line(&format!("{{Kind: billing.GateQuota, Name: {:?}}},", limit));
            }
        }
    }
    p.dedent();
    p.line("},");
}

/// PG.C.2 — translate a `Query` IR variant back to the canonical
/// `<callable_kind>` string used as a key in the emit-time gate
/// map (`<feature>/query.list:<name>`, etc.).
fn query_callable_kind(query: &Query) -> &'static str {
    match query {
        Query::List(_) => "query.list",
        Query::Lookup(_) => "query.lookup",
        Query::Sql(q) => sql_query_callable_kind(q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppManifest, Defaults, Field, Module, Policies, QualifiedName, Record};

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/query.gen.go");
        emit_query_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
    }

    fn emit_from_module(module: &Module, feature_name: &str) -> Option<String> {
        let feature = module
            .features
            .iter()
            .find(|feature| feature.name == feature_name)
            .expect("feature exists");
        let index = CrossFeatureIndex::build(module);
        let emit_ctx = EmitContext::no_source("customer/query.gen.go");
        emit_query_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
    }

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
                title: None,
                version: None,
                lazuli_version: None,
                targets: Vec::new(),
                default_locale: None,
                default_timezone: None,
                auth_failed_redirect: None,
                not_found: None,
                error_pages: Vec::new(),
                uses: Vec::new(),
                packs: Vec::new(),
                bindings: Vec::new(),
                architecture: None,
                services: Vec::new(),
                communication: None,
                environments: Vec::new(),
                urls: Vec::new(),
                cors: None,
                headers: None,
                cookie: None,
                proxy: None,
                limits: None,
                env: Vec::new(),
                integrations: Vec::new(),
                capabilities: Vec::new(),
                runtime: Vec::new(),
                deploy: None,
                logging: None,
                tracing: None,
                observability: None,
                locale: None,
                encryption_bindings: Vec::new(),
                route_guard: None,
                actor_query: None,
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: Vec::new(),
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn slot(name: &str, type_ref: TypeRef, required: bool) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref,
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
        }
    }

    fn qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

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

#[cfg(test)]
mod feature_emit {
    use super::*;
    use lazuli_ir::{AppManifest, Defaults, Field, Module, Policies};

    fn emit(feature: &Feature) -> Option<String> {
        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
                title: None,
                version: None,
                lazuli_version: None,
                targets: Vec::new(),
                default_locale: None,
                default_timezone: None,
                auth_failed_redirect: None,
                not_found: None,
                error_pages: Vec::new(),
                uses: Vec::new(),
                packs: Vec::new(),
                bindings: Vec::new(),
                architecture: None,
                services: Vec::new(),
                communication: None,
                environments: Vec::new(),
                urls: Vec::new(),
                cors: None,
                headers: None,
                cookie: None,
                proxy: None,
                limits: None,
                env: Vec::new(),
                integrations: Vec::new(),
                capabilities: Vec::new(),
                runtime: Vec::new(),
                deploy: None,
                logging: None,
                tracing: None,
                observability: None,
                locale: None,
                encryption_bindings: Vec::new(),
                route_guard: None,
                actor_query: None,
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/query.gen.go");
        emit_query_file(
            "features/customer/customer.lzi",
            feature,
            "lazuli/test",
            &index,
            &emit_ctx,
        )
    }

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn slot(name: &str, type_ref: TypeRef, required: bool) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref,
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
        }
    }

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
