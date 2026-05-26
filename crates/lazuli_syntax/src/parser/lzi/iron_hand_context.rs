//! Iron-hand context vocabulary — feature-scoped `purpose`,
//! `non_goals`, and `attach_ctx` directives.
//!
//! Extracted from `feature_walker.rs` so the main `parse_feature_skeleton`
//! line walker stays Rails-thin. Each helper returns the parsed AST node
//! plus the index of the first line not consumed; the caller folds the
//! result into its `FeatureSkeleton` builder.

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::helpers::take_quoted_string;
use crate::ast::{LziFeatureAttachCtx, LziFeatureNonGoals, LziFeaturePurpose, Span};

/// Parse a single `purpose "<sentence>"` line at feature-child indent.
/// The caller has already validated indent + keyword prefix.
pub(super) fn parse_feature_purpose_line(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LziFeaturePurpose, ParseError> {
    let (text, tail) = take_quoted_string(rest.trim_start(), line).map_err(|_| {
        line_error(
            line,
            "`purpose` requires a quoted string literal — e.g. \
             `purpose \"Discover and book lodging\"`",
        )
    })?;
    if !tail.trim().is_empty() {
        return Err(line_error(
            line,
            "`purpose` accepts exactly one quoted string and no trailing tokens",
        ));
    }
    Ok(LziFeaturePurpose {
        text,
        span: Span::new(line.start, line.end),
    })
}

/// Parse a single `attach_ctx "<relative-path>"` line at feature-child
/// indent. The caller has already validated indent + keyword prefix.
pub(super) fn parse_feature_attach_ctx_line(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LziFeatureAttachCtx, ParseError> {
    let (path, tail) = take_quoted_string(rest.trim_start(), line).map_err(|_| {
        line_error(
            line,
            "`attach_ctx` requires a quoted relative path — e.g. \
             `attach_ctx \"./ctx.md\"`",
        )
    })?;
    if !tail.trim().is_empty() {
        return Err(line_error(
            line,
            "`attach_ctx` accepts exactly one quoted path and no trailing tokens",
        ));
    }
    Ok(LziFeatureAttachCtx {
        path,
        span: Span::new(line.start, line.end),
    })
}

/// Parse a `non_goals` block starting at `lines[start]`. Two surface
/// shapes flatten to a single `entries` list (see crate docs / proposal
/// `VOCAB-CONTEXT-NONGOALS-001`):
///
///   non_goals
///     "Full marketplace listing optimization"
///     "Real-time chat (use messaging feature)"
///
///   non_goals
///     delegated_to
///       customer_auth: "customer login and MFA"
///     out_of_scope
///       "Invoicing"
///
/// Returns the parsed block AST + the index of the first line not
/// consumed.
pub(super) fn parse_feature_non_goals_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LziFeatureNonGoals, usize), ParseError> {
    let line = &lines[start];
    let header_indent = line.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = child_indent + 2;
    let block_start = line.start;
    let mut block_end = line.end;
    let mut entries: Vec<String> = Vec::new();
    let mut j = start + 1;
    while j < lines.len() {
        let child = &lines[j];
        let child_trim = child.text.trim_start();
        if is_trivia(child_trim) {
            j += 1;
            continue;
        }
        if child.indent <= header_indent {
            break;
        }
        if child.indent != child_indent {
            return Err(line_error(
                child,
                "`non_goals` entries must be indented by exactly two spaces \
                 beyond the `non_goals` header",
            ));
        }
        // Partitioned form: `delegated_to` / `out_of_scope`
        // group header followed by `key: "text"` lines.
        if child_trim == "delegated_to" || child_trim == "out_of_scope" {
            block_end = child.end;
            let mut k = j + 1;
            while k < lines.len() {
                let grand = &lines[k];
                let grand_trim = grand.text.trim_start();
                if is_trivia(grand_trim) {
                    k += 1;
                    continue;
                }
                if grand.indent <= child_indent {
                    break;
                }
                if grand.indent != grandchild_indent {
                    return Err(line_error(
                        grand,
                        "`non_goals` partition entries must be indented by exactly \
                         two spaces beyond their group header",
                    ));
                }
                // Accept either `key: "text"` (the canonical
                // partitioned shape) or a bare `"text"` line.
                if let Some(colon_pos) = grand_trim.find(':') {
                    let value_part = grand_trim[colon_pos + 1..].trim_start();
                    let (text, tail) = take_quoted_string(value_part, grand).map_err(|_| {
                        line_error(
                            grand,
                            "`non_goals` partition entry value must be a quoted \
                             string — e.g. `customer_auth: \"customer login and MFA\"`",
                        )
                    })?;
                    if !tail.trim().is_empty() {
                        return Err(line_error(
                            grand,
                            "`non_goals` partition entry accepts exactly one quoted \
                             string after `:`",
                        ));
                    }
                    entries.push(text);
                } else {
                    let (text, tail) = take_quoted_string(grand_trim, grand).map_err(|_| {
                        line_error(
                            grand,
                            "`non_goals` entries must be quoted strings or \
                             `<key>: \"<text>\"` pairs",
                        )
                    })?;
                    if !tail.trim().is_empty() {
                        return Err(line_error(
                            grand,
                            "`non_goals` entries accept exactly one quoted string \
                             per line",
                        ));
                    }
                    entries.push(text);
                }
                block_end = grand.end;
                k += 1;
            }
            j = k;
            continue;
        }
        // Flat form: bare quoted string at child indent.
        let (text, tail) = take_quoted_string(child_trim, child).map_err(|_| {
            line_error(
                child,
                "`non_goals` entries must be quoted strings — e.g. \
                 `  \"Full marketplace listing optimization\"`",
            )
        })?;
        if !tail.trim().is_empty() {
            return Err(line_error(
                child,
                "`non_goals` entries accept exactly one quoted string per line",
            ));
        }
        entries.push(text);
        block_end = child.end;
        j += 1;
    }
    Ok((
        LziFeatureNonGoals {
            entries,
            span: Span::new(block_start, block_end),
        },
        j,
    ))
}
