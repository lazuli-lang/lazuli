//! `tenant_migration <name>` block parser — sibling of `job` /
//! `webhook` / `notification` under `feature`.
//!
//! Extracted from the original monolithic `job.rs`. Body shape is
//! closed: `target query.*|command.*`, `axis <name>`,
//! `idempotency <path>`, `retry`, `timeout`, `handler`. Older
//! `target tenants <axis>` and `idempotency by <path>` spellings
//! stay accepted for compatibility.

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD};
use super::parse_job_retry;
use crate::ast::{JobRetry, Span, TenantMigration};

pub(in super::super) fn parse_tenant_migration(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(TenantMigration, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("tenant_migration ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| {
            line_error(
                header,
                "tenant_migration header must be `tenant_migration <name>`",
            )
        })?;
    if name.is_empty() {
        return Err(line_error(
            header,
            "tenant_migration header requires a name",
        ));
    }

    let mut target_ref: Option<String> = None;
    let mut target_axis: Option<String> = None;
    let mut legacy_target_tenants = false;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut timeout: Option<String> = None;
    let mut handler: Option<String> = None;
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
                "tenant_migration body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("target tenants ") {
            target_axis = Some(rest.trim().to_owned());
            legacy_target_tenants = true;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            let target = rest.trim();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`target` requires `query.<name>` or `command.<name>`",
                ));
            }
            target_ref = Some(target.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("axis ") {
            let axis = rest.trim();
            if axis.is_empty() {
                return Err(line_error(line, "`axis` requires a tenant axis name"));
            }
            target_axis = Some(axis.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "tenant_migration children are `target query.<name>|command.<name>`, `axis <name>`, `idempotency <path>`, `retry`, `timeout`, or `handler`",
            ));
        }
    }

    let target_axis = target_axis
        .ok_or_else(|| line_error(header, "`tenant_migration` requires `axis <name>`"))?;
    if target_ref.is_none() && !legacy_target_tenants {
        return Err(line_error(
            header,
            "`tenant_migration` requires `target query.<name>` or `target command.<name>`",
        ));
    }
    let handler = handler
        .ok_or_else(|| line_error(header, "`tenant_migration` requires `handler \"<path>\"`"))?;

    Ok((
        TenantMigration {
            name,
            target_ref,
            target_axis,
            idempotency_by,
            retry,
            timeout,
            handler,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
