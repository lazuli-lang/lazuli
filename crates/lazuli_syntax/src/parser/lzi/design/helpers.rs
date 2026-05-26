//! Shared design-token helpers — name/value blocks, scale blocks, and
//! the bare/quoted ident splitter that handles digit-leading names.
//!
//! Extracted from the original monolithic `design.rs`. Consumed by
//! every per-group parser (`color`, `typography`, `motion`, `scales`)
//! and by the top-level dispatch in `design/mod.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, strip_inline_comment};
use super::super::super::error::ParseError;
use crate::ast::TextScaleTokenAst;

/// Generic `<name> <value>` block parser used by space/radius/shadow/
/// breakpoint/z plus motion.duration/easing plus typography.family/
/// weight/tracking. Values are captured verbatim with surrounding quotes
/// stripped if present; the analyzer applies type-specific validation.
pub(super) fn parse_design_named_value_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<(String, String)>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "design value entries use one indentation level deeper than the group header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, rest) = split_design_name(line, trimmed)?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(line_error(
                line,
                "design value entry requires `<name> <value>`",
            ));
        }
        entries.push((name, strip_design_quotes(rest).to_owned()));
        i += 1;
    }
    Ok((entries, i))
}

/// Parse `typography.scale` body: `<name> size <size>, line_height <lh>`.
pub(super) fn parse_design_scale_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<TextScaleTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<TextScaleTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "typography.scale entries use one indentation level deeper than the `scale` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        // Expected: `size <size>, line_height <lh>`.
        let after = after.strip_prefix("size ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry must be `<name> size <size>, line_height <lh>`",
            )
        })?;
        let comma_idx = after.find(',').ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry requires `, line_height <value>` after the size",
            )
        })?;
        let size = strip_design_quotes(after[..comma_idx].trim()).to_owned();
        let after_comma = after[comma_idx + 1..].trim();
        let lh = after_comma.strip_prefix("line_height ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry expects `line_height <value>` after the comma",
            )
        })?;
        let line_height = strip_design_quotes(lh.trim()).to_owned();
        entries.push(TextScaleTokenAst {
            name,
            size,
            line_height,
        });
        i += 1;
    }
    Ok((entries, i))
}

/// Split `<name> <rest>` where `<name>` may be a bare ident or a quoted
/// string (needed for digit-leading names like `"2xl"`). The split
/// happens at the first whitespace following the (possibly-quoted) name.
pub(super) fn split_design_name<'a>(
    line: &SourceLine<'_>,
    trimmed: &'a str,
) -> Result<(String, &'a str), ParseError> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err(line_error(line, "expected a token name"));
    }
    let (name_text, rest) = if bytes[0] == b'"' {
        // Scan to matching closing quote.
        let mut i = 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return Err(line_error(line, "unterminated quoted token name"));
        }
        let name = &trimmed[1..i];
        let after = trimmed[i + 1..].trim_start();
        (name.to_owned(), after)
    } else {
        let end = bytes
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let name = trimmed[..end].to_owned();
        let after = trimmed[end..].trim_start();
        (name, after)
    };
    if name_text.is_empty() {
        return Err(line_error(line, "token name cannot be empty"));
    }
    Ok((name_text, rest))
}

/// Strip surrounding `"..."` quotes if present, returning the inner slice.
pub(super) fn strip_design_quotes(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}
