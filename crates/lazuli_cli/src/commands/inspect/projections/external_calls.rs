//! `--expand=external_calls` projection.
//!
//! Walks every `command <name>` and `job <name>` top-level block on a
//! feature and surfaces the `calls <slot>.<operation>` rows declared
//! inside, along with the surrounding `timeout` / `retry` /
//! `idempotency by ...` / `audit required` envelope. Each call carries
//! the typed argument list (declared as `<key> = <value>` lines under
//! the call) so consumers can reason about the runtime contract
//! without parsing the source again.

use super::super::InspectCallArg;
use super::super::InspectExternalCall;
use super::super::expand::{is_identifier, leading_spaces};
use super::super::text_walkers::{
    block_has_exact_line, block_prefixed_value, block_scalar_value, named_block_name, strip_quotes,
};

pub(in crate::commands::inspect) fn inspect_external_calls(
    feature: &str,
    lines: &[String],
) -> Vec<InspectExternalCall> {
    let mut calls = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let leading = leading_spaces(&lines[index]);

        if leading == 2 && (trimmed.starts_with("command ") || trimmed.starts_with("job ")) {
            let (kind, name) = if let Some(name) = named_block_name(trimmed, "command") {
                ("command", name)
            } else if let Some(name) = named_block_name(trimmed, "job") {
                ("job", name)
            } else {
                index += 1;
                continue;
            };

            let start = index;
            index += 1;
            while index < lines.len() && leading_spaces(&lines[index]) > 2 {
                index += 1;
            }

            calls.extend(inspect_external_calls_in_block(
                feature,
                kind,
                name,
                &lines[start..index],
            ));
        } else {
            index += 1;
        }
    }

    calls
}

fn inspect_external_calls_in_block(
    feature: &str,
    kind: &str,
    name: &str,
    lines: &[String],
) -> Vec<InspectExternalCall> {
    let timeout = block_scalar_value(lines, "timeout").map(strip_quotes);
    let retry = block_scalar_value(lines, "retry").map(str::to_owned);
    let idempotency = block_prefixed_value(lines, "idempotency by ").map(str::to_owned);
    let audit = block_has_exact_line(lines, "audit required");
    let subject = format!("{feature}.{kind}.{name}");
    let mut calls = Vec::new();
    let mut index = 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();

        if leading_spaces(&lines[index]) == 4
            && let Some((slot, operation)) = parse_external_call_header(trimmed)
        {
            let mut args = Vec::new();
            index += 1;

            while index < lines.len() && leading_spaces(&lines[index]) > 4 {
                let child = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 6
                    && let Some((name, value)) = child.split_once('=')
                {
                    args.push(InspectCallArg {
                        name: name.trim().to_owned(),
                        value: value.trim().to_owned(),
                    });
                }
                index += 1;
            }

            calls.push(InspectExternalCall {
                subject: subject.clone(),
                slot: slot.to_owned(),
                operation: operation.to_owned(),
                args,
                timeout: timeout.clone(),
                retry: retry.clone(),
                idempotency: idempotency.clone(),
                audit,
                origin: "calls",
            });
        } else {
            index += 1;
        }
    }

    calls
}

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}
