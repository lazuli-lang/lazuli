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

// =============================================================================
// Iron-hand context vocabulary — parser tests for `purpose`, `non_goals`,
// `attach_ctx`. Driven by docs/canonical-semantics.md#feature-context-
// vocabulary and the meta-bundle proposal: the `tdd-iron-hand` preset
// escalates the three VOCAB-CONTEXT-* rules from warn to error, so the
// parser MUST anchor each field with its own span for precise
// diagnostics.
// =============================================================================
#[cfg(test)]
mod iron_hand_context_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn purpose_line_lowers_into_skeleton() {
        let source = "\nfeature catalog\n  purpose \"Discover and book lodging\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let purpose = features[0].purpose.as_ref().expect("purpose present");
        assert_eq!(purpose.text, "Discover and book lodging");
    }

    #[test]
    fn empty_purpose_string_parses_but_keeps_empty_text() {
        // The lint, not the parser, decides whether empty is allowed.
        let source = "\nfeature catalog\n  purpose \"\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features[0].purpose.as_ref().unwrap().text, "");
    }

    #[test]
    fn purpose_requires_quoted_string() {
        let source = "\nfeature catalog\n  purpose Discover and book lodging\n";
        let err = parse_feature_skeletons(source).expect_err("rejects bareword");
        let msg = format!("{err}");
        assert!(
            msg.contains("quoted string"),
            "expected quoted-string diagnostic, got: {msg}",
        );
    }

    #[test]
    fn duplicate_purpose_is_rejected() {
        let source = "\nfeature catalog\n  purpose \"A\"\n  purpose \"B\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `purpose`"), "got: {msg}");
    }

    #[test]
    fn non_goals_flat_form_collects_entries() {
        let source = "\nfeature catalog\n  non_goals\n    \"Full marketplace listing optimization\"\n    \"Real-time chat (use messaging feature)\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert_eq!(block.entries.len(), 2);
        assert_eq!(block.entries[0], "Full marketplace listing optimization");
        assert_eq!(block.entries[1], "Real-time chat (use messaging feature)");
    }

    #[test]
    fn non_goals_partitioned_form_flattens_into_entries() {
        let source = "\nfeature customer\n  non_goals\n    delegated_to\n      user: \"staff authentication\"\n      customer_auth: \"customer login and MFA\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert_eq!(block.entries.len(), 2);
        assert_eq!(block.entries[0], "staff authentication");
        assert_eq!(block.entries[1], "customer login and MFA");
    }

    #[test]
    fn non_goals_empty_block_is_legal_at_parse_time() {
        // Lint VOCAB-CONTEXT-NONGOALS-001 owns the empty-block rule.
        let source = "\nfeature catalog\n  non_goals\n  defaults\n    timestamps\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert!(block.entries.is_empty());
    }

    #[test]
    fn duplicate_non_goals_is_rejected() {
        let source = "\nfeature catalog\n  non_goals\n    \"A\"\n    \"B\"\n  non_goals\n    \"C\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `non_goals`"), "got: {msg}");
    }

    #[test]
    fn attach_ctx_line_lowers_into_skeleton() {
        let source = "\nfeature catalog\n  attach_ctx \"./ctx.md\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let ctx = features[0].attach_ctx.as_ref().expect("attach_ctx present");
        assert_eq!(ctx.path, "./ctx.md");
    }

    #[test]
    fn attach_ctx_requires_quoted_path() {
        let source = "\nfeature catalog\n  attach_ctx ./ctx.md\n";
        let err = parse_feature_skeletons(source).expect_err("rejects bareword");
        let msg = format!("{err}");
        assert!(msg.contains("quoted relative path"), "got: {msg}");
    }

    #[test]
    fn duplicate_attach_ctx_is_rejected() {
        let source = "\nfeature catalog\n  attach_ctx \"./a.md\"\n  attach_ctx \"./b.md\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `attach_ctx`"), "got: {msg}");
    }

    #[test]
    fn feature_level_context_string_is_retired() {
        // The dead feature-header `context "<path>"` form used to be
        // silently dropped (no parser branch). It is now a HARD parse
        // error pointing the author at `attach_ctx`. Negative case.
        let source = "\nfeature catalog\n  context \"@x\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dead context form");
        let msg = format!("{err}");
        assert!(msg.contains("attach_ctx"), "got: {msg}");
    }

    #[test]
    fn feature_level_context_emits_e_context_retired_code() {
        // Mirrors `E-WORKFLOW-RETIRED`: the rejection ships with the
        // stable `E-CONTEXT-RETIRED` code prefix on the diagnostic so
        // the analyzer / LSP / downstream tooling recognise it by code.
        let source = "\nfeature catalog\n  context \"@docs/customer/customer.ctx.md\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dead context form");
        let msg = format!("{err}");
        assert!(msg.contains("E-CONTEXT-RETIRED"), "got: {msg}");
    }

    #[test]
    fn attach_ctx_still_parses_after_context_retirement() {
        // Positive: the canonical `attach_ctx "<path>"` form is
        // unaffected by the `context` retirement.
        let source = "\nfeature catalog\n  attach_ctx \"@x\"\n";
        let features = parse_feature_skeletons(source).expect("attach_ctx still parses");
        let ctx = features[0].attach_ctx.as_ref().expect("attach_ctx present");
        assert_eq!(ctx.path, "@x");
    }

    #[test]
    fn iron_hand_block_combines_with_existing_children() {
        // Smoke-check the three fields parse alongside resources /
        // commands / defaults — the canonical iron-hand-clean layout.
        let source = r#"
feature catalog
  purpose "Discover and book lodging via host properties + services"
  non_goals
    "Full marketplace listing optimization"
    "Real-time chat (use messaging feature)"
  attach_ctx "./ctx.md"
  defaults
    timestamps
  resource Property
    name: Text required
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let f = &features[0];
        assert_eq!(
            f.purpose.as_ref().unwrap().text,
            "Discover and book lodging via host properties + services"
        );
        assert_eq!(f.non_goals.as_ref().unwrap().entries.len(), 2);
        assert_eq!(f.attach_ctx.as_ref().unwrap().path, "./ctx.md");
        assert_eq!(f.resources.len(), 1);
    }
}
