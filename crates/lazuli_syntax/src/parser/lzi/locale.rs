//! `locale_negotiate <name>` block parser.
//!
//! The block declares how a request's effective locale is chosen:
//!
//! ```text
//! locale_negotiate primary
//!   source <expr>        # identifier expression (e.g. `header.accept_language`)
//!   strategy <name>      # catalog item (e.g. `best_match`)
//!   fallback "<locale>"  # quoted fallback locale tag
//! ```
//!
//! All three children are optional at parse time — the analyzer enforces
//! presence rules per axis. Indentation is locked: `locale_negotiate`
//! lives at a feature or api child indent `H`, body children at `H+2`.
//!
//! ## See also
//!
//! - `docs/proposals/locale-negotiate.md`
//! - `lazuli_ir::nodes::locale_negotiate` — lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use crate::ast::{LocaleNegotiateDecl, Span};

pub(super) fn parse_locale_negotiate_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LocaleNegotiateDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut source: Option<String> = None;
    let mut strategy: Option<String> = None;
    let mut fallback: Option<String> = None;
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
                "`locale_negotiate` body children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("strategy ") {
            strategy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("fallback ") {
            fallback = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`locale_negotiate` children are `source`, `strategy`, or `fallback`",
            ));
        }
    }

    Ok((
        LocaleNegotiateDecl {
            source,
            strategy,
            fallback,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
