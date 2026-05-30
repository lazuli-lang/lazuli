//! Cell W5 — `query.compose` predicate + operand rendering and the low-level
//! SQL string helpers shared across the generator. Split out of `sql/mod.rs`
//! (Rails-style ≤500 LOC layout) so the SELECT assembler and the predicate
//! sublanguage stay separately legible.
//!
//! The renderers thread the shared [`super::ComposeBind`] plan so `params.*` /
//! `ctx.*` operands allocate positional `$N` binds in SQL text order — the
//! `ctx.*` actor axis is therefore BOUND from ctx (via `lazuli.CtxValue`),
//! never inlined as a literal a model could drop.

use lazuli_ir::{CompareOp, Expr, OrderBy, OrderDir, Predicate, Resource, TypeRef};

use super::ComposeBind;

/// Render the `key` clause as a root predicate (`root.<col> = $N`). The RHS is
/// a `params.<name>` ref that becomes a positional bind.
pub(super) fn render_key_predicate(
    key: &lazuli_ir::KeyClause,
    binds: &mut Vec<ComposeBind>,
) -> Option<String> {
    let col = column_segments(&key.path.segments);
    let rhs = render_rhs(&key.equals, binds)?;
    Some(format!("root.{} = {}", quote_ident(&col), rhs))
}

/// Render a root-scoped author predicate (`filters`). Only equality/inequality
/// is rendered on the root WHERE; the column side resolves to `root.<col>`, the
/// value side to a literal or a `params.<name>`/`ctx.<path>` bind.
pub(super) fn render_root_predicate(
    pred: &Predicate,
    binds: &mut Vec<ComposeBind>,
) -> Option<String> {
    let Predicate::Comparison { left, op, right } = pred else {
        return None;
    };
    let op_sql = compare_op_sql(*op)?;
    // Decide which side is the column. A path that is NOT a source path
    // (params/ctx/…) is the column side.
    let (col, value) = match (path_column(left), path_column(right)) {
        (Some(c), _) => (c, right),
        (None, Some(c)) => (c, left),
        _ => return None,
    };
    // `<col> = nil` / `<col> != nil` → IS [NOT] NULL.
    if matches!(value, Expr::Nil) {
        let null_op = if matches!(op, CompareOp::Ne) {
            "IS NOT NULL"
        } else {
            "IS NULL"
        };
        return Some(format!("root.{} {}", quote_ident(&col), null_op));
    }
    let rhs = render_rhs(value, binds)?;
    Some(format!("root.{} {} {}", quote_ident(&col), op_sql, rhs))
}

/// Render a child-scoped predicate inside a subselect `where`/`filter`. The
/// column side resolves against the child alias; `ctx.*` operands allocate a
/// positional bind (resolved from ctx at runtime, not inlined), `self.<col>`
/// resolves to the child column, scalar literals render verbatim. Only the
/// closed equality/inequality + AND/OR shapes are rendered (`<`/`>` skipped —
/// doctor rejects them at W3). `nil` RHS lowers to `IS NULL` / `IS NOT NULL`.
pub(super) fn render_child_predicate(
    pred: &Predicate,
    child_alias: &str,
    child_resource: Option<&Resource>,
    binds: &mut Vec<ComposeBind>,
) -> Option<String> {
    match pred {
        Predicate::Comparison { left, op, right } => {
            let op_sql = compare_op_sql(*op)?;
            // `<col> = nil` / `<col> != nil` → IS [NOT] NULL (canonical SQL).
            if matches!(right, Expr::Nil) {
                let lhs = render_child_operand(left, child_alias, child_resource, binds)?;
                let null_op = if matches!(op, CompareOp::Ne) {
                    "IS NOT NULL"
                } else {
                    "IS NULL"
                };
                return Some(format!("{lhs} {null_op}"));
            }
            let lhs = render_child_operand(left, child_alias, child_resource, binds)?;
            let rhs = render_child_operand(right, child_alias, child_resource, binds)?;
            Some(format!("{lhs} {op_sql} {rhs}"))
        }
        Predicate::And(parts) => {
            let rendered: Vec<String> = parts
                .iter()
                .filter_map(|p| render_child_predicate(p, child_alias, child_resource, binds))
                .collect();
            if rendered.is_empty() {
                None
            } else {
                Some(format!("({})", rendered.join(" AND ")))
            }
        }
        Predicate::Or(parts) => {
            let rendered: Vec<String> = parts
                .iter()
                .filter_map(|p| render_child_predicate(p, child_alias, child_resource, binds))
                .collect();
            if rendered.is_empty() {
                None
            } else {
                Some(format!("({})", rendered.join(" OR ")))
            }
        }
        Predicate::Has { .. } => None,
    }
}

/// Render one operand inside a subselect predicate. A bare/`self.` column maps
/// to `<child_alias>.<col>`; a `ctx.<path>` ref allocates a positional bind
/// (resolved from ctx via `lazuli.CtxValue` in the generated `SQLArgsCtx`), so
/// `sender != ctx.user.id` becomes `c.sender != $N` — bound, not inlined.
/// `<alias>.<col>` is a joined-node reference; scalar literals render verbatim.
fn render_child_operand(
    expr: &Expr,
    child_alias: &str,
    _child_resource: Option<&Resource>,
    binds: &mut Vec<ComposeBind>,
) -> Option<String> {
    match expr {
        Expr::Path(path) => {
            let head = path.segments.first().map(String::as_str).unwrap_or("");
            match head {
                // `self.<col>` / bare `<col>` → child column.
                "self" => Some(format!(
                    "{}.{}",
                    child_alias,
                    quote_ident(&column_segments(&path.segments))
                )),
                // `ctx.<path>` — the actor axis. Allocate a positional bind in
                // text order; the generated SQLArgsCtx resolves it from ctx.
                "ctx" => {
                    let n = next_bind(binds, ComposeBind::Ctx(path.segments[1..].join(".")));
                    Some(format!("${n}"))
                }
                // `params.<name>` inside a subselect predicate → positional bind.
                "params" | "input" | "route" => {
                    let name = path.segments.get(1)?.clone();
                    let n = next_bind(binds, ComposeBind::Param(name));
                    Some(format!("${n}"))
                }
                // `<alias>.<col>` — a joined node reference.
                _ if path.segments.len() == 2 => Some(format!(
                    "{}.{}",
                    quote_ident(&path.segments[0]),
                    quote_ident(&path.segments[1])
                )),
                _ => Some(format!(
                    "{}.{}",
                    child_alias,
                    quote_ident(&column_segments(&path.segments))
                )),
            }
        }
        Expr::String(s) => Some(format!("'{}'", escape_sql_literal(s))),
        Expr::Integer(n) => Some(n.to_string()),
        Expr::Boolean(b) => Some(if *b { "TRUE".to_owned() } else { "FALSE".to_owned() }),
        Expr::Nil => Some("NULL".to_owned()),
        Expr::Enum(literal) => Some(format!("'{}'", escape_sql_literal(&literal.variant))),
        Expr::FnCall(_) => None,
    }
}

/// Render the RHS of a root predicate: a `params.<name>` / `ctx.<path>` ref
/// becomes a positional `$N` bind; a literal renders inline.
fn render_rhs(expr: &Expr, binds: &mut Vec<ComposeBind>) -> Option<String> {
    match expr {
        Expr::Path(path) => {
            let head = path.segments.first().map(String::as_str).unwrap_or("");
            match head {
                "params" | "input" | "route" => {
                    let name = path.segments.get(1)?.clone();
                    let n = next_bind(binds, ComposeBind::Param(name));
                    Some(format!("${n}"))
                }
                // `ctx.<path>` on the root WHERE — allocate a positional bind
                // resolved from ctx via `lazuli.CtxValue` in `SQLArgsCtx`. The
                // actor axis is therefore bound, never inlined.
                "ctx" => {
                    let n = next_bind(binds, ComposeBind::Ctx(path.segments[1..].join(".")));
                    Some(format!("${n}"))
                }
                _ => {
                    // Bare path on the RHS — treat as a param by its head name.
                    let n = next_bind(binds, ComposeBind::Param(head.to_owned()));
                    Some(format!("${n}"))
                }
            }
        }
        Expr::String(s) => Some(format!("'{}'", escape_sql_literal(s))),
        Expr::Integer(n) => Some(n.to_string()),
        Expr::Boolean(b) => Some(if *b { "TRUE".to_owned() } else { "FALSE".to_owned() }),
        Expr::Nil => Some("NULL".to_owned()),
        Expr::Enum(literal) => Some(format!("'{}'", escape_sql_literal(&literal.variant))),
        Expr::FnCall(_) => None,
    }
}

/// Render `ORDER BY` from the IR order clauses. Empty ⇒ no clause (the
/// runtime's `query.view` path does not impose a default order).
pub(super) fn render_order(order: &[OrderBy]) -> Option<String> {
    if order.is_empty() {
        return None;
    }
    let parts: Vec<String> = order
        .iter()
        .map(|o| {
            let dir = if matches!(o.direction, OrderDir::Desc) {
                "DESC"
            } else {
                "ASC"
            };
            format!("{} {}", quote_ident(&o.field), dir)
        })
        .collect();
    Some(format!("ORDER BY {}", parts.join(", ")))
}

/// Push a bind onto the plan and return its 1-based `$N` index.
pub(super) fn next_bind(binds: &mut Vec<ComposeBind>, bind: ComposeBind) -> usize {
    binds.push(bind);
    binds.len()
}

/// SQL operator for the closed comparison catalog. Only `=`/`!=` are admitted
/// on the compose surface (`<`/`>` stay in `query.sql`, §3.2 #3); ordered ops
/// return `None` so the predicate is skipped (doctor rejects them at W3).
fn compare_op_sql(op: CompareOp) -> Option<&'static str> {
    match op {
        CompareOp::Eq => Some("="),
        CompareOp::Ne => Some("!="),
        CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge => None,
    }
}

// ----------------------------------------------------------------------------
// Low-level SQL string + column helpers (shared with `sql/mod.rs`)
// ----------------------------------------------------------------------------

/// Lower a path's segments to a single column name, dropping a leading
/// `self` and joining `<a>.<b>` FK-id forms (`user.id` → `user_id`).
pub(super) fn column_segments(segments: &[String]) -> String {
    let trimmed = if segments.first().map(String::as_str) == Some("self") {
        &segments[1..]
    } else {
        segments
    };
    match trimmed {
        [] => String::new(),
        [one] => one.to_ascii_lowercase(),
        [head, tail] if tail == "id" => format!("{}_id", head.to_ascii_lowercase()),
        many => many
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
    }
}

/// Return the column name for a path expression that is NOT a source path
/// (`params`/`ctx`/`input`/`route`). Source paths return `None` so the
/// predicate renderer knows which side is the column.
fn path_column(expr: &Expr) -> Option<String> {
    let Expr::Path(path) = expr else {
        return None;
    };
    let head = path.segments.first().map(String::as_str).unwrap_or("");
    if matches!(head, "params" | "input" | "ctx" | "route" | "target") {
        return None;
    }
    Some(column_segments(&path.segments))
}

/// Snake-cased table name for a `TypeRef` (`UserDefined(ServiceTransaction)`
/// → `service_transaction`). Builtins fall back to a lowercased name.
pub(super) fn type_ref_table(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => {
            super::super::super::util::pascal_to_snake(&qname.name)
        }
        TypeRef::Many(inner) => type_ref_table(inner),
        TypeRef::Unresolved(name) => super::super::super::util::pascal_to_snake(name),
        _ => "unknown".to_owned(),
    }
}

/// Double-quote a SQL identifier (`quoteIdent` parity on the runtime side).
/// Generated code controls all identifiers; quoting guards reserved words.
pub(super) fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Escape a SQL string literal (single-quote doubling).
pub(super) fn escape_sql_literal(raw: &str) -> String {
    raw.replace('\'', "''")
}

/// Indent + join select items so the embedded SELECT reads as a block.
pub(super) fn indent_join(items: &[String], sep: &str) -> String {
    items
        .iter()
        .map(|item| format!("  {item}"))
        .collect::<Vec<_>>()
        .join(sep)
}
