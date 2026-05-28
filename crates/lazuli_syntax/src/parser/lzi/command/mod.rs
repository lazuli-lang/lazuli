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
//! - `mod.rs` — `parse_command_decl`: the single big match table
//!   walking children of one `command <name>` header.
//! - `slots.rs` — `parse_command_route_slot`,
//!   `parse_command_input_block`, `split_command_input_modifiers`.
//!   Reused by `api.rs` (route/input slots) and `query.rs` (params).
//! - `audit.rs` — `parse_command_audit`. Reused by `report.rs` for
//!   its audit envelope grammar.
//! - `approval/triggers/write_window/deprecated/tests` lives in
//!   `lifecycle.rs` — the modifier-shape children that gate /
//!   temporalise / supersede a command.
//! - `effect.rs` — `parse_command_effect`, `parse_command_emit`,
//!   `parse_target_expr`, `parse_let_binding`. Reused by `job.rs`
//!   for its declarative-typed body.
//! - `invalidates.rs` — `parse_invalidates_block`,
//!   `parse_invalidates_entry`.
//!
//! ## Cross-file dependencies (kept in `super` = `lzi/mod.rs`)
//!
//! - Shared parser utilities (`split_call_signature`,
//!   `parse_named_args`, `extract_field_constraints`,
//!   `parse_translation_key_token`) live in `lzi/mod.rs` because
//!   they're referenced from sibling parsers (resource, query,
//!   policy, translation).
//! - Numeric helpers (`parse_rate_limit_line_body`,
//!   `fold_rate_limit_line`) sit in `numerics.rs`.
//! - Job-shared helpers (`parse_handler_line`, `parse_job_retry`,
//!   `parse_external_call`) sit in `job.rs`.
//!
//! See `docs/canonical-semantics.md` §"Command" for the prose
//! grammar reference.

mod audit;
mod effect;
mod invalidates;
mod lifecycle;
mod slots;

#[cfg(test)]
mod deprecated_tests;
#[cfg(test)]
mod gap_audit01_tests;
#[cfg(test)]
mod w4_tests;

pub(in crate::parser::lzi) use audit::parse_command_audit;
pub(in crate::parser::lzi) use effect::{parse_command_effect, parse_let_binding, parse_target_expr};
pub(crate) use invalidates::parse_invalidates_entry;
pub(in crate::parser::lzi) use lifecycle::{parse_command_deprecated, parse_deprecated_block};
pub(in crate::parser::lzi) use slots::{
    parse_command_input_block, parse_command_route_slot, split_command_input_modifiers,
};

use effect::parse_command_emit;
use invalidates::parse_invalidates_block;
use lifecycle::{
    parse_command_approval, parse_command_tests_block, parse_command_triggers,
    parse_command_triggers_block, parse_command_write_window,
};

use super::super::common::{
    SourceLine, is_kebab_or_snake_ident, is_trivia, line_error, unquote_lzx_value,
};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::job::{parse_external_call, parse_handler_line, parse_job_retry};
use super::numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    parse_translation_key_token,
};

use crate::ast::{
    CommandApproval, CommandAudit, CommandDecl, CommandDeprecatedDecl, CommandEffectDecl,
    CommandEffectKindDecl, CommandEmit, CommandInputDecl, CommandReorderDecl, CommandRouteSlot,
    CommandWriteWindow, InvalidatesDecl, JobExternalCall, JobHandler, JobRetry, LetBindingDecl,
    PolicyExprAst, RateLimitSpecAst, Span, TargetExprDecl, TranslationKeyRefAst,
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
    let mut reorder: Option<CommandReorderDecl> = None;
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
        } else if let Some(rest) = trimmed.strip_prefix("reorder ") {
            // W4 GAP-REORDER-01 — `reorder <Resource> by <position_field>`.
            if reorder.is_some() {
                return Err(line_error(
                    line,
                    "a command may declare at most one `reorder` body",
                ));
            }
            reorder = Some(parse_command_reorder(line, rest)?);
            last_end = line.end;
            i += 1;
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
                "`command` children are `previously`, `route`, `input`, `policy`, `rate_limit`, `audit`, `approval`, `deprecated`, `target`, `let`, `validate`, `creates`/`updates`/`deletes`/`reorder`, `returns`, `handler`, `emits`, `triggers transition`, `invalidates`, `calls`, `timeout`, `retry`, `idempotency by`, `write_window`, or `tests`",
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
            reorder,
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

/// W4 GAP-REORDER-01 — parse a `reorder <Resource> by <position_field>`
/// command body. Single-line; the resource name may be qualified
/// (`feature.Resource`). The `by <field>` clause is required and names the
/// integer position column that the batch UPDATE rewrites.
fn parse_command_reorder(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandReorderDecl, ParseError> {
    let rest = rest.trim();
    let Some((resource, by)) = rest.split_once(" by ") else {
        return Err(line_error(
            line,
            "`reorder` requires `reorder <Resource> by <position_field>`",
        ));
    };
    let resource = resource.trim();
    let position_field = by.trim();
    if resource.is_empty() {
        return Err(line_error(line, "`reorder` requires a resource name"));
    }
    if position_field.is_empty() || position_field.split_whitespace().count() != 1 {
        return Err(line_error(
            line,
            "`reorder <Resource> by` requires exactly one position field name",
        ));
    }
    Ok(CommandReorderDecl {
        resource: resource.to_owned(),
        position_field: position_field.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

// =============================================================================
// `deprecated` block tests — both `command <name>` and `api <name>` share
// the same parser shape, so the test module sits with the command parser
// (the api parser delegates to the same helper).
// =============================================================================
