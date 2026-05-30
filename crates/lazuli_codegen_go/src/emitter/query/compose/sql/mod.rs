//! Cell W5 — the `query.compose` SQL generator. Builds the ONE parameterized
//! SELECT a [`ComposeQuery`] lowers to (§5.2), plus the ordered positional
//! bind plan the `SQLArgsCtx` projection consumes.
//!
//! Shape:
//! - root `FROM "<table>" root` + each `join` → `[LEFT] JOIN "<table>" <alias>
//!   ON <alias>.id = <parent>.<fk>` (LEFT when nullable; the belongs-to FK
//!   direction the audited handlers use).
//! - each `subselect` → a correlated scalar sub-select keyed on `related_by`'s
//!   last FK segment: `COUNT` / `[NOT] EXISTS` / `latest` (`ORDER BY … LIMIT
//!   1`) / `aggregate` (`<fn>(…) [FILTER (WHERE …)]`).
//! - `filters` + the generated tenant/soft-delete scope (when inherited) →
//!   root `WHERE` predicates; `key` ⇒ single-row identity predicate.
//! - `order` / `paginate` (list shape only).
//!
//! The bind plan is allocated in **SQL text order** — SELECT-list subselect
//! ctx/param binds first, then root-WHERE binds — so positional `$N` indices
//! line up with `DB().Query(ctx, sqlText, values...)`.

use lazuli_ir::{
    AggFn, ComposeJoin, ComposeQuery, ComposeSubselect, Feature, ProjectionSource, Resource,
    SubselectKind, Tenancy, TypeRef,
};

use super::super::filters::effective_tenancy;
use super::super::util::pascal_to_snake;

mod predicates;
use predicates::{
    indent_join, next_bind, quote_ident, render_child_predicate, render_key_predicate,
    render_order, render_root_predicate, type_ref_table,
};

/// The fully-built SELECT plus the ordered bind plan. `binds` lists, in `$N`
/// order, where each positional value comes from: a tenant `org_id` from ctx,
/// a `ctx.<path>` actor ref, or a named param.
pub(super) struct ComposeSql {
    pub(super) text: String,
    pub(super) binds: Vec<ComposeBind>,
}

/// One positional bind in the generated SELECT, in `$N` text order.
pub(super) enum ComposeBind {
    /// `org_id = $N` — the tenant id read from ctx (the moat).
    TenantOrg,
    /// A named `params.<name>` arg (the key clause / param filters).
    Param(String),
    /// A `ctx.<path>` actor-axis ref (e.g. `user.id`), resolved from ctx via
    /// `lazuli.CtxValue` — the author never writes a bind index for it.
    Ctx(String),
}

/// Build the one parameterized SELECT for a [`ComposeQuery`] (§5.2). Returns
/// the SQL text + the ordered bind plan codegen needs to emit `SQLArgsCtx`.
pub(super) fn build_compose_sql(query: &ComposeQuery, feature: &Feature) -> ComposeSql {
    let root = root_resource(query, feature);
    let root_table = type_ref_table(&query.root);

    let mut binds: Vec<ComposeBind> = Vec::new();

    // --- SELECT list: projections in authored order ------------------------
    // Subselect binds (ctx actor refs) are allocated HERE, before WHERE binds,
    // because they appear earlier in the SQL text — `$N` order must match text
    // order so the positional bind plan stays correct.
    let mut select_items: Vec<String> = Vec::with_capacity(query.projections.len());
    for proj in &query.projections {
        let expr = match &proj.source {
            ProjectionSource::SelfCol(col) => format!("root.{}", quote_ident(col)),
            ProjectionSource::Joined(alias, col) => {
                format!("{}.{}", quote_ident(alias), quote_ident(col))
            }
            ProjectionSource::Subselect(name) => match query
                .subselects
                .iter()
                .find(|s| &s.name == name)
            {
                Some(sub) => render_subselect(sub, feature, &mut binds),
                None => "NULL".to_owned(),
            },
        };
        select_items.push(format!("{expr} AS {}", quote_ident(&proj.name)));
    }

    // --- FROM + JOINs ------------------------------------------------------
    let mut from_lines: Vec<String> = vec![format!("FROM {} root", quote_ident(&root_table))];
    for join in &query.joins {
        from_lines.push(render_join(join, root, feature));
    }

    // --- WHERE: generated tenant/soft-delete scope + key + param filters ---
    let mut where_conds: Vec<String> = Vec::new();
    // THE MOAT — generated tenant + soft-delete predicate. Inherited unless
    // the author wrote `scope override`. Calls the SAME `effective_tenancy`
    // path `query.list` uses (filters.rs) so the predicate cannot drift.
    if !query.scope_override {
        if let Some(root) = root {
            if root.soft_delete {
                where_conds.push("root.deleted_at IS NULL".to_owned());
            }
            if matches!(effective_tenancy(feature, root), Tenancy::Org) {
                let n = next_bind(&mut binds, ComposeBind::TenantOrg);
                where_conds.push(format!("root.org_id = ${n}"));
            }
        }
    }
    // `key` clause — single-row identity predicate (e.g. `self.id = $N`).
    if let Some(key) = &query.key {
        if let Some(cond) = render_key_predicate(key, &mut binds) {
            where_conds.push(cond);
        }
    }
    // Author `filters` — equality predicates over root columns. Param-bound
    // RHS becomes a positional bind; literal RHS is inlined.
    for filter in &query.filters {
        if let Some(cond) = render_root_predicate(&filter.predicate, &mut binds) {
            where_conds.push(cond);
        }
    }

    // --- ORDER BY + LIMIT --------------------------------------------------
    let order_clause = render_order(&query.order);
    let limit_clause = if !single_row(query) {
        Some(format!("LIMIT {}", query.paginate.unwrap_or(100)))
    } else {
        None
    };

    // --- assemble ----------------------------------------------------------
    let mut text = String::new();
    text.push_str("SELECT\n");
    text.push_str(&indent_join(&select_items, ",\n"));
    text.push('\n');
    text.push_str(&from_lines.join("\n"));
    if !where_conds.is_empty() {
        text.push_str("\nWHERE ");
        text.push_str(&where_conds.join(" AND "));
    }
    if let Some(order) = order_clause {
        text.push('\n');
        text.push_str(&order);
    }
    if let Some(limit) = limit_clause {
        text.push('\n');
        text.push_str(&limit);
    }

    ComposeSql { text, binds }
}

/// `true` when the compose is a single-row read (`key` present).
pub(super) fn single_row(query: &ComposeQuery) -> bool {
    query.key.is_some()
}

/// Render one `[LEFT] JOIN "<table>" <alias> ON ...` line for a belongs-to FK
/// path. The FK path is rooted at the source resource's snake name; each hop
/// after the anchor names an FK field walked onto its target. For the
/// belongs-to direction (the audited handler shape) the parent holds the FK
/// column pointing at the joined row's id: `ON <alias>.id = <parent>.<fk>`.
/// Multi-hop paths emit intermediate joins anchored on the prior alias.
fn render_join(join: &ComposeJoin, root: Option<&Resource>, feature: &Feature) -> String {
    let kw = if join.nullable { "LEFT JOIN" } else { "JOIN" };
    // Walk the hops after the anchor. `current` tracks the resolved resource at
    // each hop so the FK column + target table resolve in-feature; cross-feature
    // hops fall back to a snake-cased table name.
    let hops: &[String] = join.path.segments.get(1..).unwrap_or(&[]);
    let mut current: Option<&Resource> = root;
    let mut parent_alias = "root".to_owned();
    let mut lines: Vec<String> = Vec::new();
    for (i, hop) in hops.iter().enumerate() {
        let is_last = i + 1 == hops.len();
        let alias = if is_last {
            join.alias.clone()
        } else {
            hop.clone()
        };
        let (target_table, target_resource) = resolve_fk_hop(current, hop, feature);
        lines.push(format!(
            "{kw} {} {} ON {}.{} = {}.{}",
            quote_ident(&target_table),
            quote_ident(&alias),
            quote_ident(&alias),
            quote_ident("id"),
            quote_ident(&parent_alias),
            quote_ident(hop),
        ));
        parent_alias = alias;
        current = target_resource;
    }
    if lines.is_empty() {
        // Defensive: a join path with only the anchor — emit a trivially safe
        // line so codegen stays panic-free; doctor (W3) rejects the shape.
        format!(
            "{kw} {} {} ON true",
            quote_ident(&join.alias),
            quote_ident(&join.alias)
        )
    } else {
        lines.join("\n")
    }
}

/// Resolve an FK hop's `(target_table, target_resource)` against the in-feature
/// relation graph. `UserDefined`-typed FK fields name their target resource;
/// cross-feature / unresolved hops fall back to a snake-cased table derived
/// from the hop name (doctor validates the cross-feature edge).
pub(super) fn resolve_fk_hop<'a>(
    current: Option<&'a Resource>,
    hop: &str,
    feature: &'a Feature,
) -> (String, Option<&'a Resource>) {
    if let Some(resource) = current {
        if let Some(field) = resource.fields.iter().find(|f| f.name == hop) {
            if let TypeRef::UserDefined(qname) = &field.type_ref {
                let target = feature.resources.iter().find(|r| r.name == qname.name);
                return (pascal_to_snake(&qname.name), target);
            }
            if let Some(cf) = &field.cross_feature_target {
                return (pascal_to_snake(&cf.resource), None);
            }
        }
    }
    // Unresolvable in-feature: snake the hop name itself as the table.
    (hop.to_owned(), None)
}

/// Render a closed-catalog subselect as a correlated scalar sub-select
/// (§5.2). The child correlates to root by `related_by`'s last FK segment:
/// `child.<fk> = root.id`. `where`/`filter` predicates AND onto it; `ctx.*`
/// refs allocate positional binds (in text order) so the actor predicate
/// (`sender != ctx.user.id`) is bound from ctx, not inlined.
fn render_subselect(
    sub: &ComposeSubselect,
    feature: &Feature,
    binds: &mut Vec<ComposeBind>,
) -> String {
    let (child_table, child_resource) = subselect_child(sub, feature);
    let child_alias = "c";
    let fk_col = sub
        .related_by
        .segments
        .last()
        .cloned()
        .unwrap_or_else(|| "id".to_owned());
    let correlation = format!(
        "{}.{} = root.{}",
        child_alias,
        quote_ident(&fk_col),
        quote_ident("id")
    );

    // Closed predicate `where` clauses are ANDed onto the correlation.
    let mut conds = vec![correlation];
    for pred in &sub.where_pred {
        if let Some(c) = render_child_predicate(pred, child_alias, child_resource, binds) {
            conds.push(c);
        }
    }
    let where_sql = conds.join(" AND ");

    match &sub.kind {
        SubselectKind::Count(_) => format!(
            "(SELECT COUNT(*) FROM {} {} WHERE {})",
            quote_ident(&child_table),
            child_alias,
            where_sql
        ),
        SubselectKind::Exists { negate, .. } => {
            let kw = if *negate { "NOT EXISTS" } else { "EXISTS" };
            format!(
                "{kw} (SELECT 1 FROM {} {} WHERE {})",
                quote_ident(&child_table),
                child_alias,
                where_sql
            )
        }
        SubselectKind::Latest { column, .. } => {
            let order = if sub.order.is_empty() {
                String::new()
            } else {
                format!(" {}", render_order(&sub.order).unwrap_or_default())
            };
            format!(
                "(SELECT {}.{} FROM {} {} WHERE {}{} LIMIT 1)",
                child_alias,
                quote_ident(column),
                quote_ident(&child_table),
                child_alias,
                where_sql,
                order
            )
        }
        SubselectKind::Aggregate { func, column, .. } => {
            let fn_sql = agg_fn_sql(*func, &format!("{}.{}", child_alias, quote_ident(column)));
            // SQL FILTER (WHERE ...) on the aggregate, from `filter`.
            let filter_conds: Vec<String> = sub
                .filter_pred
                .iter()
                .filter_map(|p| render_child_predicate(p, child_alias, child_resource, binds))
                .collect();
            let filter_sql = if filter_conds.is_empty() {
                String::new()
            } else {
                format!(" FILTER (WHERE {})", filter_conds.join(" AND "))
            };
            format!(
                "(SELECT {}{} FROM {} {} WHERE {})",
                fn_sql,
                filter_sql,
                quote_ident(&child_table),
                child_alias,
                where_sql
            )
        }
    }
}

/// Resolve the child resource + table a subselect reads from.
fn subselect_child<'a>(
    sub: &ComposeSubselect,
    feature: &'a Feature,
) -> (String, Option<&'a Resource>) {
    let resource_ref = match &sub.kind {
        SubselectKind::Count(r)
        | SubselectKind::Exists { resource: r, .. }
        | SubselectKind::Latest { resource: r, .. }
        | SubselectKind::Aggregate { resource: r, .. } => r,
    };
    let table = type_ref_table(resource_ref);
    let resource = match resource_ref {
        TypeRef::UserDefined(qname) => feature.resources.iter().find(|r| r.name == qname.name),
        _ => None,
    };
    (table, resource)
}

/// Map the closed 5-member aggregate-function catalog to its SQL function
/// call. `count_distinct` lowers to `COUNT(DISTINCT <col>)`.
fn agg_fn_sql(func: AggFn, col_expr: &str) -> String {
    match func {
        AggFn::Sum => format!("SUM({col_expr})"),
        AggFn::Avg => format!("AVG({col_expr})"),
        AggFn::Min => format!("MIN({col_expr})"),
        AggFn::Max => format!("MAX({col_expr})"),
        AggFn::CountDistinct => format!("COUNT(DISTINCT {col_expr})"),
    }
}

// ----------------------------------------------------------------------------
// Resolution helpers (shared with the emission half + the predicate module)
// ----------------------------------------------------------------------------

/// The in-feature root resource a compose reads from, when resolvable.
pub(super) fn root_resource<'a>(query: &ComposeQuery, feature: &'a Feature) -> Option<&'a Resource> {
    let TypeRef::UserDefined(qname) = &query.root else {
        return None;
    };
    feature.resources.iter().find(|r| r.name == qname.name)
}

/// The resource a join alias resolves to (the last hop's target), when
/// in-feature. Cross-feature joins return `None` (types fall back to string).
pub(super) fn joined_resource_for_alias<'a>(
    query: &ComposeQuery,
    feature: &'a Feature,
    alias: &str,
) -> Option<&'a Resource> {
    let join = query.joins.iter().find(|j| j.alias == alias)?;
    let hops: &[String] = join.path.segments.get(1..).unwrap_or(&[]);
    let mut current = root_resource(query, feature);
    for hop in hops {
        let (_, target) = resolve_fk_hop(current, hop, feature);
        current = target;
    }
    current
}

/// Escape a multi-line SQL body for embedding inside a Go `"..."` literal:
/// backslash + double-quote escaped, newlines as `\n`. Consumed by the
/// emission half (`compose/mod.rs`) when it writes the `SQLText:` field.
pub(super) fn escape_string_multiline(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

