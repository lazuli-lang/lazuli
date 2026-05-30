//! Cell E4 — `Filters`, `Order`, and `LookupBy` projection from
//! IR predicates onto the runtime's `lazuli.FilterRule` /
//! `lazuli.OrderClause` / `lazuli.LookupKey` value shapes.
//!
//! This file owns the source-resolution surface — the `FromInput` /
//! `FromCtx` / `FromTarget` / `FromCtxOwnedVia` / `FromConst` / `FromFn`
//! family of factories. Filters are emitted only when at least one
//! rule survived `filter_rule` (which can fail for non-equality
//! predicates — the runtime today only consumes equality).
//!
//! Owner-via FK lift: when a query filters `<fk> = ctx.actor.user_id`
//! and the FK target itself owns a `user` column,
//! `owned_via_source` lifts the comparison to the IN-subquery shape
//! `FromCtxOwnedVia(<related_table>, "user", "actor.user_id")`. The
//! runtime expands both this and the `OwnerScopeSql` synth output
//! through the same `whereConditionFragment` so the SQL stays
//! consistent regardless of whether the predicate was hand-authored
//! or synthesised.
//!
//! Column normalization: the conventions `[me]` synth emits keys in
//! semantic form (`user.id`, `user_id`, bare `user`). DDL keeps the
//! authored column name. `normalize_resource_column` reconciles
//! both directions so the emitted SQL column always matches the
//! migrated schema.

use lazuli_ir::{
    CompareOp, Expr, Feature, Field, Filter, KeyClause, OrderBy, OrderDir, OwnerScopeSql,
    Predicate, Resource, Tenancy, TypeRef,
};

use super::super::printer::GoPrinter;
use super::util::{escape_string, pascal_case, pascal_to_snake};

/// Emit `Filters: []lazuli.FilterRule{ ... }`, including the
/// synth-side owner-scope predicate appended after author-side
/// filters. Skipped entirely when both surfaces are empty so the
/// generated Go stays minimal.
pub(super) fn emit_filters(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    filters: &[Filter],
    owner_scope: Option<&OwnerScopeSql>,
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
fn owner_scope_filter_entry(scope: &OwnerScopeSql) -> (String, String) {
    let fk_table = pascal_to_snake(&scope.fk_target);
    let source = format!(
        "lazuli.FromCtxOwnedVia(\"{}\", \"{}\", \"user.id\")",
        escape_string(&fk_table),
        escape_string(&scope.through_column),
    );
    (scope.field_name.clone(), source)
}

/// Emit `Order: []lazuli.OrderClause{ ... }`. Skipped when no
/// `order` clause was authored.
pub(super) fn emit_order(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    order: &[OrderBy],
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
pub(super) fn emit_lookup_by_with_filters(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: Option<&Resource>,
    keys: &[KeyClause],
    filters: &[Filter],
    owner_scope: Option<&OwnerScopeSql>,
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

/// Resolve a single `Filter` to a `(column, source)` pair. Returns
/// `Err(message)` when the predicate is unsupported (non-equality,
/// no column side); callers emit the message as a `// TODO(ir):`
/// breadcrumb in the generated Go.
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

/// Render an IR `Expr` as a runtime `lazuli.Source`. The five
/// source factories — `FromInput`, `FromCtx`, `FromTarget`,
/// `FromConst`, `FromFn` — cover every shape Lazuli's analyzer
/// produces for filter/key RHS.
pub(super) fn format_source_expr(expr: &Expr) -> String {
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

pub(super) fn column_from_segments(segments: &[String]) -> String {
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
pub(super) fn normalize_resource_column(
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

fn is_fk_field(field: &Field) -> bool {
    matches!(&field.type_ref, TypeRef::UserDefined(_))
}

pub(super) fn effective_tenancy(feature: &Feature, resource: &Resource) -> Tenancy {
    resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone())
        .unwrap_or(Tenancy::None)
}

pub(super) fn input_source_path(segments: &[String]) -> String {
    segments
        .iter()
        .map(|s| pascal_case(s))
        .collect::<Vec<_>>()
        .join(".")
}
