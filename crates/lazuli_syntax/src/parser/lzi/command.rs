//! Feature-level `command <name>` block parser and its child grammar.
//!
//! A command in Lazuli is the canonical declarative *write*: one
//! header, one policy, one optional audit envelope, and one effect
//! (`creates` / `updates` / `deletes`) or one targeted side effect
//! (`target` + `let` + `validate` + `emits` + `invalidates`).
//! Header sits at `AGENT_INDENT_FEATURE_CHILD` (2); children at
//! `AGENT_INDENT_AGENT_CHILD` (4); grandchildren (input slots, audit
//! `emit_to`, approval modifiers, effect assignments, emit-field
//! pairs) at `AGENT_INDENT_GRANDCHILD` (6).
//!
//! ## Module layout
//!
//! - `parse_command_decl` — dispatch entry consumed by the feature
//!   skeleton walker in `mod.rs`. The single big match table.
//! - `parse_command_route_slot` / `parse_command_input_block` /
//!   `parse_command_audit` / `parse_target_expr` / `parse_let_binding`
//!   / `parse_command_effect` / `parse_command_emit` /
//!   `parse_invalidates_entry` / `parse_command_deprecated` /
//!   `parse_deprecated_block` — `pub(super)` because `api.rs`, `job.rs`,
//!   and `report.rs` share the same declarative spine.
//! - `parse_command_approval`, `parse_command_triggers*`,
//!   `parse_command_write_window`, `parse_invalidates_block`,
//!   `parse_command_tests_block`, `take_quoted_or_word`,
//!   `split_command_input_modifiers` — file-private; only the
//!   command dispatch reaches them.
//!
//! ## Cross-file dependencies (kept in `mod.rs`)
//!
//! - Shared parser utilities (`split_call_signature`,
//!   `parse_named_args`, `extract_field_constraints`,
//!   `parse_translation_key_token`) live in `mod.rs` because they're
//!   referenced from sibling parsers (resource, query, policy,
//!   translation).
//! - Numeric helpers (`parse_rate_limit_line_body`,
//!   `fold_rate_limit_line`) sit in `numerics.rs`.
//! - Job-shared helpers (`parse_handler_line`, `parse_job_retry`,
//!   `parse_external_call`) sit in `job.rs`.
//!
//! See `docs/canonical-semantics.md` §"Command" for the prose
//! grammar reference.

use super::super::common::{
    SourceLine, is_kebab_or_snake_ident, is_trivia, line_error, line_error_owned, unquote_lzx_value,
};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::job::{parse_external_call, parse_handler_line, parse_job_retry};
use super::numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    extract_field_constraints, parse_named_args, parse_translation_key_token, split_call_signature,
};

use crate::ast::{
    ApprovalThenDecl, AssignmentDecl, CommandApproval, CommandAudit, CommandDecl,
    CommandDeprecatedDecl, CommandEffectDecl, CommandEffectKindDecl, CommandEmit, CommandInputDecl,
    CommandInputSlot, CommandRouteSlot, CommandRouteSlotKind, CommandWriteWindow, InvalidatesDecl,
    JobExternalCall, JobHandler, JobRetry, LetBindingDecl, PolicyExprAst, RateLimitSpecAst, Span,
    TargetExprDecl, TranslationKeyRefAst,
};

pub(super) fn parse_command_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("command ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "command header must be `command <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "command header requires a name"));
    }

    let mut previously: Vec<String> = Vec::new();
    let mut route: Vec<CommandRouteSlot> = Vec::new();
    let mut input = CommandInputDecl::Empty;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    // IR Error-Vocab (Cell PARSE-1) — `when_denied @translation.<key>`
    // child under `policy` at indent 6 (GRANDCHILD).
    let mut policy_when_denied: Option<TranslationKeyRefAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut audit: Option<CommandAudit> = None;
    let mut approval: Option<CommandApproval> = None;
    let mut target: Option<TargetExprDecl> = None;
    let mut lets: Vec<LetBindingDecl> = Vec::new();
    let mut validate: Vec<String> = Vec::new();
    let mut effect: Option<CommandEffectDecl> = None;
    let mut returns: Option<String> = None;
    let mut handler: Option<JobHandler> = None;
    let mut emits: Vec<CommandEmit> = Vec::new();
    let mut triggers: Vec<String> = Vec::new();
    let mut invalidates: Vec<InvalidatesDecl> = Vec::new();
    let mut external_calls: Vec<JobExternalCall> = Vec::new();
    let mut tests: Vec<String> = Vec::new();
    let mut deprecated: Option<CommandDeprecatedDecl> = None;
    let mut timeout: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut idempotency_by: Option<String> = None;
    let mut write_window: Option<CommandWriteWindow> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`command` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route.push(parse_command_route_slot(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed == "input" {
            let (parsed, next) = parse_command_input_block(lines, i)?;
            input = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("input ") {
            // Short form: `input <field>` — single inline name.
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(
                    line,
                    "`input <name>` short form requires a name",
                ));
            }
            input = CommandInputDecl::Short(value.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            // IR Error-Vocab (Cell PARSE-1) — consume optional
            // `when_denied @translation.<key>` child at indent 6
            // under the `policy` line.
            let mut j = i + 1;
            while j < lines.len() {
                let inner = &lines[j];
                let inner_trim = inner.text.trim_start();
                if is_trivia(inner_trim) {
                    j += 1;
                    continue;
                }
                if inner.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                if inner.indent != AGENT_INDENT_GRANDCHILD {
                    return Err(line_error(
                        inner,
                        "`policy` children use six-space indentation",
                    ));
                }
                if let Some(rest) = inner_trim.strip_prefix("when_denied ") {
                    if policy_when_denied.is_some() {
                        return Err(line_error(
                            inner,
                            "`policy` may declare at most one `when_denied` child (ERR-VOCAB-MULTIPLE-WHEN-DENIED)",
                        ));
                    }
                    policy_when_denied = Some(parse_translation_key_token(inner, rest)?);
                    last_end = inner.end;
                    j += 1;
                    continue;
                }
                return Err(line_error(
                    inner,
                    "`policy` children are `when_denied @translation.<key>` only",
                ));
            }
            i = j;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit data_subject ") {
            let subject_field = rest.trim();
            if subject_field.is_empty() || !is_kebab_or_snake_ident(subject_field) {
                return Err(line_error(
                    line,
                    "`audit data_subject` requires a field identifier",
                ));
            }
            let Some(audit_spec) = audit.as_mut() else {
                return Err(line_error(
                    line,
                    "`audit data_subject <field>` must follow an `audit <subjects>` line",
                ));
            };
            if audit_spec.data_subject.is_some() {
                return Err(line_error(
                    line,
                    "`audit data_subject` may be declared at most once",
                ));
            }
            audit_spec.data_subject = Some(subject_field.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            let (parsed, next) = parse_command_audit(lines, i, rest)?;
            audit = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "approval" {
            let (parsed, next) = parse_command_approval(lines, i)?;
            approval = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            let parsed = parse_target_expr(line, rest)?;
            target = Some(parsed);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            lets.push(parse_let_binding(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("validate ") {
            validate.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("creates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Creates, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("updates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Updates, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deletes ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Deletes, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(parse_handler_line(rest));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            let (parsed, next) = parse_command_emit(lines, i, rest)?;
            emits.push(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "triggers" {
            if !triggers.is_empty() {
                return Err(line_error(
                    line,
                    "`triggers transition` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_command_triggers_block(lines, i)?;
            triggers = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("triggers ") {
            if !triggers.is_empty() {
                return Err(line_error(
                    line,
                    "`triggers transition` may be declared at most once",
                ));
            }
            triggers = parse_command_triggers(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "invalidates" {
            let (parsed, next) = parse_invalidates_block(lines, i)?;
            invalidates.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("invalidates ") {
            // Single-line form: `invalidates query.list`.
            invalidates.push(parse_invalidates_entry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("calls ") {
            let (call, next) = parse_external_call(lines, i, rest)?;
            external_calls.push(call);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` timeout.
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` retry.
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` idempotency.
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("write_window ") {
            write_window = Some(parse_command_write_window(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — `gate behind plan.feature: ...` / `gate quota plan.limit: ...`.
            // These directives are lifted via the side-channel
            // `parse_feature_gates` pass. Accept and discard here so the
            // canonical-indent parser does not reject the body.
            last_end = line.end;
            i += 1;
        } else if trimmed == "tests" {
            let (parsed, next) = parse_command_tests_block(lines, i)?;
            tests.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "deprecated" {
            let (parsed, next) = parse_deprecated_block(lines, i)?;
            deprecated = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deprecated ") {
            deprecated = Some(parse_command_deprecated(line, rest)?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`command` children are `previously`, `route`, `input`, `policy`, `rate_limit`, `audit`, `approval`, `deprecated`, `target`, `let`, `validate`, `creates`/`updates`/`deletes`, `returns`, `handler`, `emits`, `triggers transition`, `invalidates`, `calls`, `timeout`, `retry`, `idempotency by`, `write_window`, or `tests`",
            ));
        }
    }

    Ok((
        CommandDecl {
            name,
            public_contract: None,
            previously,
            route,
            input,
            policy,
            policy_expr,
            policy_when_denied,
            rate_limit,
            audit,
            approval,
            target,
            lets,
            validate,
            effect,
            returns,
            handler,
            emits,
            triggers,
            invalidates,
            external_calls,
            timeout,
            retry,
            idempotency_by,
            write_window,
            tests,
            deprecated,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_command_triggers(line: &SourceLine<'_>, rest: &str) -> Result<Vec<String>, ParseError> {
    let rest = rest.trim();
    let names = if rest == "transition" {
        ""
    } else if let Some(names) = rest.strip_prefix("transition ") {
        names.trim()
    } else {
        // Legacy pilot files used `triggers <transition>` before the surface
        // grew the explicit `transition` discriminator. Keep accepting it,
        // but normalize to the same `CommandDecl.triggers` vector.
        rest
    };
    if names.is_empty() {
        return Err(line_error(
            line,
            "`triggers transition` requires at least one transition name",
        ));
    }

    parse_command_trigger_names(line, names)
}

fn parse_command_trigger_names(
    line: &SourceLine<'_>,
    names: &str,
) -> Result<Vec<String>, ParseError> {
    let mut triggers = Vec::new();
    for name in names.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`triggers transition` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        if name.chars().any(char::is_whitespace) {
            return Err(line_error(
                line,
                "transition names in `triggers transition` cannot contain whitespace; separate with commas",
            ));
        }
        triggers.push(name.to_owned());
    }
    Ok(triggers)
}

fn parse_command_triggers_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut triggers = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`triggers` children use six-space indentation",
            ));
        }

        let Some(rest) = trimmed.strip_prefix("transition ") else {
            return Err(line_error(
                line,
                "`triggers` children use `transition <name>[, <name>]`",
            ));
        };

        let parsed = parse_command_trigger_names(line, rest.trim())?;
        triggers.extend(parsed);
        i += 1;
    }

    if triggers.is_empty() {
        return Err(line_error(
            &lines[start],
            "`triggers` requires at least one `transition <name>` child",
        ));
    }

    Ok((triggers, i))
}

fn parse_command_write_window(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandWriteWindow, ParseError> {
    let Some(rest) = rest.trim().strip_prefix("by ") else {
        return Err(line_error(
            line,
            "`write_window` must be `write_window by <path> within <duration_or_ref>`",
        ));
    };
    let Some((by, within)) = rest.trim().split_once(" within ") else {
        return Err(line_error(
            line,
            "`write_window by <path>` requires `within <duration_or_ref>`",
        ));
    };
    let by = by.trim();
    let within = within.trim();
    if by.is_empty() {
        return Err(line_error(line, "`write_window by` requires a path"));
    }
    if within.is_empty() {
        return Err(line_error(
            line,
            "`write_window within` requires a duration or reference",
        ));
    }
    Ok(CommandWriteWindow {
        by: by.to_owned(),
        within: within.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

/// Parse `deprecated [since "<X>"] [replacement <ref>] [sunset "<Y>"]` —
/// inline single-line shape. Keys may appear in any order; each at most
/// once.
pub(super) fn parse_command_deprecated(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandDeprecatedDecl, ParseError> {
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut cursor = rest.trim();
    while !cursor.is_empty() {
        if let Some(after) = cursor.strip_prefix("since ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated since` requires a value"))?;
            since = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("replacement ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("sunset ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated sunset` requires a value"))?;
            sunset = Some(val);
            cursor = next.trim_start();
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
    }
    Ok(CommandDeprecatedDecl {
        since,
        replacement,
        sunset,
        span: Span::new(line.start, line.end),
    })
}

pub(super) fn parse_deprecated_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandDeprecatedDecl, usize), ParseError> {
    let header = &lines[start];
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`deprecated` block children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("since ") {
            since = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("replacement ") {
            let (value, _) = take_quoted_or_word(rest)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(value);
        } else if let Some(rest) = trimmed.strip_prefix("sunset ") {
            sunset = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        CommandDeprecatedDecl {
            since,
            replacement,
            sunset,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Take a quoted string or a single bare word (dotted refs allowed),
/// returning the unquoted value and the remainder of the input.
fn take_quoted_or_word(s: &str) -> Option<(String, &str)> {
    let trimmed = s.trim_start();
    if let Some(after_quote) = trimmed.strip_prefix('"') {
        let end = after_quote.find('"')?;
        let value = after_quote[..end].to_owned();
        let next = &after_quote[end + 1..];
        Some((value, next))
    } else {
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let value = trimmed[..end].to_owned();
        if value.is_empty() {
            return None;
        }
        let next = &trimmed[end..];
        Some((value, next))
    }
}

pub(super) fn parse_command_route_slot(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandRouteSlot, ParseError> {
    let rest = rest.trim();
    if rest == "signed_token" {
        return Ok(CommandRouteSlot {
            name: "signed_token".to_owned(),
            type_text: "Text".to_owned(),
            from: None,
            kind: CommandRouteSlotKind::SignedToken,
            span: Span::new(line.start, line.end),
        });
    }
    let signed_token_rest;
    let (kind, rest) = if let Some(after) = rest.strip_prefix("opaque ") {
        (CommandRouteSlotKind::OpaqueToken, after.trim())
    } else if let Some(after) = rest.strip_prefix("signed_token:") {
        signed_token_rest = format!("signed_token:{}", after);
        (
            CommandRouteSlotKind::SignedToken,
            signed_token_rest.as_str(),
        )
    } else {
        (CommandRouteSlotKind::Plain, rest)
    };
    let (name, after) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`route` requires `<name>: <Type>` (e.g. `route id: ID`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`route` requires a slot name before `:`"));
    }
    let after = after.trim();
    let (type_text, from) = if let Some(idx) = after.find(" from ") {
        let from_expr = after[idx + " from ".len()..].trim().to_owned();
        (after[..idx].trim().to_owned(), Some(from_expr))
    } else {
        (after.to_owned(), None)
    };
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`route` requires a type after `:` (e.g. `ID`)",
        ));
    }
    Ok(CommandRouteSlot {
        name: name.to_owned(),
        type_text,
        from,
        kind,
        span: Span::new(line.start, line.end),
    })
}

pub(super) fn parse_command_input_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandInputDecl, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`command input` children use six-space indentation",
            ));
        }

        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "`command input` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a name before `:`",
            ));
        }
        let rest = type_part.trim();
        // L0 #3 §10 — peel inline constraints first so the residual
        // string is just `<Type> [required|optional]`. Constraint
        // combination rules are enforced in the analyzer.
        let (rest_after, constraints) = extract_field_constraints(line, rest)?;
        // Walk to find the `required` or `optional` token at the end,
        // honouring parenthesised type-arg lists.
        let (type_text, required, optional) = split_command_input_modifiers(&rest_after);
        if type_text.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a type after `:`",
            ));
        }
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((CommandInputDecl::Typed(slots), i))
}

pub(super) fn split_command_input_modifiers(rest: &str) -> (String, bool, bool) {
    // Find the last whitespace-separated tokens. Walk from the right and
    // peel `required` / `optional` modifiers; whatever remains is the
    // type text.
    let mut type_text = rest.to_owned();
    let mut required = false;
    let mut optional = false;
    loop {
        let trimmed = type_text.trim_end();
        if trimmed.ends_with(" required") {
            required = true;
            type_text = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            optional = true;
            type_text = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else {
            type_text = trimmed.to_owned();
            break;
        }
    }
    (type_text, required, optional)
}

pub(super) fn parse_command_audit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(CommandAudit, usize), ParseError> {
    let header = &lines[start];
    let mut subjects: Vec<String> = Vec::new();
    let mut record_before = false;
    let mut record_after = false;
    let mut retain_for: Option<String> = None;
    for part in rest.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "before" {
            record_before = true;
        } else if part == "after" {
            record_after = true;
        } else if let Some(duration) = part.strip_prefix("retain ") {
            let duration = duration.trim();
            if duration.is_empty() {
                return Err(line_error(header, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
        } else {
            subjects.push(part.to_owned());
        }
    }
    if subjects.is_empty() && !record_before && !record_after && retain_for.is_none() {
        return Err(line_error(
            header,
            "`audit` requires at least one subject (e.g. `audit actor, target.id`)",
        ));
    }
    let mut emit_to: Option<String> = None;
    let mut data_subject: Option<String> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`audit` children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("emit_to ") {
            if emit_to.is_some() {
                return Err(line_error(
                    line,
                    "`audit emit_to` may be declared at most once",
                ));
            }
            emit_to = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("data_subject ") {
            let subject_field = rest.trim();
            if subject_field.is_empty() || !is_kebab_or_snake_ident(subject_field) {
                return Err(line_error(
                    line,
                    "`audit data_subject` requires a field identifier",
                ));
            }
            if data_subject.is_some() {
                return Err(line_error(
                    line,
                    "`audit data_subject` may be declared at most once",
                ));
            }
            data_subject = Some(subject_field.to_owned());
            i += 1;
        } else if trimmed == "before" {
            record_before = true;
            i += 1;
        } else if trimmed == "after" {
            record_after = true;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retain ") {
            if retain_for.is_some() {
                return Err(line_error(
                    line,
                    "`audit retain` may be declared at most once",
                ));
            }
            let duration = rest.trim();
            if duration.is_empty() {
                return Err(line_error(line, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`audit` children are `emit_to <event_group>`, `data_subject <field>`, `before`, `after`, or `retain <duration>` only",
            ));
        }
    }
    Ok((
        CommandAudit {
            subjects,
            emit_to,
            data_subject,
            record_before,
            record_after,
            retain_for,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

fn parse_command_approval(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandApproval, usize), ParseError> {
    let header = &lines[start];
    let mut required_when: Option<String> = None;
    let mut by: Option<String> = None;
    let mut timeout: Option<String> = None;
    let mut then: Option<ApprovalThenDecl> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`approval` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("required_when ") {
            required_when = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("by ") {
            by = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("then ") {
            then = Some(match rest.trim() {
                "deny" => ApprovalThenDecl::Deny,
                "allow" => ApprovalThenDecl::Allow,
                "escalate" => ApprovalThenDecl::Escalate,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!(
                            "`approval then` requires `deny`, `allow`, or `escalate` (got `{other}`)"
                        ),
                    ));
                }
            });
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`approval` children are `required_when`, `by`, `timeout`, or `then`",
            ));
        }
    }
    let by = by.ok_or_else(|| {
        line_error(
            header,
            "`approval` requires a `by @role.<name>` or `by @actor.<name>` declaration",
        )
    })?;
    let then = then.ok_or_else(|| {
        line_error(
            header,
            "`approval` requires a `then deny | allow | escalate` declaration",
        )
    })?;
    Ok((
        CommandApproval {
            required_when,
            by,
            timeout,
            then,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `target query.<name>(args)` — single-line; args are name=expr pairs
/// inside the parens. The parser keeps the dotted query reference
/// verbatim so the analyzer's namespace resolver decides between
/// local/cross-feature.
pub(super) fn parse_target_expr(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<TargetExprDecl, ParseError> {
    let rest = rest.trim();
    let (query_part, args_part) = split_call_signature(line, rest)?;
    let args = parse_named_args(line, args_part)?;
    Ok(TargetExprDecl {
        query: query_part.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

pub(super) fn parse_let_binding(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LetBindingDecl, ParseError> {
    let rest = rest.trim();
    let (name, value) = rest.split_once('=').ok_or_else(|| {
        line_error(
            line,
            "`let` requires `<name> = <expr>` (e.g. `let resolved = user.query.by_id(id: input.id)`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`let` requires a binding name before `=`"));
    }
    Ok(LetBindingDecl {
        name: name.to_owned(),
        value: value.trim().to_owned(),
        span: Span::new(line.start, line.end),
    })
}

/// Parse the `creates X`, `updates X`, `deletes X` family. Children at
/// AGENT_INDENT_GRANDCHILD (6) are `<field> = <expr>` assignments. The
/// `from input` shorthand collapses into `from_input: true` with no
/// assignment block.
pub(super) fn parse_command_effect(
    lines: &[SourceLine<'_>],
    start: usize,
    kind: CommandEffectKindDecl,
    rest: &str,
) -> Result<(CommandEffectDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (resource, from_input) = if let Some(res) = rest.strip_suffix(" from input") {
        (res.trim().to_owned(), true)
    } else {
        (rest.to_owned(), false)
    };
    if resource.is_empty() {
        return Err(line_error(
            header,
            "`creates`/`updates`/`deletes` requires a resource name",
        ));
    }
    let mut assignments: Vec<AssignmentDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "command effect children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "command effect assignments use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "command effect assignment requires a field name before `=`",
            ));
        }
        assignments.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((
        CommandEffectDecl {
            kind,
            resource,
            from_input,
            assignments,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `emits <event>` line. Recognises trailing ` from creates` /
/// ` from updates` / ` from deletes`. Optional child block uses six-
/// space indent with `<key> = <expr>` lines.
pub(super) fn parse_command_emit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(CommandEmit, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (name, from) = if let Some(n) = rest.strip_suffix(" from creates") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Creates))
    } else if let Some(n) = rest.strip_suffix(" from updates") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Updates))
    } else if let Some(n) = rest.strip_suffix(" from deletes") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Deletes))
    } else {
        (rest.to_owned(), None)
    };
    if name.is_empty() {
        return Err(line_error(header, "`emits` requires an event name"));
    }
    let mut fields: Vec<AssignmentDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`emits` children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "`emits` field children use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "`emits` field child requires a field name before `=`",
            ));
        }
        fields.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((
        CommandEmit {
            name,
            from,
            fields,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

fn parse_invalidates_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<InvalidatesDecl>, usize), ParseError> {
    let mut out: Vec<InvalidatesDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`invalidates` children use six-space indentation",
            ));
        }
        out.push(parse_invalidates_entry(line, trimmed)?);
        i += 1;
    }
    Ok((out, i))
}

pub(crate) fn parse_invalidates_entry(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<InvalidatesDecl, ParseError> {
    let rest = rest.trim();
    // `query.list` or `query.by_id(id: route.id)`.
    if rest.contains('(') {
        let (query, args_part) = split_call_signature(line, rest)?;
        let args = parse_named_args(line, args_part)?;
        Ok(InvalidatesDecl {
            query: query.to_owned(),
            args,
            span: Span::new(line.start, line.end),
        })
    } else {
        if rest.is_empty() {
            return Err(line_error(
                line,
                "`invalidates` entry requires a query reference",
            ));
        }
        Ok(InvalidatesDecl {
            query: rest.to_owned(),
            args: Vec::new(),
            span: Span::new(line.start, line.end),
        })
    }
}

fn parse_command_tests_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut out: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        out.push(trimmed.to_owned());
        i += 1;
    }
    Ok((out, i))
}
