//! Resource-scoped `lifecycle_routes` block parser (router-w4).
//!
//! A `lifecycle_routes` block maps lifecycle-state values (or the
//! sentinels `none` / `*`) to URL strings — used by the codegen
//! router-w4 work to emit post-transition redirects. The block lives
//! directly under a `resource <Name>` body:
//!
//! ```text
//! resource Order
//!   lifecycle_routes
//!     pending  -> "/orders/pending"
//!     paid     -> "/orders/{id}"
//!     *        -> "/orders"
//! ```
//!
//! Closed-grammar rules:
//!
//! - Arms live at grandchild indent (4 spaces from `resource` start).
//! - State is a bare identifier, the sentinel `none`, or the wildcard `*`.
//! - URL is a double-quoted string.
//! - At least one arm is required — an empty block is a parse error.
//!
//! Visibility: `parse_resource_lifecycle_routes` is `pub(super)` — only
//! the `resource <Name>` dispatcher in `resource/mod.rs` calls it.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;

use crate::ast::{ResourceLifecycleRouteArmAst, ResourceLifecycleRoutesAst, Span};

pub(super) fn parse_resource_lifecycle_routes(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ResourceLifecycleRoutesAst, usize), ParseError> {
    let header = &lines[start];
    let mut arms = Vec::new();
    let mut i = start + 1;
    let body_indent = header.indent + 2;
    let mut last_end = header.end;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "`lifecycle_routes` arms use grandchild indentation (4 spaces)",
            ));
        }
        let (state, rest) = trimmed.split_once("->").ok_or_else(|| {
            line_error(
                line,
                "`lifecycle_routes` arms use `<state> -> \"<url>\"` (state name, `none`, or `*`)",
            )
        })?;
        let state = state.trim().to_owned();
        if state.is_empty() {
            return Err(line_error(
                line,
                "`lifecycle_routes` arm state must be a bare identifier, `none`, or `*`",
            ));
        }
        let url_text = rest.trim();
        if !url_text.starts_with('"') || !url_text.ends_with('"') || url_text.len() < 2 {
            return Err(line_error(
                line,
                "`lifecycle_routes` arm URL must be a double-quoted string",
            ));
        }
        let url = url_text[1..url_text.len() - 1].to_owned();
        arms.push(ResourceLifecycleRouteArmAst {
            state,
            url,
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }
    if arms.is_empty() {
        return Err(line_error(
            header,
            "`lifecycle_routes` requires at least one `<state> -> \"<url>\"` arm",
        ));
    }
    Ok((
        ResourceLifecycleRoutesAst {
            arms,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
