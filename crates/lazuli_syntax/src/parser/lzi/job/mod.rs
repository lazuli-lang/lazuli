//! `job <name>` block parser and the shared `tenant_migration` / handler
//! / external-call helpers.
//!
//! All five sub-parsers exposed here serve more than one block:
//!
//! - `parse_job` — the `job <name>` block walker (feature child indent).
//! - `parse_job_trigger`, `parse_job_retry` — also consumed by
//!   `parse_webhook` and `parse_tenant_migration`.
//! - `parse_handler_line` — shared between `job`, `webhook`,
//!   `tenant_migration`, and any future construct with a
//!   `handler "<path>"` slot.
//! - `parse_external_call` — `calls <slot>.<op>` six-space child block.
//! - `parse_tenant_migration` — top-level migration declaration.
//!
//! ## Job body shapes
//!
//! Per `docs/proposals/phase-l-tier-3-job-effect-scope.md`, a job body
//! is one of:
//!
//! - `JobBody::Handler` — `handler "./path.go" returns Type`
//! - `JobBody::Declarative` — `target query.<name>(...)` + optional
//!   `let <name> = <expr>` lines + a single `creates`/`updates`/
//!   `deletes` effect block.
//! - `JobBody::None` — when neither is provided (e.g. fanout-only).
//!
//! `emits <event>` (with optional `from creates|updates|deletes` suffix
//! + indented payload child block) is parsed inline; only the event
//! names reach IR for Tier 3 — payload assignments stay on the source
//! surface for doctor diagnostics.
//!
//! ## See also
//!
//! - `docs/proposals/phase-l-tier-3-job-effect-scope.md`
//! - `docs/proposals/tenant-migration.md`
//! - `lazuli_ir::nodes::job` — typed lowering target.

mod tenant_migration;

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::command::{parse_command_effect, parse_let_binding, parse_target_expr};
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{
    CommandEffectDecl, CommandEffectKindDecl, Job, JobBody, JobDeclarativeTyped, JobExternalCall,
    JobExternalCallArg, JobFanout, JobHandler, JobRetry, JobTrigger, LetBindingDecl, PolicyExprAst,
    Span, TargetExprDecl,
};

pub(super) use tenant_migration::parse_tenant_migration;

pub(super) fn parse_job(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Job, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("job ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "job header must be `job <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "job header requires a name"));
    }

    let mut trigger: Option<JobTrigger> = None;
    let mut queue: Option<String> = None;
    let mut tenant_from: Option<String> = None;
    let mut fanout: Option<JobFanout> = None;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut timeout: Option<String> = None;
    let mut external_calls: Vec<JobExternalCall> = Vec::new();
    let mut handler: Option<JobHandler> = None;
    let mut declarative_target: Option<TargetExprDecl> = None;
    let mut declarative_lets: Vec<LetBindingDecl> = Vec::new();
    let mut declarative_effect: Option<CommandEffectDecl> = None;
    // `emits <event>` lines (with their optional indented payload child
    // block silently skipped — Tier 3 IR only carries event names).
    let mut emits: Vec<String> = Vec::new();
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
                "job body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("trigger ") {
            trigger = Some(parse_job_trigger(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("queue ") {
            queue = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("fanout ") {
            fanout = Some(parse_job_fanout(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("calls ") {
            let (call, next) = parse_external_call(lines, i, rest)?;
            external_calls.push(call);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(parse_handler_line(rest));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            declarative_target = Some(parse_target_expr(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            declarative_lets.push(parse_let_binding(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("creates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Creates, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("updates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Updates, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deletes ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Deletes, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            // Strip the optional ` from <kind>` axis, then split the
            // remaining `a, b` list into one event name apiece (mirrors
            // `command::parse_command_emit`). Only event names reach IR;
            // any indented payload child block stays on the source surface.
            let raw = rest.trim();
            let names_part = if let Some(n) = raw.strip_suffix(" from creates") {
                n.trim()
            } else if let Some(n) = raw.strip_suffix(" from updates") {
                n.trim()
            } else if let Some(n) = raw.strip_suffix(" from deletes") {
                n.trim()
            } else {
                raw
            };
            let names: Vec<&str> = names_part
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            last_end = line.end;
            i += 1;
            // Consume any indented payload child block (`<field> = <expr>`).
            let mut has_child_block = false;
            while i < lines.len() {
                let child = &lines[i];
                let child_trim = child.text.trim_start();
                if is_trivia(child_trim) {
                    i += 1;
                    continue;
                }
                if child.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                has_child_block = true;
                last_end = child.end;
                i += 1;
            }
            if names.len() > 1 && has_child_block {
                return Err(line_error(
                    line,
                    "a multi-name `emits` (e.g. `emits a, b`) cannot carry a payload child block — \
                     give each event its own `emits` line to bind fields",
                ));
            }
            emits.extend(names.into_iter().map(str::to_owned));
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "job children are `trigger`, `queue`, `tenant_from`, `fanout`, `idempotency by`, `retry`, `policy`, `timeout`, `calls`, `handler`, `target`, `let`, `updates`/`creates`/`deletes`, `emits`, or `gate behind/quota plan.*`",
            ));
        }
    }

    let trigger = trigger.ok_or_else(|| {
        line_error(
            header,
            "`job` requires a `trigger event ...` or `trigger schedule ...` declaration",
        )
    })?;

    let body = if let Some(handler) = handler {
        JobBody::Handler(handler)
    } else if declarative_target.is_some() || declarative_effect.is_some() {
        JobBody::Declarative(JobDeclarativeTyped {
            target: declarative_target,
            lets: declarative_lets,
            effect: declarative_effect,
        })
    } else {
        JobBody::None
    };

    Ok((
        Job {
            name,
            trigger,
            queue,
            tenant_from,
            fanout,
            idempotency_by,
            retry,
            policy,
            policy_expr,
            timeout,
            external_calls,
            body,
            emits,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

pub(super) fn parse_job_trigger(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<JobTrigger, ParseError> {
    let rest = rest.trim();
    if let Some(ev) = rest.strip_prefix("event ") {
        let ev = ev.trim();
        if ev.is_empty() {
            return Err(line_error(line, "`trigger event` requires an event name"));
        }
        return Ok(JobTrigger::Event(ev.to_owned()));
    }
    if let Some(cron) = rest.strip_prefix("schedule ") {
        let cron = cron.trim();
        if cron.is_empty() {
            return Err(line_error(
                line,
                "`trigger schedule` requires a quoted cron expression",
            ));
        }
        return Ok(JobTrigger::Schedule(unquote_lzx_value(cron).to_owned()));
    }
    Err(line_error(
        line,
        "`trigger` requires `event <name>` or `schedule \"<cron>\"`",
    ))
}

fn parse_job_fanout(line: &SourceLine<'_>, rest: &str) -> Result<JobFanout, ParseError> {
    let rest = rest.trim();
    let (scope, axis) = rest.split_once(' ').ok_or_else(|| {
        line_error(
            line,
            "`fanout` requires `<scope> <axis>`, e.g. `fanout tenants org`",
        )
    })?;
    Ok(JobFanout {
        scope: scope.to_owned(),
        axis: axis.trim().to_owned(),
    })
}

pub(super) fn parse_job_retry(line: &SourceLine<'_>, rest: &str) -> Result<JobRetry, ParseError> {
    let rest = rest.trim();
    let (count_str, tail) = rest.split_once(' ').ok_or_else(|| {
        line_error(
            line,
            "`retry` requires `<count> backoff <strategy>` (e.g. `retry 3 backoff exponential`)",
        )
    })?;
    let count = count_str
        .parse::<u32>()
        .map_err(|_| line_error(line, "retry count must be a non-negative integer"))?;
    let tail = tail.trim();
    let backoff = tail.strip_prefix("backoff ").ok_or_else(|| {
        line_error(
            line,
            "`retry` requires `<count> backoff <strategy>` (e.g. `retry 3 backoff exponential`)",
        )
    })?;
    Ok(JobRetry {
        count,
        backoff: backoff.trim().to_owned(),
    })
}

pub(super) fn parse_handler_line(rest: &str) -> JobHandler {
    let rest = rest.trim();
    // `"./path.go" returns Type` — split before the unquoted `returns`.
    let (path_part, returns_part) = if let Some(idx) = rest.find("\" returns ") {
        let end = idx + 1; // include closing quote
        (
            rest[..end].to_owned(),
            Some(rest[end + " returns ".len()..].trim().to_owned()),
        )
    } else {
        (rest.to_owned(), None)
    };
    JobHandler {
        path: unquote_lzx_value(path_part.trim()).to_owned(),
        returns: returns_part,
    }
}

pub(super) fn parse_external_call(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(JobExternalCall, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let (slot, op) = head.split_once('.').ok_or_else(|| {
        line_error(
            header,
            "`calls` requires `<slot>.<op>` (e.g. `calls crm.upsert_customer`)",
        )
    })?;
    let mut args: Vec<JobExternalCallArg> = Vec::new();
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
                "`calls` argument lines use six-space indentation",
            ));
        }

        let (name, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "`calls` argument lines must use `<name> = <expr>`"))?;
        args.push(JobExternalCallArg {
            name: name.trim().to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }

    Ok((
        JobExternalCall {
            slot: slot.trim().to_owned(),
            op: op.trim().to_owned(),
            args,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// =============================================================================
// Migrations bucket cycle — `tenant_migration` block parser slice tests.
// =============================================================================
#[cfg(test)]
mod tenant_migration_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn tenant_migration_full_block_parses() {
        let source = r#"
feature customer
  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org_id
    idempotency envelope.tenant_id
    timeout 10m
    retry 3 backoff exponential
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let tm = &features[0].tenant_migrations[0];
        assert_eq!(tm.name, "backfill_lifecycle_stage");
        assert_eq!(tm.target_ref.as_deref(), Some("query.by_id"));
        assert_eq!(tm.target_axis, "org_id");
        assert_eq!(tm.idempotency_by.as_deref(), Some("envelope.tenant_id"));
        assert_eq!(tm.timeout.as_deref(), Some("10m"));
        assert_eq!(tm.retry.as_ref().expect("retry").count, 3);
        assert_eq!(tm.handler, "./migrations/backfill_lifecycle_stage.go");
    }

    #[test]
    fn tenant_migration_legacy_target_and_idempotency_parses() {
        let source = r#"
feature customer
  tenant_migration backfill
    target tenants org
    idempotency by tenant.org_id
    handler "./migrations/backfill.go"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let tm = &features[0].tenant_migrations[0];
        assert!(tm.target_ref.is_none());
        assert_eq!(tm.target_axis, "org");
        assert_eq!(tm.idempotency_by.as_deref(), Some("tenant.org_id"));
    }

    #[test]
    fn tenant_migration_missing_axis_errors() {
        let source = r#"
feature customer
  tenant_migration backfill
    target query.by_id
    idempotency envelope.tenant_id
    handler "./migrations/backfill.go"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("axis <name>"), "got {message}");
    }

    #[test]
    fn tenant_migration_unknown_child_errors() {
        let source = r#"
feature customer
  tenant_migration backfill
    target query.by_id
    axis org
    emits changed
    handler "./migrations/backfill.go"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("tenant_migration children"),
            "got {message}"
        );
    }
}

#[cfg(test)]
mod emits_split_tests;
