//! Iron-hand context vocabulary — feature-scoped `purpose`,
//! `non_goals`, and `knowledge` directives.
//!
//! Extracted from `feature_walker.rs` so the main `parse_feature_skeleton`
//! line walker stays Rails-thin. Each helper returns the parsed AST node
//! plus the index of the first line not consumed; the caller folds the
//! result into its `FeatureSkeleton` builder.
//!
//! (Feature context is resolved by the co-located `<feature>.ctx.md`
//! CONVENTION in the analyzer — the retired `attach_ctx` keyword
//! hard-errors as `E-ATTACH-CTX-RETIRED` in `feature_walker/skeleton.rs`.)

use super::super::common::{SourceLine, is_kebab_or_snake_ident, is_trivia, line_error};
use super::super::error::ParseError;
use super::helpers::take_quoted_string;
use crate::ast::{LziFeatureKnowledge, LziFeatureNonGoals, LziFeaturePurpose, Span};

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

/// Parse a single `knowledge <sector>` line at feature-child indent. The
/// caller has already validated indent + keyword prefix.
///
/// Unlike `attach_ctx`, the argument is a **bareword** sector slug
/// (kebab/snake, e.g. `billing`) — NOT a quoted string — naming the
/// `knowledge/<sector>/` vault. Exactly one slug, no trailing
/// tokens. Sector ↔ on-disk-vault cross-checks live in the planned
/// `VOCAB-KNOWLEDGE-*` doctor lints (a later stage); the parser only
/// captures the slug verbatim. See
/// `docs/proposals/knowledge-sector-field.md`.
pub(super) fn parse_feature_knowledge_line(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LziFeatureKnowledge, ParseError> {
    let mut tokens = rest.split_whitespace();
    let Some(sector) = tokens.next() else {
        return Err(line_error(
            line,
            "`knowledge` requires a sector slug — e.g. `knowledge billing`",
        ));
    };
    if tokens.next().is_some() {
        return Err(line_error(
            line,
            "`knowledge` accepts exactly one sector slug and no trailing tokens",
        ));
    }
    if !is_kebab_or_snake_ident(sector) {
        return Err(line_error(
            line,
            "`knowledge` sector must be a kebab/snake slug \
             (lowercase letter first, then letters/digits/`_`/`-`) — \
             e.g. `knowledge billing`, not a quoted string",
        ));
    }
    Ok(LziFeatureKnowledge {
        sector: sector.to_string(),
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
        let source =
            "\nfeature catalog\n  non_goals\n    \"A\"\n    \"B\"\n  non_goals\n    \"C\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `non_goals`"), "got: {msg}");
    }

    #[test]
    fn attach_ctx_keyword_is_retired() {
        // The `attach_ctx "<path>"` keyword was retired in favour of the
        // co-located `<feature>.ctx.md` convention. It is now a HARD
        // parse error whose message names the convention (not another
        // retired keyword). Negative case.
        let source = "\nfeature catalog\n  attach_ctx \"./ctx.md\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects retired attach_ctx");
        let msg = format!("{err}");
        assert!(
            msg.contains(".ctx.md") && msg.contains("convention"),
            "expected the message to name the `<feature>.ctx.md` convention, got: {msg}",
        );
    }

    #[test]
    fn attach_ctx_emits_e_attach_ctx_retired_code() {
        // Mirrors `E-CONTEXT-RETIRED` / `E-WORKFLOW-RETIRED`: the
        // rejection ships with the stable `E-ATTACH-CTX-RETIRED` code
        // prefix so the analyzer / LSP / downstream tooling recognise it
        // by code.
        let source = "\nfeature catalog\n  attach_ctx \"@docs/customer/customer.ctx.md\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects retired attach_ctx");
        let msg = format!("{err}");
        assert!(msg.contains("E-ATTACH-CTX-RETIRED"), "got: {msg}");
    }

    #[test]
    fn feature_level_context_string_is_retired() {
        // The dead feature-header `context "<path>"` form used to be
        // silently dropped (no parser branch). It is now a HARD parse
        // error pointing the author at the `<feature>.ctx.md` convention.
        // Negative case — the message names the convention, NOT another
        // retired keyword.
        let source = "\nfeature catalog\n  context \"@x\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dead context form");
        let msg = format!("{err}");
        assert!(
            msg.contains(".ctx.md") && !msg.contains("attach_ctx"),
            "expected the message to name the convention and not `attach_ctx`, got: {msg}",
        );
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

    // ── knowledge <sector> — mirrors the attach_ctx tests, but the
    //    argument is a bareword sector slug, not a quoted string.
    //    See docs/proposals/knowledge-sector-field.md. ──

    #[test]
    fn knowledge_line_lowers_into_skeleton() {
        let source = "\nfeature billing\n  knowledge billing\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let knowledge = features[0].knowledge.as_ref().expect("knowledge present");
        assert_eq!(knowledge.sector, "billing");
    }

    #[test]
    fn knowledge_accepts_kebab_and_snake_slugs() {
        let source = "\nfeature ops\n  knowledge tax_reporting\n";
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(
            features[0].knowledge.as_ref().unwrap().sector,
            "tax_reporting"
        );

        let source = "\nfeature ops\n  knowledge tax-reporting\n";
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(
            features[0].knowledge.as_ref().unwrap().sector,
            "tax-reporting"
        );
    }

    #[test]
    fn knowledge_rejects_quoted_string() {
        // The sector is a bareword slug — a quoted string is not a valid
        // kebab/snake ident, so it is rejected (the inverse of attach_ctx,
        // which *requires* the quotes).
        let source = "\nfeature billing\n  knowledge \"billing\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects quoted sector");
        let msg = format!("{err}");
        assert!(msg.contains("kebab/snake slug"), "got: {msg}");
    }

    #[test]
    fn knowledge_rejects_trailing_tokens() {
        let source = "\nfeature billing\n  knowledge billing invoices\n";
        let err = parse_feature_skeletons(source).expect_err("rejects trailing tokens");
        let msg = format!("{err}");
        assert!(msg.contains("exactly one sector slug"), "got: {msg}");
    }

    #[test]
    fn duplicate_knowledge_is_rejected() {
        let source = "\nfeature billing\n  knowledge billing\n  knowledge invoicing\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `knowledge`"), "got: {msg}");
    }

    #[test]
    fn absent_knowledge_stays_none() {
        let source = "\nfeature billing\n  purpose \"Bill customers\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        assert!(features[0].knowledge.is_none());
    }

    #[test]
    fn knowledge_combines_with_purpose_non_goals() {
        // Smoke-check `knowledge` parses alongside the sibling context
        // fields + the rest of the iron-hand-clean layout. (Feature
        // context is now the co-located `<feature>.ctx.md` convention —
        // no `attach_ctx` line.)
        let source = r#"
feature billing
  purpose "Charge customers and reconcile invoices"
  non_goals
    "Tax calculation"
  knowledge billing
  defaults
    timestamps
  resource Invoice
    amount: Int required
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let f = &features[0];
        assert_eq!(
            f.purpose.as_ref().unwrap().text,
            "Charge customers and reconcile invoices"
        );
        assert_eq!(f.non_goals.as_ref().unwrap().entries.len(), 1);
        assert_eq!(f.knowledge.as_ref().unwrap().sector, "billing");
        assert_eq!(f.resources.len(), 1);
    }

    #[test]
    fn iron_hand_block_combines_with_existing_children() {
        // Smoke-check the context fields parse alongside resources /
        // commands / defaults — the canonical iron-hand-clean layout.
        // (Feature context is now the co-located `<feature>.ctx.md`
        // convention — no `attach_ctx` line.)
        let source = r#"
feature catalog
  purpose "Discover and book lodging via host properties + services"
  non_goals
    "Full marketplace listing optimization"
    "Real-time chat (use messaging feature)"
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
        assert_eq!(f.resources.len(), 1);
    }
}
