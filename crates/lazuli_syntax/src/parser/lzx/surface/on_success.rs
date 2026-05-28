//! `view create` post-submit redirect / flash / invalidates contract.
//!
//! Parses the `on_success` child block of a `view create` body. The
//! block is grammar-restricted to a small alphabet:
//!
//! ```text
//! on_success
//!   back
//!   redirect "/path"
//!   flash success|error|info @translation.<key>
//!   invalidates query.<name>
//!   replace
//! ```
//!
//! Each child line is at most once. The block is mounted from
//! `surface/mod.rs`' view dispatcher and feeds `OnSuccessSpecAst`
//! straight into the view-create branch. The `replace` flag tweaks the
//! navigation transition (push vs replace) at the runtime layer; the
//! parser only records its presence.
//!
//! Errors carry the canonical "block keyword does not accept inline
//! content" prose so doctor/LSP can lift the message verbatim.

use super::super::super::common::{SourceLine, is_trivia, line_error, strip_inline_comment};
use super::super::super::error::ParseError;
use super::super::super::lzi::{parse_invalidates_entry, parse_translation_key_token};
use crate::ast::{FlashSpecAst, InvalidatesDecl, OnSuccessSpecAst, Span};

pub(crate) fn parse_on_success_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(OnSuccessSpecAst, usize), ParseError> {
    let header = &lines[start];
    let child_indent = parent_indent + 2;
    let mut back = false;
    let mut redirect: Option<String> = None;
    let mut flash: Option<FlashSpecAst> = None;
    let mut invalidates: Vec<InvalidatesDecl> = Vec::new();
    let mut replace = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`on_success` children use one indentation level deeper than the block header",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed == "back" {
            if back {
                return Err(line_error(
                    line,
                    "`on_success.back` is declared at most once",
                ));
            }
            back = true;
        } else if let Some(rest) = trimmed.strip_prefix("redirect ") {
            if redirect.is_some() {
                return Err(line_error(
                    line,
                    "`on_success.redirect` is declared at most once",
                ));
            }
            redirect = Some(parse_on_success_redirect(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("flash ") {
            if flash.is_some() {
                return Err(line_error(
                    line,
                    "`on_success.flash` is declared at most once",
                ));
            }
            flash = Some(parse_on_success_flash(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("invalidates ") {
            invalidates.push(parse_invalidates_entry(line, rest)?);
        } else if trimmed == "replace" {
            if replace {
                return Err(line_error(
                    line,
                    "`on_success.replace` is declared at most once",
                ));
            }
            replace = true;
        } else {
            return Err(line_error(
                line,
                "`on_success` children are `back`, `redirect \"<path>\"`, `flash <success|error|info> @translation.<key>`, `invalidates query.<name>`, or `replace`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        OnSuccessSpecAst {
            back,
            redirect,
            flash,
            invalidates,
            replace,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_on_success_redirect(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let trimmed = rest.trim();
    let Some(after_open) = trimmed.strip_prefix('"') else {
        return Err(line_error(
            line,
            "`on_success.redirect` target must be a quoted string",
        ));
    };
    let Some(close_idx) = after_open.find('"') else {
        return Err(line_error(
            line,
            "`on_success.redirect` target is missing the closing quote",
        ));
    };
    let value = after_open[..close_idx].to_owned();
    if !after_open[close_idx + 1..].trim().is_empty() {
        return Err(line_error(
            line,
            "`on_success.redirect` accepts exactly one quoted string",
        ));
    }
    Ok(value)
}

fn parse_on_success_flash(line: &SourceLine<'_>, rest: &str) -> Result<FlashSpecAst, ParseError> {
    let mut parts = rest.trim().splitn(2, char::is_whitespace);
    let kind = parts.next().unwrap_or("");
    if !matches!(kind, "success" | "error" | "info") {
        return Err(line_error(
            line,
            "`on_success.flash` kind must be `success`, `error`, or `info`",
        ));
    }
    let message_key = parse_translation_key_token(line, parts.next().unwrap_or(""))?;
    Ok(FlashSpecAst {
        kind: kind.to_owned(),
        message_key,
        span: Span::new(line.start, line.end),
    })
}
