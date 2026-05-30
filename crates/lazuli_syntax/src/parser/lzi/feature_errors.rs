//! `errors` block parser (IR Error-Vocab Cell PARSE-1).
//!
//! Promotes the pre-existing LSP-only shape validator (legacy site at
//! `crates/lazuli_lsp/src/lib.rs:6933` and around) into the canonical-indent
//! parser so the surface earns a real IR slot (`ir::FeatureErrors`).
//!
//! Header at indent 2 (FEATURE_CHILD); children at indent 4 (AGENT_CHILD).
//! Closed-catalog grammar (verbatim from
//! `docs/proposals/ir-error-messages-vocab.md` §2.C):
//!
//! ```text
//!   errors
//!     default hide
//!     expose client 4xx <comma-list>
//!     expose client 5xx <comma-list>
//!     <code> message @translation.<key>          (zero or more)
//! ```
//!
//! Closed-catalog enforcement (allowed codes, allowed field-name lists)
//! lives analyzer-side / doctor-side (see ERR-VOCAB-CODE-UNKNOWN /
//! ERR-VOCAB-EXPOSE-UNKNOWN); the parser keeps verbatim tokens so doctor
//! diagnostics can surface canonical messages with the offending text.

use super::super::common::{
    SourceLine, is_kebab_or_snake_ident, is_trivia, line_error, line_error_owned, unquote_lzx_value,
};
use super::super::error::ParseError;
use super::translation::parse_translation_key_token;
use crate::ast::{
    ErrorExposureDefaultAst, FeatureErrorExposeRuleDecl, FeatureErrorMessageDecl,
    FeatureErrorsDecl, Span,
};

pub(super) fn parse_feature_errors_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(FeatureErrorsDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut default: Option<ErrorExposureDefaultAst> = None;
    let mut exposure_4xx: Option<Vec<String>> = None;
    let mut exposure_5xx: Option<Vec<String>> = None;
    let mut audience_exposure: Vec<FeatureErrorExposeRuleDecl> = Vec::new();
    let mut redact_patterns: Vec<String> = Vec::new();
    let mut messages: Vec<FeatureErrorMessageDecl> = Vec::new();
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
                "`errors` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("default ") {
            if default.is_some() {
                return Err(line_error(
                    line,
                    "`errors` may declare at most one `default <hide|expose>` line",
                ));
            }
            match rest.trim() {
                "hide" => default = Some(ErrorExposureDefaultAst::Hide),
                "expose" => default = Some(ErrorExposureDefaultAst::Expose),
                _ => {
                    return Err(line_error(
                        line,
                        "`default` must be `default hide` or `default expose`",
                    ));
                }
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("expose client ") {
            let rest = rest.trim();
            let (kind, fields_text) = rest.split_once(' ').ok_or_else(|| {
                line_error(
                    line,
                    "`expose client <4xx|5xx> <comma-list>` requires both a status family and a field list",
                )
            })?;
            let fields: Vec<String> = fields_text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            if fields.is_empty() {
                return Err(line_error(
                    line,
                    "`expose client <4xx|5xx>` requires at least one field",
                ));
            }
            match kind {
                "4xx" => {
                    if exposure_4xx.is_some() {
                        return Err(line_error(
                            line,
                            "`errors` may declare at most one `expose client 4xx` line",
                        ));
                    }
                    exposure_4xx = Some(fields);
                }
                "5xx" => {
                    if exposure_5xx.is_some() {
                        return Err(line_error(
                            line,
                            "`errors` may declare at most one `expose client 5xx` line",
                        ));
                    }
                    exposure_5xx = Some(fields);
                }
                _ => {
                    return Err(line_error(
                        line,
                        "`expose client` status family must be `4xx` or `5xx`",
                    ));
                }
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("error_redact ") {
            let pattern = unquote_lzx_value(rest.trim()).trim().to_owned();
            if pattern.is_empty() {
                return Err(line_error(line, "`error_redact` requires a pattern"));
            }
            redact_patterns.push(pattern);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("expose to @audience ") {
            let (audience, fields_text) = rest.trim().split_once(' ').ok_or_else(|| {
                line_error(
                    line,
                    "`expose to @audience <name> <comma-list>` requires an audience and field list",
                )
            })?;
            if !is_kebab_or_snake_ident(audience) {
                return Err(line_error_owned(
                    line,
                    format!("audience `{}` must be kebab/snake case", audience),
                ));
            }
            let fields: Vec<String> = fields_text
                .split(',')
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(str::to_owned)
                .collect();
            if fields.is_empty() {
                return Err(line_error(
                    line,
                    "`expose to @audience` requires at least one field",
                ));
            }
            audience_exposure.push(FeatureErrorExposeRuleDecl {
                audience: Some(audience.to_owned()),
                fields,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        // `<code> message @translation.<key>` — closed-catalog enforced
        // analyzer-side. The parser only checks structural shape (split
        // on `message ` keyword).
        if let Some((code_part, message_part)) = trimmed.split_once(" message ") {
            let code = code_part.trim().to_owned();
            if code.is_empty() {
                return Err(line_error(
                    line,
                    "`<code> message @translation.<key>` requires a code identifier",
                ));
            }
            let key = parse_translation_key_token(line, message_part)?;
            messages.push(FeatureErrorMessageDecl {
                code,
                message: key,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`errors` children are `default <hide|expose>`, `expose client <4xx|5xx> <fields>`, or `<code> message @translation.<key>`",
        ));
    }

    Ok((
        FeatureErrorsDecl {
            default,
            exposure_4xx: exposure_4xx.unwrap_or_default(),
            exposure_5xx: exposure_5xx.unwrap_or_default(),
            audience_exposure,
            redact_patterns,
            messages,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// =============================================================================
// IR Error-Vocab (Cell PARSE-1) — parser slice tests for the `errors` block
// (`default`, `expose client <4xx|5xx>`, `<code> message @translation.<key>`).
// =============================================================================
#[cfg(test)]
mod feature_errors_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn feature_errors_block_lifts_default_exposure_and_messages() {
        let source = r#"
feature account
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.account_signin_required
    validation_failed message @translation.account_invalid_input
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let errors = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.errors.as_ref())
            .expect("errors block lifted");
        assert_eq!(
            errors.default,
            Some(crate::ast::ErrorExposureDefaultAst::Hide)
        );
        assert_eq!(errors.exposure_4xx, vec!["message", "code"]);
        assert_eq!(errors.exposure_5xx, vec!["code"]);
        assert_eq!(errors.messages.len(), 2);
        assert_eq!(errors.messages[0].code, "policy_denied");
        assert_eq!(errors.messages[0].message.key, "account_signin_required");
        assert_eq!(errors.messages[1].code, "validation_failed");
        assert_eq!(errors.messages[1].message.key, "account_invalid_input");
    }

    #[test]
    fn feature_errors_block_rejects_duplicate_block() {
        let source = r#"
feature account
  errors
    default hide
  errors
    default expose
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate errors block must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("at most one `errors` block"),
            "expected duplicate-block error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_block_rejects_invalid_default() {
        let source = r#"
feature account
  errors
    default sometimes
"#;
        let err = parse_feature_skeletons(source).expect_err("invalid default must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("`default hide` or `default expose`"),
            "expected canonical default error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_block_rejects_unknown_child() {
        let source = r#"
feature account
  errors
    splat ok
"#;
        let err = parse_feature_skeletons(source).expect_err("unknown errors child must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("`errors` children are"),
            "expected children-enumeration error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_round_trip_via_full_capsule_fixture() {
        // Smoke check that the canonical fixture (extended in Cell
        // PARSE-1) still parses end-to-end and the new IR slots are
        // populated for the `customer` feature.
        let source = include_str!("../../../../../examples/full-capsule/full-capsule.lzi");
        let features = parse_feature_skeletons(source).expect("parses");
        let customer = features
            .iter()
            .find(|f| f.name == "customer")
            .expect("customer feature");

        // Per-policy when_denied: the `edit` category (SPEC-07 C semantic
        // rename of the former `update`) gained one in the PARSE-1 fixture.
        let policies = customer.policies.as_ref().expect("policies block present");
        let update = policies
            .categories
            .iter()
            .find(|c| c.name == "edit")
            .expect("edit category");
        assert_eq!(
            update.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("customer_update_admin_only")
        );

        // Per-command when_denied: `capture_lead` gained one.
        let capture_lead = customer
            .commands
            .iter()
            .find(|c| c.name == "capture_lead")
            .expect("capture_lead command");
        assert_eq!(
            capture_lead
                .policy_when_denied
                .as_ref()
                .map(|k| k.key.as_str()),
            Some("capture_lead_signin_required")
        );

        // Feature-level errors block: two `<code> message
        // @translation.<key>` rows + the pre-existing exposure rules.
        let errors = customer.errors.as_ref().expect("errors block present");
        assert_eq!(
            errors.default,
            Some(crate::ast::ErrorExposureDefaultAst::Hide)
        );
        assert!(errors.exposure_4xx.contains(&"message".to_owned()));
        assert!(errors.exposure_4xx.contains(&"code".to_owned()));
        assert!(errors.exposure_5xx.contains(&"code".to_owned()));
        assert_eq!(errors.messages.len(), 2);
        let codes: Vec<&str> = errors.messages.iter().map(|m| m.code.as_str()).collect();
        assert!(codes.contains(&"policy_denied"));
        assert!(codes.contains(&"validation_failed"));
    }
}
