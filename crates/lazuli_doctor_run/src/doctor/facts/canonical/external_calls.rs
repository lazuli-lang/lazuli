//! IR-driven `external_calls` fact populators for jobs + commands.
//!
//! Two helpers, both replacing the retired text-walker branches of
//! `collect_external_calls_in_block`. They read the typed
//! `lazuli_ir::Job` / `lazuli_ir::Command` shape (timeouts, retries,
//! idempotency, span_ref anchors) and emit `ExternalCallFact` rows
//! with the typed `has_timeout` / `has_retry` / `has_idempotency`
//! axes the doctor's `INT-CALL-*` rules consume.
//!
//! Lifted out of the `canonical` god-file in the rails-style R9 split.

use crate::doctor::scanners::leading_spaces;
use crate::doctor::{
    DoctorFile, ExternalCallFact, OperationalFacts, collect_construct_lines, line_col_for_offset,
};

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `job` branch of `collect_external_calls_in_block`. Walks each
/// `job.external_calls` entry, reads `ExternalCallRef.span_ref` to
/// anchor the diagnostic at the `calls <slot>.<op>` line, and emits an
/// `ExternalCallFact` carrying the typed `has_timeout` / `has_retry` /
/// `has_idempotency` axes lifted from the `Job` IR. The job branch of
/// the legacy text walker is gone.
pub(crate) fn populate_job_external_calls_from_ir(
    file: &DoctorFile,
    feature: &lazuli_ir::Feature,
    operational: &mut OperationalFacts,
) {
    if feature.jobs.is_empty() {
        return;
    }
    let job_lines = collect_construct_lines(
        &file.source,
        "job ",
        feature.jobs.iter().map(|j| j.name.as_str()).collect(),
    );
    for job in &feature.jobs {
        if job.external_calls.is_empty() {
            continue;
        }
        let header_line = job_lines.get(&job.name).copied().unwrap_or(1);
        let has_timeout = job.timeout.is_some();
        let has_retry = job.retry.is_some();
        let has_idempotency = job.idempotency.is_some();
        let subject = format!("{}.job.{}", feature.name, job.name);
        for call in &job.external_calls {
            let (call_line, call_column) = match call.span_ref.as_ref() {
                Some(span) => {
                    let (line, col) = line_col_for_offset(&file.source, span.start);
                    (line, col)
                }
                None => (header_line, 1),
            };
            operational.external_calls.push(ExternalCallFact {
                path: file.path.clone(),
                line: call_line,
                column: call_column,
                feature: feature.name.clone(),
                subject_kind: "job".to_owned(),
                subject: subject.clone(),
                slot: call.slot.clone(),
                operation: call.op.clone(),
                has_timeout,
                has_retry,
                has_idempotency,
            });
        }
    }
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `command` branch of `collect_external_calls_in_block`. Walks each
/// `command.external_calls` entry, finds its `calls <slot>.<op>` line
/// in the source, and emits an `ExternalCallFact` carrying the typed
/// `has_timeout` / `has_retry` / `has_idempotency` axes lifted from the
/// `Command` IR. The line lookup is keyed on the verbatim `calls
/// <slot>.<op>` substring inside the command body's source range, so
/// the diagnostic anchors stay precise.
pub(crate) fn populate_command_external_calls_from_ir(
    file: &DoctorFile,
    feature: &lazuli_ir::Feature,
    operational: &mut OperationalFacts,
) {
    if feature.commands.is_empty() {
        return;
    }
    let command_lines = collect_construct_lines(
        &file.source,
        "command ",
        feature.commands.iter().map(|c| c.name.as_str()).collect(),
    );
    let source_lines: Vec<&str> = file.source.lines().collect();
    for command in &feature.commands {
        if command.external_calls.is_empty() {
            continue;
        }
        let header_line = command_lines
            .get(&command.name)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        // Block ends at the next top-level construct (indent <= 2).
        let mut block_end = header_line + 1;
        while block_end < source_lines.len() && leading_spaces(source_lines[block_end]) > 2 {
            block_end += 1;
        }
        let has_timeout = command.timeout.is_some();
        let has_retry = command.retry.is_some();
        let has_idempotency = command.idempotency.is_some();
        let subject = format!("{}.command.{}", feature.name, command.name);
        for call in &command.external_calls {
            let needle = format!("calls {}.{}", call.slot, call.op);
            let mut call_line = header_line + 1; // fall back to header
            let mut call_column = 1;
            for i in (header_line + 1)..block_end {
                if source_lines[i].trim_start().starts_with(needle.as_str()) {
                    call_line = i + 1;
                    call_column = leading_spaces(source_lines[i]) + 1;
                    break;
                }
            }
            operational.external_calls.push(ExternalCallFact {
                path: file.path.clone(),
                line: call_line,
                column: call_column,
                feature: feature.name.clone(),
                subject_kind: "command".to_owned(),
                subject: subject.clone(),
                slot: call.slot.clone(),
                operation: call.op.clone(),
                has_timeout,
                has_retry,
                has_idempotency,
            });
        }
    }
}
