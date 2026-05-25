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
