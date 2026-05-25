//! Cell E4 — `query.list` emission. Walks a `ListQuery` and emits the
//! typed `<List><Resource>Args` struct, the
//! `lazuli.Query[Args, Resource]` value (with header, filters, order,
//! paginate, cache), and the exported Go wrapper that lets internal
//! Go callers invoke the list without going through the HTTP router.
//!
//! The args struct / var naming hugs two precedents:
//! - Default `list` queries → `List<Resources>Args` / `list<Resources>`
//!   (plural). Mirrors the canonical Rails `Resource.list` shape.
//! - Named queries (`query.list active_users` or `list active_users`) →
//!   `<Name>Args` / `<lowerCamelName>`. The query name carries its own
//!   semantics so we keep it verbatim and don't bolt on the resource.

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
    let func_name = pascal_case(query_name);
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
