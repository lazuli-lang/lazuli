//! Numeric and rate-limit lexer helpers shared across every `.lzi`
//! construct that admits a numeric tail. The `parse_*` family error out
//! with parser-friendly messages anchored to the offending source line
//! and lets the call sites stay declarative.
//!
//! ## Functions
//!
//! - `parse_float` — decimal literal (`f64`).
//! - `parse_uint32` — non-negative integer.
//! - `parse_int64` — signed integer.
//! - `parse_rate_limit_line_body` — splits `"<spec>"` from optional
//!   `in <env_list>` tail; returns the literal + envs.
//! - `fold_rate_limit_line` — accumulates parsed lines into the
//!   in-progress `RateLimitSpecAst` (default + by_env), enforcing the
//!   `rate_limit_duplicate_default` rule.
//!
//! The rate-limit helpers are `pub(super)` so every declaration that
//! carries a `rate_limit` line (agent, command, api, auth, query)
//! shares the same fold. Numeric helpers are `pub(super)` to feed the
//! agent block (temperature / max_tokens / top_p / seed).

use super::super::common::{SourceLine, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use crate::ast::{RateLimitByEnvAst, RateLimitSpecAst, Span};

pub(super) fn parse_float(line: &SourceLine<'_>, rest: &str) -> Result<f64, ParseError> {
    rest.trim()
        .parse::<f64>()
        .map_err(|_| line_error(line, "expected a decimal value (e.g. `0`, `0.2`)"))
}

pub(super) fn parse_uint32(line: &SourceLine<'_>, rest: &str) -> Result<u32, ParseError> {
    rest.trim()
        .parse::<u32>()
        .map_err(|_| line_error(line, "expected a non-negative integer"))
}

pub(super) fn parse_int64(line: &SourceLine<'_>, rest: &str) -> Result<i64, ParseError> {
    rest.trim()
        .parse::<i64>()
        .map_err(|_| line_error(line, "expected a signed integer"))
}

pub(super) fn parse_rate_limit_line_body(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<Vec<String>>), ParseError> {
    let trimmed = rest.trim_start();

    // The literal is a double-quoted string. Find the closing quote so
    // we can isolate the optional `in <env_list>` tail.
    if !trimmed.starts_with('"') {
        return Err(line_error(
            line,
            "`rate_limit` requires a quoted spec literal (e.g. `rate_limit \"5 per minute per ip\"`)",
        ));
    }
    let after_open = &trimmed[1..];
    let close_offset = after_open.find('"').ok_or_else(|| {
        line_error(
            line,
            "`rate_limit` spec literal is missing its closing quote",
        )
    })?;
    let literal = &after_open[..close_offset];
    if literal.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit` spec literal must be non-empty",
        ));
    }

    let tail = after_open[close_offset + 1..].trim_start();
    if tail.is_empty() {
        return Ok((literal.to_owned(), None));
    }

    // The only legal tail today is `in <env_list>`.
    let Some(after_in) = tail.strip_prefix("in") else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };
    // `in` must be separated from what follows by whitespace (so
    // `intermediate` doesn't accidentally parse). When the suffix is
    // empty (just `in` on its own), `strip_prefix` returns "", which
    // we explicitly reject below as an empty env list.
    let env_list_text = if let Some(stripped) = after_in.strip_prefix(char::is_whitespace) {
        stripped.trim()
    } else if after_in.is_empty() {
        ""
    } else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };

    if env_list_text.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit \"X\" in` requires at least one env name (e.g. `in dev, staging, test`)",
        ));
    }

    let mut envs: Vec<String> = Vec::new();
    for part in env_list_text.split(',') {
        let name = part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`rate_limit` env list cannot contain empty entries (drop the trailing/extra comma)",
            ));
        }
        // The closed catalog is validated at the analyzer / Cell 3
        // doctor layer; the parser accepts any identifier here and
        // surfaces unknown names as a doctor warning later.
        envs.push(name.to_owned());
    }

    Ok((literal.to_owned(), Some(envs)))
}

/// `ir-rate-limit-env-aware` cell 1 — accumulate one `rate_limit` line
/// into the in-progress `RateLimitSpecAst` for a declaration.
///
/// The first unqualified line establishes the default; further
/// unqualified lines are rejected (`rate_limit_duplicate_default`).
/// Qualified lines are appended to `by_env` in source order.
pub(super) fn fold_rate_limit_line(
    line: &SourceLine<'_>,
    spec: &mut Option<RateLimitSpecAst>,
    literal: String,
    envs: Option<Vec<String>>,
) -> Result<(), ParseError> {
    let entry_span = Span::new(line.start, line.end);
    let aggregate = spec.get_or_insert_with(|| RateLimitSpecAst {
        default: None,
        by_env: Vec::new(),
        span: entry_span,
    });
    aggregate.span = Span::new(aggregate.span.start, entry_span.end);

    match envs {
        None => {
            if aggregate.default.is_some() {
                return Err(line_error(
                    line,
                    "rate_limit_duplicate_default: declaration already carries an unqualified `rate_limit` line — add `in <env_list>` to differentiate or remove one",
                ));
            }
            aggregate.default = Some(unquote_lzx_value(&literal).to_owned());
        }
        Some(envs) => {
            aggregate.by_env.push(RateLimitByEnvAst {
                limit: literal,
                envs,
                span: entry_span,
            });
        }
    }

    Ok(())
}

// =============================================================================
// `ir-rate-limit-env-aware` cell 1 — parser tests for the env-qualified
// `rate_limit` shape (proposal §4.2, §9). Cover both back-compat
// (single-line `rate_limit "X"`) and the new multi-line shape with
// optional `in <env_list>` qualification.
// =============================================================================
#[cfg(test)]
mod rate_limit_env_aware_tests {
    use super::super::parse_feature_skeletons;

    fn feature_with_command_body(body: &str) -> String {
        format!(
            "\nfeature customer\n  resource Customer\n    name: Text required\n\n  command create\n    input\n      name: Text required\n    policy @policy.public\n{body}    creates Customer\n      name = params.name\n",
        )
    }

    fn single_command<'a>(
        features: &'a [crate::ast::FeatureSkeleton],
    ) -> &'a crate::ast::CommandDecl {
        let feature = features.first().expect("one feature parsed");
        feature.commands.first().expect("one command parsed")
    }

    #[test]
    fn single_unqualified_rate_limit_is_back_compat() {
        // Proposal §8 — the existing `rate_limit "X per Y per Z"` shape
        // must still parse and lower into `{ default: "X...", by_env: [] }`.
        let source = feature_with_command_body("    rate_limit \"5 per 10 minutes per ip\"\n");
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.default.as_deref(), Some("5 per 10 minutes per ip"));
        assert!(spec.by_env.is_empty());
    }

    #[test]
    fn default_plus_single_qualified_line_parses() {
        // Proposal §5.1 — `account.register` shape after the migration.
        let source = feature_with_command_body(
            "    rate_limit \"5 per 10 minutes per ip\"\n    rate_limit \"unlimited\" in dev, test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.default.as_deref(), Some("5 per 10 minutes per ip"));
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "unlimited");
        assert_eq!(entry.envs, vec!["dev".to_owned(), "test".to_owned()]);
    }

    #[test]
    fn single_line_with_three_envs_folds_into_one_entry() {
        // Proposal §5.4 — one qualified line, multiple envs.
        let source = feature_with_command_body(
            "    rate_limit \"5 per 1 minutes per user\"\n    rate_limit \"1000 per 1 minutes per user\" in dev, staging, test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "1000 per 1 minutes per user");
        assert_eq!(
            entry.envs,
            vec!["dev".to_owned(), "staging".to_owned(), "test".to_owned()]
        );
    }

    #[test]
    fn unlimited_keyword_in_qualified_line_preserves_literal_for_analyzer() {
        // Proposal §4.4 — `"unlimited"` is preserved verbatim at AST
        // level; the analyzer lowers it to the empty-string sentinel in
        // `ir::RateLimitByEnv.limit`.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"unlimited\" in test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "unlimited");
        assert_eq!(entry.envs, vec!["test".to_owned()]);
    }

    #[test]
    fn duplicate_default_lines_are_rejected() {
        // Proposal §9.2 — two unqualified declarations is the
        // `rate_limit_duplicate_default` error.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"10 per minute per ip\"\n",
        );
        let err = parse_feature_skeletons(&source).expect_err("duplicate default rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("rate_limit_duplicate_default"),
            "expected `rate_limit_duplicate_default` code, got: {msg}",
        );
    }

    #[test]
    fn empty_in_tail_is_rejected() {
        // Proposal §12 Cell 1 — `rate_limit "X" in` (empty list) errors.
        let source = feature_with_command_body("    rate_limit \"5 per minute per ip\" in\n");
        let err = parse_feature_skeletons(&source).expect_err("empty `in` tail rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("requires at least one env name"),
            "expected empty-env-list diagnostic, got: {msg}",
        );
    }

    #[test]
    fn unknown_env_identifier_parses_at_ast_level() {
        // Proposal §4.3 / §9.2 — the parser is forgiving; the doctor
        // (Cell 3) emits the `rate_limit_unknown_env` warning later.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"unlimited\" in dev, qa\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        // Raw identifiers as authored; analyzer normalises closed-catalog
        // names and surfaces unknowns via `ir::RateLimitByEnv.unknown_envs`.
        assert_eq!(spec.by_env[0].envs, vec!["dev".to_owned(), "qa".to_owned()]);
    }

    #[test]
    fn trailing_garbage_after_literal_is_rejected() {
        // Defensive — anything other than `in <env_list>` after the
        // quoted literal must fail (catches typos like `for` or `on`).
        let source =
            feature_with_command_body("    rate_limit \"5 per minute per ip\" on production\n");
        let err = parse_feature_skeletons(&source).expect_err("trailing garbage rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("`in <env_list>`"),
            "expected `in <env_list>` diagnostic, got: {msg}",
        );
    }
}
