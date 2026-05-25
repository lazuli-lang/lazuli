//! `aggregate <Name>` block + the shared `invariant <name>` parser.
//!
//! `aggregate` is a feature-level sibling of `resource` / `command`.
//! It declares a transactional consistency boundary by naming a root
//! resource and the resources it contains, plus the invariants those
//! resources must preserve as a group (CL.C.4):
//!
//! ```text
//! aggregate Order
//!   root Order
//!   contains OrderLine, Payment
//!   invariants
//!     invariant total_consistent
//!       when sum(lines.amount) == total
//!       message "Order total must match line items"
//! ```
//!
//! `invariant <name>` blocks also appear directly inside a `resource`
//! body — the same parser handles both. Closed body:
//!
//! - `when <predicate>` (required, exactly once).
//! - `message "<text>"` (optional, exactly once).
//!
//! Visibility: both entry points are `pub(super)`. `parse_aggregate_decl`
//! is called by the feature-skeleton walker in `lzi/mod.rs`;
//! `parse_invariant_decl` is shared with the resource dispatcher in
//! `resource/mod.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;

use crate::ast::{AggregateDecl, InvariantDecl, Span};

pub(in super::super) fn parse_aggregate_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AggregateDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let rest = header_trimmed
        .strip_prefix("aggregate ")
        .ok_or_else(|| line_error(header, "aggregate header must be `aggregate <Name>`"))?;
    let name = rest.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "aggregate header requires a name (`aggregate <Name>`)",
        ));
    }
    let name = name.to_owned();
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut root: Option<String> = None;
    let mut contains: Vec<String> = Vec::new();
    let mut invariants: Vec<InvariantDecl> = Vec::new();
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
                "aggregate body children use one indentation level deeper than the `aggregate` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("root ") {
            if root.is_some() {
                return Err(line_error(line, "aggregate declares `root` at most once"));
            }
            let target = rest.trim();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`root` requires a resource name (`root <Resource>`)",
                ));
            }
            if target.split_whitespace().count() != 1 {
                return Err(line_error(line, "`root` accepts exactly one resource name"));
            }
            root = Some(target.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("contains ") {
            let parts = rest.split(',').map(str::trim).filter(|s| !s.is_empty());
            for member in parts {
                if member.split_whitespace().count() != 1 {
                    return Err(line_error(
                        line,
                        "`contains` accepts comma-separated resource names only",
                    ));
                }
                contains.push(member.to_owned());
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "invariants" {
            // Open-block form: child block of `invariant <name>` blocks
            // at grandchild_indent.
            i += 1;
            last_end = line.end;
            while i < lines.len() {
                let inv_line = &lines[i];
                let inv_trim = inv_line.text.trim_start();
                if is_trivia(inv_trim) {
                    i += 1;
                    continue;
                }
                if inv_line.indent <= child_indent {
                    break;
                }
                if inv_line.indent != grandchild_indent {
                    return Err(line_error(
                        inv_line,
                        "`invariants` children use one indentation level deeper than the `invariants` header",
                    ));
                }
                if let Some(inv_rest) = inv_trim.strip_prefix("invariant ") {
                    let (inv, next) = parse_invariant_decl(lines, i, inv_rest)?;
                    invariants.push(inv);
                    last_end = lines[next.saturating_sub(1).max(i)].end;
                    i = next;
                    continue;
                }
                return Err(line_error(
                    inv_line,
                    "`invariants` body accepts only `invariant <name>` blocks",
                ));
            }
            continue;
        }

        return Err(line_error(
            line,
            "`aggregate` children are `root`, `contains`, or `invariants`",
        ));
    }

    let root = root
        .ok_or_else(|| line_error(header, "aggregate requires a `root <Resource>` declaration"))?;

    Ok((
        AggregateDecl {
            name,
            root,
            contains,
            invariants,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse a single `invariant <name>` block. Reused by
/// `parse_aggregate_decl` (aggregate-scoped) and the resource parser
/// (resource-scoped). Closed body: `when <expr>` (required), `message
/// "<text>"` (optional).
///
/// `name_rest` is the substring after `invariant ` on the header line.
pub(in super::super) fn parse_invariant_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    name_rest: &str,
) -> Result<(InvariantDecl, usize), ParseError> {
    let header = &lines[start];
    let name = name_rest.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "`invariant` requires a name (`invariant <name>`)",
        ));
    }
    if name.split_whitespace().count() != 1 {
        return Err(line_error(
            header,
            "`invariant` accepts exactly one name identifier",
        ));
    }
    let name = name.to_owned();
    let header_indent = header.indent;
    let child_indent = header_indent + 2;

    let mut when: Option<String> = None;
    let mut message: String = String::new();
    let mut message_seen = false;
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
                "invariant body children use one indentation level deeper than the `invariant` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("when ") {
            if when.is_some() {
                return Err(line_error(line, "invariant declares `when` at most once"));
            }
            let expr = rest.trim();
            if expr.is_empty() {
                return Err(line_error(line, "`when` requires a predicate expression"));
            }
            when = Some(expr.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("message ") {
            if message_seen {
                return Err(line_error(
                    line,
                    "invariant declares `message` at most once",
                ));
            }
            message_seen = true;
            let raw = rest.trim();
            if let Some(quoted) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                message = quoted.to_owned();
            } else {
                message = raw.to_owned();
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`invariant` children are `when <predicate>` and optional `message \"<text>\"`",
        ));
    }

    let when =
        when.ok_or_else(|| line_error(header, "`invariant` requires a `when <predicate>` clause"))?;

    Ok((
        InvariantDecl {
            name,
            when,
            message,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
