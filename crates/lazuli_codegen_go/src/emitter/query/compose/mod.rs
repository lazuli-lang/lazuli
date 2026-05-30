//! Cell W5 — `query.compose` emission. Lowers a resolved [`ComposeQuery`]
//! (root resource + FK-path JOIN projection + the CLOSED 4-member sub-select
//! catalog) to **one parameterized SQL SELECT** embedded in `query.gen.go`,
//! plus the generated return-record struct.
//!
//! Proposal: `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §5.2.
//!
//! ## Why this reuses the `query.sql` / `query.view` runtime path
//!
//! Compose is NOT a new query engine (`grading-rubric.md:206` — no opaque
//! engine). It generates a SELECT and hands it to the same `pgx` path
//! `query.sql`/`query.view` already use: the emitted value is a
//! `lazuli.Query[Args, Row]{ Kind: lazuli.QueryView, SQLText: "<SELECT>",
//! SQLArgsCtx: …, SQLMany: …, Returns: … }`. The author reads the generated
//! SELECT inline in `query.gen.go` — the exact auditability `query.sql`
//! cannot offer.
//!
//! ## The moat — generated tenant/soft-delete scope (the verifiability proof)
//!
//! When `scope_origin == Inherited` (the secure default), the root `WHERE`
//! gains the tenant + soft-delete predicate by calling the SAME
//! [`super::filters::effective_tenancy`] path `query.list` uses — the author
//! never wrote it, codegen generated it, so a dropped tenant predicate is
//! structurally impossible. The `org_id` bind is supplied from `ctx` (not a
//! param) via `SQLArgsCtx`, so it cannot be omitted or mis-bound.
//! `scope override` (`scope_origin == Overridden`) suppresses it — the
//! author's policy/scope then governs.
//!
//! The SQL generator + bind plan live in the sibling [`sql`] module; this file
//! owns the Go value / struct / wrapper emission.

use lazuli_ir::{
    ComposeProjection, ComposeQuery, ComposeSubselect, Feature, ProjectionSource, Resource,
    SubselectKind, TypeRef,
};

use super::super::error_resolver::emit_operation_error_keys;
use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_QUERY_PGX_SQL, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};
use super::args::{emit_args_struct, query_error_keys_var};
use super::header::{
    emit_gate_annotations, emit_query_header, query_policy_denied_key_for_parts,
};
use super::util::{escape_string, lower_camel, pascal_case, write_section_banner};

mod sql;
use sql::{
    ComposeBind, ComposeSql, build_compose_sql, escape_string_multiline, joined_resource_for_alias,
    root_resource,
};

/// Emit a single `query.compose` — the generated return-record struct, the
/// `lazuli.Query[Args, Row]` value carrying the embedded SELECT, and the
/// exported Go wrapper.
pub(super) fn emit_compose_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &ComposeQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    // Wire registry key: `<feature>.<query_name>`. See list.rs for rationale.
    let qualified_name = format!("{}.{}", feature.name, query.name);
    let args_struct = format!("{}Args", pascal_case(&query.name));
    let var_name = lower_camel(&query.name);
    let error_keys_var = query_error_keys_var(&var_name);
    let row_type = return_record_name(query);
    // `key` ⇒ single-row read (§3.2 #6); absence ⇒ list semantics.
    let single_row = query.key.is_some();

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.compose {}", query.name),
        ],
    );

    // The generated return record — 1:1 with `projections[]`. Each projection
    // name is both the SQL `AS <name>` alias and the struct's `db:"<name>"`
    // tag, so `pgx.RowToStructByName` lines up by construction.
    emit_compose_row_struct(p, &row_type, query, feature, ctx);
    p.blank();

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

    // Build the one parameterized SELECT + its ordered positional bind plan.
    let compose_sql = build_compose_sql(query, feature);

    emit_pattern_header(p, PATTERN_QUERY_PGX_SQL);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, query.span_ref);
    p.line(&format!(
        "var {var_name} = lazuli.Query[{args_struct}, {row_type}]{{"
    ));
    p.indent();
    // Compose lowers onto the SQL-backed runtime path (`QueryView`), so the
    // embedded SELECT runs through the same pgx scanner as `query.view`.
    emit_query_header(
        p,
        feature,
        &qualified_name,
        None,
        "lazuli.QueryView",
        emit_ctx,
        &query.name,
        query.span_ref,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
        &error_keys_var,
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.compose", &query.name));
    if query.scope_override {
        // Author opted out of inherited tenancy (requires policy + reason,
        // doctor-gated). The generated SELECT carries NO tenant predicate;
        // the author's scope/policy governs. Surfaced as a breadcrumb.
        p.line("// scope override: inherited tenant/soft-delete predicate suppressed (§5.2).");
    }
    emit_sql_text(p, &compose_sql);
    p.line(&format!("Returns: \"{}\",", escape_string(&row_type)));
    p.line(&format!("SQLMany: {},", !single_row));
    emit_sql_args_from_binds(p, &compose_sql.binds, &args_struct);
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
    p.blank();
    emit_compose_query_wrapper(p, &query.name, &var_name, &args_struct, &row_type, single_row);
}

/// W5 import-registration hook — the resolved [`TypeRef`]s the generated
/// return record's fields project from, so `register_imports_for_query` (in
/// `args.rs`) can pull each field type's package (e.g. `time` for a projected
/// `DateTime` column) before the banner is painted. Subselect projections
/// (`count`/`exists`/`latest`/`aggregate`) lower to builtin Go scalars
/// (`int64`/`bool`/`*string`) that need no import, so only `self.`/`<alias>.`
/// column projections contribute a `TypeRef`.
pub(super) fn compose_projection_import_types(
    query: &ComposeQuery,
    feature: &Feature,
) -> Vec<TypeRef> {
    let mut out = Vec::new();
    for proj in &query.projections {
        let (resource, col) = match &proj.source {
            ProjectionSource::SelfCol(col) => (root_resource(query, feature), col.as_str()),
            ProjectionSource::Joined(alias, col) => {
                (joined_resource_for_alias(query, feature, alias), col.as_str())
            }
            ProjectionSource::Subselect(_) => continue,
        };
        if let Some(resource) = resource {
            if let Some(field) = resource.fields.iter().find(|f| f.name == col) {
                out.push(field.type_ref.clone());
            }
        }
    }
    out
}

/// Generated return-record name for a `query.compose`. `returns` always
/// carries a resolved [`TypeRef`] (the analyzer defaults it to `<Compose>Row`),
/// so we just project it to its Pascal Go type name.
fn return_record_name(query: &ComposeQuery) -> String {
    match &query.returns {
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => pascal_case(&qname.name),
        TypeRef::Many(inner) => match inner.as_ref() {
            TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => pascal_case(&qname.name),
            _ => format!("{}Row", pascal_case(&query.name)),
        },
        _ => format!("{}Row", pascal_case(&query.name)),
    }
}

// ----------------------------------------------------------------------------
// Generated return-record struct
// ----------------------------------------------------------------------------

/// Emit the `type <Compose>Row struct { ... }` derived 1:1 from
/// `projections[]`. Each field's `db:"<name>"` tag equals the projection
/// name, which is also the SQL `AS <name>` alias — so `RowToStructByName`
/// resolves every column to its field. The Go type is inferred from the
/// projection's source (root/joined column type, or the subselect's shape).
fn emit_compose_row_struct(
    p: &mut GoPrinter,
    row_type: &str,
    query: &ComposeQuery,
    feature: &Feature,
    ctx: &TypeCtx<'_>,
) {
    p.line(&format!(
        "// {row_type} is the row materialised from the query.compose `{}` SELECT.",
        query.name
    ));
    p.line("// Fields derive 1:1 from the `select` projections; each `db` tag is the");
    p.line("// SQL column alias so pgx scans by name.");
    p.line(&format!("type {row_type} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> = Vec::with_capacity(query.projections.len());
    for proj in &query.projections {
        let (go_type, _import) = compose_projection_go_type(proj, query, feature, ctx);
        let tag = format!("`db:\"{0}\" json:\"{0}\"`", escape_string(&proj.name));
        rows.push((pascal_case(&proj.name), go_type, tag));
    }
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(n, t, g)| (n.as_str(), t.as_str(), g.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

/// Resolve the Go type for one projection. `self.<col>` / `<alias>.<col>`
/// read the underlying field's type (defaulting to `string` when the column
/// is not statically resolvable — e.g. a cross-feature joined node). A
/// `subselect` ref types from the subselect kind: `count` → `int64`,
/// `exists` → `bool`, `latest`/`aggregate` → a nullable scalar.
fn compose_projection_go_type(
    proj: &ComposeProjection,
    query: &ComposeQuery,
    feature: &Feature,
    ctx: &TypeCtx<'_>,
) -> (String, Option<String>) {
    match &proj.source {
        ProjectionSource::SelfCol(col) => {
            field_go_type(root_resource(query, feature), col, ctx, false)
        }
        ProjectionSource::Joined(alias, col) => {
            let resource = joined_resource_for_alias(query, feature, alias);
            // A column read off a LEFT (`optional`) join can be NULL even when
            // the underlying field is `required`, so the projection is nullable
            // regardless of field optionality. COMPOSE-NULLABILITY-MISMATCH-001
            // (W3) cross-checks the record contract; here we map to a pointer so
            // the generated struct scans NULL safely.
            let left_joined = query
                .joins
                .iter()
                .find(|j| &j.alias == alias)
                .map(|j| j.nullable)
                .unwrap_or(false);
            field_go_type(resource, col, ctx, left_joined)
        }
        ProjectionSource::Subselect(name) => {
            let Some(sub) = query.subselects.iter().find(|s| &s.name == name) else {
                return ("string".to_owned(), None);
            };
            compose_subselect_go_type(sub)
        }
    }
}

/// Go type for a subselect projection (§5.2). `count` is a non-null `int64`;
/// `exists` is a non-null `bool`; `latest`/`aggregate` are nullable scalars
/// (`SELECT ... LIMIT 1` / aggregate over zero rows can be NULL), so they map
/// to a pointer.
fn compose_subselect_go_type(sub: &ComposeSubselect) -> (String, Option<String>) {
    match &sub.kind {
        SubselectKind::Count(_) => ("int64".to_owned(), None),
        SubselectKind::Exists { .. } => ("bool".to_owned(), None),
        SubselectKind::Latest { .. } | SubselectKind::Aggregate { .. } => {
            // Nullable by nature — no COALESCE is generated, so the column may
            // be NULL when the correlated set is empty. Map to a pointer.
            ("*string".to_owned(), None)
        }
    }
}

/// Resolve a field's Go type on `resource`, falling back to `string` when the
/// resource or field can't be resolved in-feature (cross-feature joins are
/// trusted; doctor validates them with Module context). `force_nullable` makes
/// the type a pointer regardless of the field's declared optionality — set for
/// columns read off a LEFT (`optional`) join, which can yield NULL.
fn field_go_type(
    resource: Option<&Resource>,
    col: &str,
    ctx: &TypeCtx<'_>,
    force_nullable: bool,
) -> (String, Option<String>) {
    // `id` is the implicit identity column; `org_id` the implicit tenant
    // column. Neither is a declared field but both are `lazuli.ID`.
    if col == "id" || col == "org_id" {
        let go = if force_nullable { "*lazuli.ID" } else { "lazuli.ID" };
        return (go.to_owned(), None);
    }
    let Some(resource) = resource else {
        let go = if force_nullable { "*string" } else { "string" };
        return (go.to_owned(), None);
    };
    let Some(field) = resource.fields.iter().find(|f| f.name == col) else {
        let go = if force_nullable { "*string" } else { "string" };
        return (go.to_owned(), None);
    };
    let (go_type, import) = types::go_type_for(&field.type_ref, ctx);
    if field.required && !force_nullable {
        (go_type, import)
    } else {
        (format!("*{go_type}"), import)
    }
}

// ----------------------------------------------------------------------------
// SQLText + SQLArgsCtx + Go wrapper emission
// ----------------------------------------------------------------------------

/// Emit `SQLText: ` + the generated SELECT as a Go double-quoted literal with
/// embedded newlines (`\n`). Kept as a single `"..."` so the author reads the
/// whole SELECT inline in `query.gen.go`.
fn emit_sql_text(p: &mut GoPrinter, sql: &ComposeSql) {
    p.line(&format!("SQLText: \"{}\",", escape_string_multiline(&sql.text)));
}

/// Emit `SQLArgsCtx: func(ctx *lazuli.Ctx, args FooArgs) []any { ... }` from a
/// precomputed bind plan — the ctx-aware positional bind projection. The
/// generated tenant `org_id` + actor refs are read from ctx (NOT params — the
/// author can't omit or mis-bind them); every `params.<name>` bind reads from
/// the typed args struct. When the compose has no binds, returns `nil`.
fn emit_sql_args_from_binds(p: &mut GoPrinter, binds: &[ComposeBind], args_struct: &str) {
    p.line(&format!(
        "SQLArgsCtx: func(ctx *lazuli.Ctx, args {args_struct}) []any {{"
    ));
    p.indent();
    if binds.is_empty() {
        p.line("return nil");
    } else {
        p.line("return []any{");
        p.indent();
        for bind in binds {
            match bind {
                // The generated tenant bind resolves through the SAME ctx
                // resolver `query.list` scope uses — fail-closed (nil when no
                // tenant), never a hand-written field access the author could
                // get wrong.
                ComposeBind::TenantOrg => p.line("lazuli.CtxValue(ctx, \"tenant.org_id\"),"),
                ComposeBind::Ctx(path) => {
                    p.line(&format!("lazuli.CtxValue(ctx, \"{}\"),", escape_string(path)))
                }
                ComposeBind::Param(name) => p.line(&format!("args.{},", pascal_case(name))),
            }
        }
        p.dedent();
        p.line("}");
    }
    p.dedent();
    p.line("},");
}

/// Emit the exported Go wrapper around the package-private compose value.
/// Single-row composes return `(<Row>, error)`; list composes return
/// `([]<Row>, error)`. Both delegate to `RunSQL` (the `QueryView` path).
fn emit_compose_query_wrapper(
    p: &mut GoPrinter,
    query_name: &str,
    var_name: &str,
    args_struct: &str,
    row_type: &str,
    single_row: bool,
) {
    let func_name = pascal_case(query_name);
    emit_pattern_header(p, PATTERN_QUERY_PGX_SQL);
    p.line(&format!(
        "// {func_name} is the exported Go wrapper around the package-private"
    ));
    p.line(&format!(
        "// `{var_name}` compose value. The composite read lowers to one"
    ));
    p.line("// parameterized SELECT (see SQLText); this wrapper runs it through");
    p.line("// the shared RunSQL pgx path so Go-internal callers can invoke it.");
    let ret = if single_row {
        format!("({row_type}, error)")
    } else {
        format!("([]{row_type}, error)")
    };
    p.line(&format!(
        "func {func_name}(ctx *lazuli.Ctx, args {args_struct}) {ret} {{"
    ));
    p.indent();
    p.line(&format!("out, err := {var_name}.RunSQL(ctx, args)"));
    p.line("if err != nil {");
    p.indent();
    if single_row {
        p.line(&format!("var zero {row_type}"));
        p.line("return zero, err");
    } else {
        p.line("return nil, err");
    }
    p.dedent();
    p.line("}");
    if single_row {
        p.line(&format!("return out.({row_type}), nil"));
    } else {
        p.line(&format!("return out.([]{row_type}), nil"));
    }
    p.dedent();
    p.line("}");
}

#[cfg(test)]
mod tests;
