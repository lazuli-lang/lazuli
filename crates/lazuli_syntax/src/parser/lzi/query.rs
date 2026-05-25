//! Feature-level `query.*` block parsers.
//!
//! Lazuli queries come in four canonical shapes, chosen by the
//! header keyword:
//!
//! | Header              | Body                                                            | IR shape         |
//! |---------------------|-----------------------------------------------------------------|------------------|
//! | `query.lookup <n>`  | `by <field>: <Type>` inline OR `params`/`filters` block         | `LookupQueryDecl`|
//! | `query.list <n>`    | `policy`, `params`, `filters`, `search`, `cache`, `order`, ...  | `ListQueryDecl`  |
//! | `query.sql <n>`     | `returns <Type>` + `sql "./<path>.sql"`                         | `SqlQueryDecl`   |
//! | `query.view <n>`    | `returns <Type>` + `source @file.<name>.sql`                    | `SqlQueryDecl`   |
//!
//! Every shape supports the closed catalog of cross-cutting children
//! that an LLM expects when authoring a read path:
//!
//! - `policy <expr>` — single source of truth for read authorization.
//! - `params <name>: <Type>` — inputs the caller supplies; share the
//!   inline-constraint catalog (min/max/in/pattern) with command
//!   inputs and resource fields via `extract_field_constraints`
//!   (`pub(super)` in `mod.rs`) and `split_command_input_modifiers`
//!   (`pub(super)` in `command.rs`).
//! - `scope` / `scope override` — tenant boundary; override is opt-in
//!   and demands a `reason "..."` clause.
//! - `filters`, `order`, `paginate`, `cache`, `search` — list-only
//!   ergonomics, gated by header keyword.
//!
//! The dispatch entry `parse_query_decl` is `pub(super)` so the
//! feature-skeleton walker in `mod.rs` keeps a single call site.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::command::split_command_input_modifiers;
use super::field_constraints::extract_field_constraints;

use crate::ast::{
    CommandInputSlot, ListQueryDecl, LookupKey, LookupQueryDecl, PolicyExprAst, QueryDecl,
    QuerySearch, Span, SqlQueryDecl, SqlQueryKind,
};

pub(super) fn parse_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("query.lookup ") {
        return parse_query_lookup_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.list ") {
        return parse_query_list_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.sql ") {
        return parse_query_sql_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.view ") {
        return parse_query_view_decl(lines, start, rest);
    }
    Err(line_error(
        header,
        "query header must be `query.list <name>`, `query.lookup <name> by ...`, `query.sql <name>`, or `query.view <name>`",
    ))
}

fn parse_query_lookup_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    // Two shapes accepted today:
    //   - inline: `<name> by <field>: <Type>` (Cut A canonical).
    //   - block: `<name>` with `params` / `filters` / `policy` children.
    let (name, inline_key) = if let Some((name, after)) = rest.split_once(" by ") {
        (
            name.trim().to_owned(),
            Some(parse_lookup_key(header, after)?),
        )
    } else {
        (rest.to_owned(), None)
    };
    if name.is_empty() {
        return Err(line_error(header, "`query.lookup` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    // `filters` lines are captured for cross-check but not lowered to
    // typed keys today; Cut A's contract is `keys` (from `by ...`) so
    // multi-key block lookups keep their filters in the AST sidecar
    // while IR uses `keys` from the inline form.
    let mut filters: Vec<String> = Vec::new();
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
                "`query.lookup` body children use one indentation level deeper than the header",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.lookup` children are `policy`, `params`, `filters`, or `gate behind/quota plan.*`",
            ));
        }
    }
    // Build the IR-facing keys list. Inline form contributes one
    // explicit key; block form synthesises a key per param so the IR
    // shape stays consistent.
    let keys: Vec<LookupKey> = if let Some(key) = inline_key {
        vec![key]
    } else {
        params
            .iter()
            .map(|p| LookupKey {
                name: p.name.clone(),
                type_text: p.type_text.clone(),
                span: p.span,
            })
            .collect()
    };
    Ok((
        QueryDecl::Lookup(LookupQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            keys,
            filters,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_lookup_key(line: &SourceLine<'_>, rest: &str) -> Result<LookupKey, ParseError> {
    let (name, type_text) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires `<field>: <Type>`",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a field name before `:`",
        ));
    }
    let type_text = type_text.trim();
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a type after `:`",
        ));
    }
    Ok(LookupKey {
        name: name.to_owned(),
        type_text: type_text.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn parse_query_list_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(header, "`query.list` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut modifier: Option<String> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_override = false;
    let mut scope_reason: Option<String> = None;
    let mut scope_assignments: Vec<String> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut filters: Vec<String> = Vec::new();
    let mut search: Option<QuerySearch> = None;
    let mut cache: Vec<String> = Vec::new();
    let mut cache_profile_ref: Option<String> = None;
    let mut paginate: Option<u32> = None;
    let mut order: Vec<String> = Vec::new();
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
                "`query.list` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("modifier ") {
            modifier = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope override" {
            scope_override = true;
            let (reason, assignments, next) =
                parse_query_scope_override_block(lines, i, grandchild_indent)?;
            scope_reason = reason;
            scope_assignments = assignments;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            let (parsed, next) = parse_query_search(lines, i, rest, grandchild_indent)?;
            search = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "cache" {
            // Inline shape: `cache` followed by indented `key`/`ttl`/...
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            cache = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("cache ") {
            // Cache bucket cycle (CL.C.3) — `cache <profile_name>` reference
            // form. Single-line shape pointing at a feature-level
            // `cache <name>` profile.
            let name = rest.trim();
            if name.is_empty() {
                return Err(line_error(
                    line,
                    "`cache <profile>` requires a profile name (declare it as a feature-level `cache <name>` block)",
                ));
            }
            if !cache.is_empty() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare `cache <profile>` only once",
                ));
            }
            cache_profile_ref = Some(name.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("paginate ") {
            paginate = Some(rest.trim().parse::<u32>().map_err(|_| {
                line_error(line, "`paginate` requires a positive integer page size")
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("order ") {
            order.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.list` children are `policy`, `modifier`, `params`, `scope`/`scope override`, `filters`, `search`, `cache`, `paginate`, `order`, or `gate behind/quota plan.*`",
            ));
        }
    }

    Ok((
        QueryDecl::List(ListQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            modifier,
            params,
            scope_override,
            scope_reason,
            scope_assignments,
            scope_lines,
            filters,
            search,
            cache,
            cache_profile_ref,
            paginate,
            order,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_query_sql_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::Sql)
}

fn parse_query_view_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::View)
}

fn parse_sql_backed_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    kind: SqlQueryKind,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a name",
                SqlQueryKind::View => "`query.view` requires a name",
            },
        ));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut returns: Option<String> = None;
    let mut sql_path: Option<String> = None;
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
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` body children use one indentation level deeper than the header"
                    }
                    SqlQueryKind::View => {
                        "`query.view` body children use one indentation level deeper than the header"
                    }
                },
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("sql ") {
            if kind == SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`query.view` uses `source @file.<name>.sql`; `sql \"./<path>.sql\"` is reserved for `query.sql`",
                ));
            }
            sql_path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            if kind != SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`source @file.<name>.sql` is only valid on `query.view`; use `sql \"./<path>.sql\"` on `query.sql`",
                ));
            }
            let source = rest.trim();
            if !source.starts_with("@file.") || !source.ends_with(".sql") {
                return Err(line_error(
                    line,
                    "`query.view source` must be shaped `@file.<name>.sql`",
                ));
            }
            sql_path = Some(source.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` children are `policy`, `params`, `scope`, `returns`, `sql`, or `gate behind/quota plan.*`"
                    }
                    SqlQueryKind::View => {
                        "`query.view` children are `policy`, `returns`, `source`, `params`, `scope`, or `gate behind/quota plan.*`"
                    }
                },
            ));
        }
    }

    let returns = returns.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `returns <Type>` declaration",
                SqlQueryKind::View => "`query.view` requires a `returns <Type>` declaration",
            },
        )
    })?;
    let sql_path = sql_path.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `sql \"./<path>.sql\"` declaration",
                SqlQueryKind::View => {
                    "`query.view` requires a `source @file.<name>.sql` declaration"
                }
            },
        )
    })?;
    Ok((
        QueryDecl::Sql(SqlQueryDecl {
            name,
            kind,
            public_contract: None,
            policy,
            policy_expr,
            params,
            scope_lines,
            returns,
            sql_path,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_query_params_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Vec<CommandInputSlot>, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
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
                "query `params` children use the deepest indentation level",
            ));
        }
        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "query `params` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "query `params` slot requires a name before `:`",
            ));
        }
        // L0 #3 §10 — query params share the inline-constraint catalog
        // with command inputs / resource fields.
        let (after_constraints, constraints) = extract_field_constraints(line, type_part.trim())?;
        let (type_text, required, optional) =
            split_command_input_modifiers(after_constraints.trim());
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((slots, i))
}

fn parse_query_scope_override_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Option<String>, Vec<String>, usize), ParseError> {
    let mut reason: Option<String> = None;
    let mut assignments: Vec<String> = Vec::new();
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
                "`scope override` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("reason ") {
            reason = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            assignments.push(trimmed.to_owned());
        }
        i += 1;
    }
    Ok((reason, assignments, i))
}

fn parse_query_indented_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> (Vec<String>, usize) {
    let mut out: Vec<String> = Vec::new();
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
        out.push(trimmed.to_owned());
        i += 1;
    }
    (out, i)
}

fn parse_query_search(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    grandchild_indent: usize,
) -> Result<(QuerySearch, usize), ParseError> {
    let header = &lines[start];
    let (source, fields) = rest.split_once(" over ").ok_or_else(|| {
        line_error(
            header,
            "`search` requires `<path> over <field>, <field>` (e.g. `search params.search over name, email`)",
        )
    })?;
    let fields: Vec<String> = fields
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let mut mode: Option<String> = None;
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
                "`search` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("mode ") {
            mode = Some(rest.trim().to_owned());
            i += 1;
        } else {
            return Err(line_error(line, "`search` children are `mode <kind>` only"));
        }
    }
    Ok((
        QuerySearch {
            source: source.trim().to_owned(),
            fields,
            mode,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}
