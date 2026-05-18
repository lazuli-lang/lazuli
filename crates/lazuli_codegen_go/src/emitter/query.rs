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
    TypeRef, TypedSlot,
};

use super::cross_feature::CrossFeatureIndex;
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
    let any_gated = queries
        .iter()
        .any(|q| !emit_ctx.gates_for(query_callable_kind(q), q.name()).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }

    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
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
    let qualified_name = format!("{}.query.{}", feature.name, query.name);
    let args_struct = list_args_struct_name(&query.name, &resource_name_axis);
    let var_name = list_var_name(&query.name, &resource_name_axis);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.list {}", query.name),
        ],
    );

    emit_args_struct(p, &args_struct, &query.params, ctx);
    p.blank();

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
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.list", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    if query.modifier.is_some() {
        p.line("// TODO(runtime): ListQuery.modifier is not yet in Lazuli Go lib.");
    }
    emit_filters(p, &query.filters);
    emit_order(p, &query.order);
    if let Some(page_size) = query.paginate {
        p.line(&format!("Paginate: {page_size},"));
    }
    if let Some(cache) = &query.cache {
        emit_cache(p, cache);
    }
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
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
    let qualified_name = format!("{}.query.{}", feature.name, query.name);
    let args_struct = lookup_args_struct_name(&query.name, &resource_name_axis);
    let var_name = lookup_var_name(&query.name, &resource_name_axis);
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
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.lookup", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    if !query.filters.is_empty() {
        p.line("// TODO(runtime): LookupQuery.filters are not applied by Lazuli Go RunLookup yet.");
    }
    emit_lookup_by(p, &query.keys);
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

fn emit_sql_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &SqlQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.query.{}", feature.name, query.name);
    let args_struct = format!("{}Args", pascal_case(&query.name));
    let var_name = lower_camel(&query.name);
    let (return_type, _import) = types::go_type_for(&query.returns, ctx);
    let returns_name = return_name(&query.returns, ctx);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.sql {}", query.name),
        ],
    );

    emit_args_struct(p, &args_struct, &query.params, ctx);
    p.blank();

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
        "lazuli.QuerySQL",
        emit_ctx,
        &query.name,
        query.span_ref,
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.sql", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    p.line(&format!(
        "SQL:     \"./queries/{}.sql\",",
        escape_string(&query.name)
    ));
    p.line(&format!("Returns: \"{}\",", escape_string(&returns_name)));
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
    let policy = match &feature.defaults.policy {
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
    };
    kv_rows.push(("Policy:".to_owned(), policy));

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "query", op, span);
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

fn emit_filters(p: &mut GoPrinter, filters: &[Filter]) {
    if filters.is_empty() {
        return;
    }
    p.line("Filters: []lazuli.FilterRule{");
    p.indent();
    for filter in filters {
        match filter_rule(filter) {
            Ok((column, source)) => p.line(&format!(
                "{{Column: \"{}\", When: {source}}},",
                escape_string(&column)
            )),
            Err(message) => p.line(&format!("// TODO(ir): {message}")),
        }
    }
    p.dedent();
    p.line("},");
}

fn emit_order(p: &mut GoPrinter, order: &[lazuli_ir::OrderBy]) {
    if order.is_empty() {
        return;
    }
    p.line("Order: []lazuli.OrderClause{");
    p.indent();
    for clause in order {
        let desc = matches!(clause.direction, OrderDir::Desc);
        p.line(&format!(
            "{{Column: \"{}\", Desc: {desc}}},",
            escape_string(&column_from_segments(&[clause.field.clone()]))
        ));
    }
    p.dedent();
    p.line("},");
}

fn emit_lookup_by(p: &mut GoPrinter, keys: &[KeyClause]) {
    if keys.is_empty() {
        return;
    }
    p.line("LookupBy: []lazuli.LookupKey{");
    p.indent();
    for key in keys {
        let column = column_from_segments(&key.path.segments);
        let source = format_source_expr(&key.equals);
        p.line(&format!(
            "{{Column: \"{}\", Source: {source}}},",
            escape_string(&column)
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

fn filter_rule(filter: &Filter) -> Result<(String, String), String> {
    let Predicate::Comparison { left, op, right } = &filter.predicate else {
        return Err("FilterRule only supports comparison predicates today.".to_owned());
    };
    if !matches!(op, CompareOp::Eq) {
        return Err("FilterRule only supports equality predicates today.".to_owned());
    }

    let left_col = column_from_expr(left);
    let right_col = column_from_expr(right);
    let column = left_col
        .or(right_col)
        .ok_or_else(|| "filter predicate has no column side.".to_owned())?;
    let source = match &filter.when {
        Some(param) => format!(
            "lazuli.FromInput(\"{}\")",
            input_source_path(&[param.clone()])
        ),
        None => {
            if column_from_expr(left).is_some() {
                format_source_expr(right)
            } else {
                format_source_expr(left)
            }
        }
    };
    Ok((column, source))
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

fn column_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) if !is_source_path(&path.segments) => {
            Some(column_from_segments(&path.segments))
        }
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

pub(super) fn resource_for_query<'a>(feature: &'a Feature, query_name: &str) -> Option<&'a Resource> {
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
        Query::Sql(_) => 2,
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
                p.line(&format!(
                    "{{Kind: billing.GateQuota, Name: {:?}}},",
                    limit
                ));
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
        Query::Sql(_) => "query.sql",
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
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
            previous_names: Vec::new(),
            span_ref: None,
        }));
        let mut account = base_feature("account");
        account.resources.push(resource("User", Vec::new()));
        let module = module_with_features(vec![customer, account]);

        let out = emit_from_module(&module, "customer").expect("must emit");
        assert!(out.contains("\"lazuli/test/account\""));
        assert!(out.contains("User *accountgen.User `json:\"user,omitempty\"`"));
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
            previous_names: Vec::new(),
            span_ref: None,
        }));
        let mut account = base_feature("account");
        account.resources.push(resource("User", Vec::new()));
        let module = module_with_features(vec![customer, account]);

        let out = emit_from_module(&module, "customer").expect("must emit");
        assert!(out.contains("\"lazuli/test/account\""));
        assert!(out.contains("Reviewers []accountgen.User `json:\"reviewers\"`"));
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
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerByEmailArgs struct {"));
        assert!(out.contains("Email lazuli.Email `json:\"email\"`"));
        assert!(out.contains("var customerByEmail = lazuli.Query[CustomerByEmailArgs, Customer]{"));
        assert!(out.contains("Kind:     lazuli.QueryLookup,"));
        assert!(out.contains("{Column: \"email\", Source: lazuli.FromInput(\"Email\")},"));
    }

    #[test]
    fn sql_query_emits_sql_path_returns_and_quoted_cache_todo() {
        let mut feature = base_feature("customer");
        feature.records.push(record("CustomerLtv"));
        feature.queries.push(Query::Sql(SqlQuery {
            name: "lifetime_value".to_owned(),
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
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type LifetimeValueArgs struct {"));
        assert!(out.contains("MinScore *int64 `json:\"min_score,omitempty\"`"));
        assert!(out.contains("var lifetimeValue = lazuli.Query[LifetimeValueArgs, []CustomerLtv]{"));
        assert!(out.contains("Kind:     lazuli.QuerySQL,"));
        assert!(out.contains("SQL:     \"./queries/lifetime_value.sql\","));
        assert!(out.contains("Returns: \"CustomerLtv[]\","));
        assert!(out.contains("// TODO(runtime): quoted QueryCache.ttl"));
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
            previous_names: Vec::new(),
            span_ref: None,
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
            previous_names: Vec::new(),
            span_ref: None,
        }));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
        let alpha_pos = a.find("Query: customer.query.alpha").expect("alpha banner");
        let zebra_pos = a.find("Query: customer.query.zebra").expect("zebra banner");
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
            previous_names: Vec::new(),
            span_ref: None,
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
            previous_names: Vec::new(),
            span_ref: None,
        }));
        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("Prelude:"), "no Prelude when no gates:\n{out}");
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates:\n{out}"
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
        emit_query_file("features/customer/customer.lzi", feature, "lazuli/test", &index, &emit_ctx)
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
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
}
