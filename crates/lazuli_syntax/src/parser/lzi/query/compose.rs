//! `query.compose` — declarative composite read (root resource + FK-path
//! JOIN projection + a CLOSED 4-member sub-select catalog).
//!
//! This is the fourth `query.<mode>` sibling alongside `list`/`lookup`/
//! `sql` (proposal `ir-composite-read-primitive-2026-05-29.md`). W1 ships
//! the parser + AST; lowering / codegen / doctor are later cells.
//!
//! ## Surface (proposal §3.1 EBNF)
//!
//! ```text
//! query.compose <name>
//!   from <Resource>                      # required, exactly 1
//!   params ...
//!   join <fk.path> [as <alias>] [optional]   # 0..N
//!   subselect <n> = <kind> ...               # 0..N, closed catalog
//!   select                                   # required, >=1 projection
//!     <field> = self.<col> | <alias>.<col> | <subselect>
//!   filters ...
//!   key <path> = <expr>                  # presence => single-row
//!   scope ... | scope override ...
//!   order <field> <dir>
//!   paginate <N>
//!   policy @policy.<name>
//!   returns <Type>                       # default <Compose>Row
//! ```
//!
//! ## Closure levers the PARSER enforces
//!
//! * `subselect` kinds are exactly `count` / `exists` / `latest` /
//!   `aggregate`; `aggregate` functions are exactly `sum` / `avg` /
//!   `min` / `max` / `count_distinct`. Anything else is a parse error.
//! * sub-select `where` / `filter` predicates use the closed operator set
//!   `=` / `!=` / `has` plus `in` **with a literal-set RHS only**.
//!   `in ( subselect )`, `in params.x`, and `in <expr>` are NOT
//!   productions — the parser rejects them here, before doctor runs
//!   (the single guard that stops `in [...]` from becoming a
//!   correlated-subquery backdoor, §3.1).

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::super::lzx::try_parse_policy_expr;
use super::blocks::{parse_query_params_block, parse_query_scope_override_block};
use crate::ast::{
    CommandInputSlot, ComposeAggFnDecl, ComposeJoinDecl, ComposePredCombinator,
    ComposeProjectionDecl, ComposeProjectionSourceDecl, ComposeQueryDecl, ComposeSubselectDecl,
    ComposeSubselectKindDecl, ComposeSubselectPred, ComposeSubselectPredOp, PolicyExprAst,
    QueryDecl, Span,
};

/// Accumulator for a `query.compose` body so the long `while` loop stays
/// flat. Lifted into [`ComposeQueryDecl`] at the end.
#[derive(Default)]
struct ComposeAcc {
    policy: Option<String>,
    policy_expr: Option<PolicyExprAst>,
    root: Option<String>,
    params: Vec<CommandInputSlot>,
    joins: Vec<ComposeJoinDecl>,
    projections: Vec<ComposeProjectionDecl>,
    subselects: Vec<ComposeSubselectDecl>,
    filters: Vec<String>,
    key: Option<String>,
    scope_lines: Vec<String>,
    scope_override: bool,
    scope_reason: Option<String>,
    scope_assignments: Vec<String>,
    order: Vec<String>,
    paginate: Option<u32>,
    returns: Option<String>,
}

pub(super) fn parse_query_compose_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(header, "`query.compose` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut acc = ComposeAcc::default();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`query.compose` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("from ") {
            let root = rest.trim();
            if root.is_empty() {
                return Err(line_error(line, "`query.compose from` requires a resource name"));
            }
            if acc.root.is_some() {
                return Err(line_error(
                    line,
                    "`query.compose` declares exactly one `from <Resource>` root (multi-root reads use `query.sql`)",
                ));
            }
            acc.root = Some(root.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            acc.policy = Some(rest.trim().to_owned());
            acc.policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            acc.params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("join ") {
            acc.joins.push(parse_compose_join(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed == "select" {
            let (projections, next) = parse_compose_select_block(lines, i, grandchild_indent)?;
            acc.projections = projections;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("subselect ") {
            let (subselect, next) =
                parse_compose_subselect(lines, i, rest, grandchild_indent)?;
            acc.subselects.push(subselect);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (collected, next) =
                super::blocks::parse_query_indented_block(lines, i, grandchild_indent);
            acc.filters = collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("key ") {
            let body = rest.trim();
            if body.is_empty() {
                return Err(line_error(line, "`query.compose key` requires `<path> = <expr>`"));
            }
            acc.key = Some(body.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "scope override" {
            acc.scope_override = true;
            let (reason, assignments, next) =
                parse_query_scope_override_block(lines, i, grandchild_indent)?;
            acc.scope_reason = reason;
            acc.scope_assignments = assignments;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (collected, next) =
                super::blocks::parse_query_indented_block(lines, i, grandchild_indent);
            acc.scope_lines = collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("order ") {
            acc.order.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("paginate ") {
            acc.paginate = Some(rest.trim().parse::<u32>().map_err(|_| {
                line_error(line, "`paginate` requires a positive integer page size")
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            acc.returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass (mirrors siblings).
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.compose` children are `from`, `params`, `join`, `select`, `subselect`, `filters`, `key`, `scope`/`scope override`, `order`, `paginate`, `policy`, or `returns`",
            ));
        }
    }

    let ComposeAcc {
        policy,
        policy_expr,
        root,
        params,
        joins,
        projections,
        subselects,
        filters,
        key,
        scope_lines,
        scope_override,
        scope_reason,
        scope_assignments,
        order,
        paginate,
        returns,
    } = acc;

    let root = root.ok_or_else(|| {
        line_error(header, "`query.compose` requires a `from <Resource>` root")
    })?;
    if projections.is_empty() {
        return Err(line_error(
            header,
            "`query.compose` requires a `select` block with at least one projection",
        ));
    }

    Ok((
        QueryDecl::Compose(ComposeQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            root,
            params,
            joins,
            projections,
            subselects,
            filters,
            key,
            scope_lines,
            scope_override,
            scope_reason,
            scope_assignments,
            order,
            paginate,
            returns,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

/// `join <fk.path> [as <alias>] [optional]`.
fn parse_compose_join(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ComposeJoinDecl, ParseError> {
    let mut tokens = rest.split_whitespace();
    let path = tokens
        .next()
        .ok_or_else(|| line_error(line, "`join` requires an FK path (e.g. `join host.user`)"))?;
    if !is_fk_path(path) {
        return Err(line_error(
            line,
            "`join` target must be an FK path of lowercase segments (e.g. `service_transaction.property`), never an `ON`-clause",
        ));
    }
    let mut alias: Option<String> = None;
    let mut nullable = false;
    while let Some(tok) = tokens.next() {
        match tok {
            "as" => {
                let a = tokens.next().ok_or_else(|| {
                    line_error(line, "`join ... as` requires an alias identifier")
                })?;
                alias = Some(a.to_owned());
            }
            "optional" => nullable = true,
            other => {
                return Err(line_error_owned_join(line, other));
            }
        }
    }
    Ok(ComposeJoinDecl {
        path: path.to_owned(),
        alias,
        nullable,
        span: Span::new(line.start, line.end),
    })
}

fn line_error_owned_join(line: &SourceLine<'_>, _tok: &str) -> ParseError {
    line_error(
        line,
        "`join <fk.path>` accepts only `as <alias>` and `optional` modifiers",
    )
}

/// `select` block — one `<name> = <source>` projection per line.
fn parse_compose_select_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Vec<ComposeProjectionDecl>, usize), ParseError> {
    let mut out: Vec<ComposeProjectionDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`query.compose select` children use the deepest indentation level",
            ));
        }
        let (name_part, source_part) = trimmed.split_once('=').ok_or_else(|| {
            line_error(
                line,
                "`select` projections use `<name> = self.<col> | <alias>.<col> | <subselect>`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(line, "`select` projection requires a name before `=`"));
        }
        let source = parse_projection_source(line, source_part.trim())?;
        out.push(ComposeProjectionDecl {
            name: name.to_owned(),
            source,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((out, i))
}

/// `self.<col>` | `<alias>.<col>` | `<subselect>` (bare ident).
fn parse_projection_source(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<ComposeProjectionSourceDecl, ParseError> {
    if text.is_empty() {
        return Err(line_error(line, "`select` projection requires a source after `=`"));
    }
    if let Some((head, tail)) = text.split_once('.') {
        let head = head.trim();
        let col = tail.trim();
        if head.is_empty() || col.is_empty() || col.contains('.') {
            return Err(line_error(
                line,
                "`select` projection source must be `self.<col>`, `<alias>.<col>`, or a `<subselect>` name",
            ));
        }
        if head == "self" {
            return Ok(ComposeProjectionSourceDecl::SelfColumn(col.to_owned()));
        }
        return Ok(ComposeProjectionSourceDecl::Joined {
            alias: head.to_owned(),
            column: col.to_owned(),
        });
    }
    // Bare identifier → reference to a declared subselect.
    Ok(ComposeProjectionSourceDecl::Subselect(text.to_owned()))
}

/// `subselect <name> = <kind>` + indented child clauses.
fn parse_compose_subselect(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    grandchild_indent: usize,
) -> Result<(ComposeSubselectDecl, usize), ParseError> {
    let header = &lines[start];
    let (name_part, kind_part) = rest.split_once('=').ok_or_else(|| {
        line_error(header, "`subselect <name> = <kind>` requires `= <kind>`")
    })?;
    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(header, "`subselect` requires a name before `=`"));
    }
    let kind = parse_subselect_kind(header, kind_part.trim())?;

    let mut related_by: Option<String> = None;
    let mut where_pred: Vec<ComposeSubselectPred> = Vec::new();
    let mut filter_pred: Vec<ComposeSubselectPred> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut negate = false;

    // The `subselect` header line sits at the body-child level
    // (`grandchild_indent - 2`); its own children are one level deeper —
    // exactly `grandchild_indent`.
    let subselect_child_indent = grandchild_indent;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < subselect_child_indent {
            break;
        }
        if line.indent != subselect_child_indent {
            return Err(line_error(
                line,
                "`subselect` children use one indentation level deeper than the `subselect` line",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("related_by ") {
            let path = rest.trim();
            if !is_fk_path(path) {
                return Err(line_error(
                    line,
                    "`subselect related_by` requires an FK path of lowercase segments (e.g. `review.transaction`)",
                ));
            }
            related_by = Some(path.to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("where ") {
            where_pred = parse_subselect_pred_list(line, rest)?;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("filter ") {
            filter_pred = parse_subselect_pred_list(line, rest)?;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("order ") {
            order.push(rest.trim().to_owned());
            i += 1;
        } else if trimmed == "negate" {
            negate = true;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`subselect` children are `related_by`, `where`, `filter`, `order`, or `negate`",
            ));
        }
    }

    Ok((
        ComposeSubselectDecl {
            name: name.to_owned(),
            kind,
            related_by,
            where_pred,
            filter_pred,
            order,
            negate,
            span: Span::new(header.start, lines[i.saturating_sub(1).max(start)].end),
        },
        i,
    ))
}

/// The CLOSED 4-member kind catalog: `count <R>` / `exists <R>` /
/// `latest <col> of <R>` / `aggregate <fn> <col> of <R>`.
fn parse_subselect_kind(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<ComposeSubselectKindDecl, ParseError> {
    if let Some(rest) = text.strip_prefix("count ") {
        let resource = rest.trim();
        require_type_ident(line, resource, "count")?;
        return Ok(ComposeSubselectKindDecl::Count {
            resource: resource.to_owned(),
        });
    }
    if let Some(rest) = text.strip_prefix("exists ") {
        let resource = rest.trim();
        require_type_ident(line, resource, "exists")?;
        return Ok(ComposeSubselectKindDecl::Exists {
            resource: resource.to_owned(),
        });
    }
    if let Some(rest) = text.strip_prefix("latest ") {
        let (column, resource) = split_col_of(line, rest, "latest")?;
        return Ok(ComposeSubselectKindDecl::Latest { column, resource });
    }
    if let Some(rest) = text.strip_prefix("aggregate ") {
        let mut tokens = rest.trim().splitn(2, char::is_whitespace);
        let fn_tok = tokens.next().unwrap_or("").trim();
        let func = parse_agg_fn(line, fn_tok)?;
        let remainder = tokens.next().unwrap_or("").trim();
        let (column, resource) = split_col_of(line, remainder, "aggregate")?;
        return Ok(ComposeSubselectKindDecl::Aggregate {
            func,
            column,
            resource,
        });
    }
    Err(line_error(
        line,
        "`subselect` kind must be one of the closed catalog: `count <R>`, `exists <R>`, `latest <col> of <R>`, `aggregate <fn> <col> of <R>`",
    ))
}

/// The CLOSED 5-member aggregate function set.
fn parse_agg_fn(line: &SourceLine<'_>, tok: &str) -> Result<ComposeAggFnDecl, ParseError> {
    match tok {
        "sum" => Ok(ComposeAggFnDecl::Sum),
        "avg" => Ok(ComposeAggFnDecl::Avg),
        "min" => Ok(ComposeAggFnDecl::Min),
        "max" => Ok(ComposeAggFnDecl::Max),
        "count_distinct" => Ok(ComposeAggFnDecl::CountDistinct),
        _ => Err(line_error(
            line,
            "`aggregate` function must be one of `sum`, `avg`, `min`, `max`, `count_distinct`",
        )),
    }
}

/// Split `<col> of <Resource>` (used by `latest` and `aggregate`).
fn split_col_of(
    line: &SourceLine<'_>,
    text: &str,
    kw: &str,
) -> Result<(String, String), ParseError> {
    let (col, resource) = text.split_once(" of ").ok_or_else(|| {
        line_error(
            line,
            match kw {
                "latest" => "`latest <column> of <Resource>` requires `of <Resource>`",
                _ => "`aggregate <fn> <column> of <Resource>` requires `of <Resource>`",
            },
        )
    })?;
    let col = col.trim();
    let resource = resource.trim();
    if col.is_empty() {
        return Err(line_error(line, "subselect requires a column name before `of`"));
    }
    require_type_ident(line, resource, kw)?;
    Ok((col.to_owned(), resource.to_owned()))
}

/// The narrowed `subselect_pred_list` (§3.1). The ONLY set-membership form
/// is `in [<literal>, ...]`; `in ( subselect )`, `in params.x`, and
/// `in <expr>` are rejected HERE so the closed surface cannot reopen.
fn parse_subselect_pred_list(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<Vec<ComposeSubselectPred>, ParseError> {
    let mut out: Vec<ComposeSubselectPred> = Vec::new();
    // Split on top-level AND / OR (no nesting inside the closed predicate
    // language other than the `[...]` literal-set, which contains no
    // AND/OR). The split is whitespace-delimited keyword aware.
    for (idx, raw) in split_pred_atoms(text).into_iter().enumerate() {
        let (combinator, atom) = raw;
        let combinator = if idx == 0 {
            if combinator.is_some() {
                return Err(line_error(
                    line,
                    "`subselect` predicate cannot start with `AND`/`OR`",
                ));
            }
            None
        } else {
            Some(combinator.ok_or_else(|| {
                line_error(line, "`subselect` predicates must be joined by `AND`/`OR`")
            })?)
        };
        out.push(parse_subselect_pred(line, combinator, atom.trim())?);
    }
    if out.is_empty() {
        return Err(line_error(line, "`subselect` `where`/`filter` requires a predicate"));
    }
    Ok(out)
}

/// Split a predicate body into `(combinator, atom)` pairs on top-level
/// ` AND ` / ` OR ` (uppercase, whitespace-bounded). Brackets `[...]` are
/// treated atomically so a literal-set never gets split.
fn split_pred_atoms(text: &str) -> Vec<(Option<ComposePredCombinator>, String)> {
    let mut out: Vec<(Option<ComposePredCombinator>, String)> = Vec::new();
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut atom_start = 0usize;
    let mut pending: Option<ComposePredCombinator> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth -= 1;
                i += 1;
            }
            b' ' if depth == 0 => {
                // Look for the keyword ` AND ` / ` OR ` starting here.
                if let Some(kw_len) = match_combinator(&text[i..]) {
                    let combinator = if text[i..].trim_start().starts_with("AND") {
                        ComposePredCombinator::And
                    } else {
                        ComposePredCombinator::Or
                    };
                    out.push((pending.take(), text[atom_start..i].to_string()));
                    pending = Some(combinator);
                    i += kw_len;
                    atom_start = i;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out.push((pending.take(), text[atom_start..].to_string()));
    out
}

/// If `s` begins with ` AND ` / ` OR ` (leading space already consumed by
/// the caller's match on `b' '`), return the byte length to skip past the
/// keyword (including the trailing space). `None` otherwise.
fn match_combinator(s: &str) -> Option<usize> {
    // s starts at a space. Skip it, check the keyword + a trailing space.
    let after = &s[1..];
    if let Some(rest) = after.strip_prefix("AND ") {
        let _ = rest;
        return Some(1 + "AND ".len());
    }
    if let Some(rest) = after.strip_prefix("OR ") {
        let _ = rest;
        return Some(1 + "OR ".len());
    }
    None
}

/// One predicate atom: `<ref> = <rhs>` / `<ref> != <rhs>` /
/// `<ref> has <rhs>` / `<ref> in [<literal>, ...]`.
fn parse_subselect_pred(
    line: &SourceLine<'_>,
    combinator: Option<ComposePredCombinator>,
    atom: &str,
) -> Result<ComposeSubselectPred, ParseError> {
    // `in` literal-set form first (so `!=`/`=` scans don't eat it).
    if let Some((lhs, rhs)) = split_keyword(atom, "in") {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        let set = parse_literal_set(line, rhs)?;
        require_scalar_ref(line, lhs)?;
        return Ok(ComposeSubselectPred {
            combinator,
            left: lhs.to_owned(),
            op: ComposeSubselectPredOp::In(set),
            span: Span::new(line.start, line.end),
        });
    }
    if let Some((lhs, rhs)) = atom.split_once("!=") {
        return finish_scalar_pred(line, combinator, lhs, rhs, |s| {
            ComposeSubselectPredOp::Ne(s)
        });
    }
    if let Some((lhs, rhs)) = split_keyword(atom, "has") {
        return finish_scalar_pred(line, combinator, &lhs, &rhs, |s| {
            ComposeSubselectPredOp::Has(s)
        });
    }
    if let Some((lhs, rhs)) = atom.split_once('=') {
        return finish_scalar_pred(line, combinator, lhs, rhs, |s| {
            ComposeSubselectPredOp::Eq(s)
        });
    }
    Err(line_error(
        line,
        "`subselect` predicate must be `<ref> = <rhs>`, `<ref> != <rhs>`, `<ref> has <rhs>`, or `<ref> in [<literal>, ...]` (no `<`/`>`, no functions, no `in (subselect)` / `in params.x`)",
    ))
}

fn finish_scalar_pred(
    line: &SourceLine<'_>,
    combinator: Option<ComposePredCombinator>,
    lhs: &str,
    rhs: &str,
    make: impl Fn(String) -> ComposeSubselectPredOp,
) -> Result<ComposeSubselectPred, ParseError> {
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.is_empty() || rhs.is_empty() {
        return Err(line_error(line, "`subselect` predicate requires both sides"));
    }
    require_scalar_ref(line, lhs)?;
    Ok(ComposeSubselectPred {
        combinator,
        left: lhs.to_owned(),
        op: make(rhs.to_owned()),
        span: Span::new(line.start, line.end),
    })
}

/// Parse a bracketed literal set `[a, b, c]`. Rejects the empty set and any
/// nested bracket / parenthesis (which would signal a subselect form).
fn parse_literal_set(line: &SourceLine<'_>, text: &str) -> Result<Vec<String>, ParseError> {
    let inner = text
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            line_error(
                line,
                "`in` admits ONLY a literal-set RHS `in [<literal>, ...]` — `in (subselect)`, `in params.x`, and `in <expr>` are not allowed",
            )
        })?;
    if inner.contains('(') || inner.contains('[') {
        return Err(line_error(
            line,
            "`in [...]` literal set cannot contain nested brackets or a subselect",
        ));
    }
    let items: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        return Err(line_error(line, "`in [...]` requires at least one literal"));
    }
    Ok(items)
}

/// A scalar reference admitted on a predicate LHS/RHS: `self.<col>`,
/// `<alias>.<col>`, or `ctx.<seg>...`. Bare literals are also accepted on
/// the RHS by the caller; this guard is for the LHS column reference and
/// re-asserts that we never accepted a `params.x` / subselect form there.
fn require_scalar_ref(line: &SourceLine<'_>, text: &str) -> Result<(), ParseError> {
    if text.contains('(') {
        return Err(line_error(
            line,
            "`subselect` predicate reference cannot be a function/subselect call",
        ));
    }
    if text.starts_with("params.") {
        return Err(line_error(
            line,
            "`subselect` predicate cannot reference `params.*` (would reopen the closed surface to a dynamic set)",
        ));
    }
    if text.is_empty() {
        return Err(line_error(line, "`subselect` predicate reference is empty"));
    }
    Ok(())
}

/// Split on a whitespace-bounded keyword operator (`has` / `in`), honoring
/// the requirement that the keyword be surrounded by spaces so it doesn't
/// match inside an identifier (`min` would not match `in`).
fn split_keyword(text: &str, kw: &str) -> Option<(String, String)> {
    let needle = format!(" {kw} ");
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        if depth == 0 && text[i..].starts_with(&needle) {
            let lhs = text[..i].to_string();
            let rhs = text[i + needle.len()..].to_string();
            return Some((lhs, rhs));
        }
        i += 1;
    }
    None
}

/// An FK path is one-or-more lowercase identifier segments joined by `.`.
fn is_fk_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && seg.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        })
}

/// A resource/type name must be a non-empty UpperCamel-ish identifier.
fn require_type_ident(line: &SourceLine<'_>, ident: &str, kw: &str) -> Result<(), ParseError> {
    if ident.is_empty() || ident.contains(char::is_whitespace) {
        return Err(line_error_owned_kind(line, kw));
    }
    Ok(())
}

fn line_error_owned_kind(line: &SourceLine<'_>, _kw: &str) -> ParseError {
    line_error(
        line,
        "`subselect` kind requires a single resource type name (e.g. `ServiceTransaction`)",
    )
}
