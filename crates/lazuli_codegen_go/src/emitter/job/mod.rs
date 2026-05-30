//! Cell G2a - `Job` kind emission. Walks every `Job` declared on a
//! feature and emits a `jobs.JobContract` value into
//! `<feature>/job.gen.go`.
//!
//! Proposal references:
//! - 3.6 - `jobs.JobContract` shape.
//! - 4 - runtime-gap discipline: when the Lazuli Go lib does not
//!   expose a field for captured IR intent, keep the output Go-valid
//!   and surface a `// TODO(runtime): ...` comment inside the value
//!   literal.
//!
//! Determinism: jobs are sorted by name before emission; nested
//! repeated entries that are semantically set-like are sorted before
//! rendering.
//!
//! ## Module layout
//!
//! - `format` — small pure renderers (qname / path / expr / casing
//!   / duration / policy / effect / banner).
//! - `slots` — per-slot field emitters (`TenantFrom`, `Fanout`,
//!   `Idempotency`, `Retry`, `ExternalCalls`, `Emits`, gate
//!   `Prelude`).
//! - `runtime_gaps` — `// TODO(runtime):` comment emitters that
//!   surface IR intent the runtime contract hasn't expressed yet.
//!
//! Split out from a single flat `job.rs` (557 prod LOC) in
//! rails-style R7-4.

use lazuli_ir::{Feature, Job, JobBody};

use super::cross_feature::CrossFeatureIndex;
use super::error_envelope::{
    bucket_names_for_external_calls, emit_wrap_helper_named, sentinel_buckets,
};
use super::imports::ImportSet;
use super::module::EmitContext;
use super::patterns::{PATTERN_JOB_HANDLER, emit_pattern_header};
use super::printer::GoPrinter;
use super::types::TypeCtx;

mod format;
mod runtime_gaps;
mod slots;

use format::{
    escape_comment, escape_string, format_trigger, job_var_name, policy_string, timeout_expr,
    write_section_banner,
};
use runtime_gaps::{effective_policy, emit_body_runtime_gaps};
use slots::{
    emit_emits, emit_external_calls, emit_fanout, emit_gate_annotations, emit_idempotency,
    emit_retry, emit_tenant_from,
};

/// Emit `<feature>/job.gen.go` for a feature, or `None` when the
/// feature declares no jobs.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_job_file("billing.lzi", &feature, "demo", &cross_index, &emit_ctx);
/// ```
pub fn emit_job_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.jobs.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/jobs");

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut jobs: Vec<&Job> = feature.jobs.iter().collect();
    jobs.sort_by(|a, b| a.name.cmp(&b.name));
    let wrap_buckets = job_wrap_buckets(&jobs);

    for job in &jobs {
        if timeout_expr(job.timeout.as_deref()).is_some() {
            imports.add("time");
        }
    }
    if !wrap_buckets.is_empty() {
        imports.add("errors");
        imports.add("lazuli.dev/runtime/lazuli/auth");
    }
    // PG.C.2 — gated jobs carry a `Prelude: []billing.GateRef{...}`
    // field on the JobContract value; the runtime dispatcher
    // (`DispatchJob` / River worker) evaluates it before invoking
    // the user handler. Import `billing` only when any job in the
    // file declares gates.
    let any_gated = jobs
        .iter()
        .any(|job| !emit_ctx.gates_for("job", &job.name).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();
    if !wrap_buckets.is_empty() {
        emit_wrap_helper_named(&mut p, "wrapErrorForJobHandler", &wrap_buckets);
        p.blank();
    }

    let mut first_block = true;
    for job in &jobs {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_job(&mut p, feature, job, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

fn job_wrap_buckets(jobs: &[&Job]) -> std::collections::BTreeSet<&'static str> {
    let referenced: std::collections::BTreeSet<&str> = jobs
        .iter()
        .flat_map(|job| bucket_names_for_external_calls(&job.external_calls))
        .collect();
    sentinel_buckets(&referenced)
}

fn emit_job(
    p: &mut GoPrinter,
    feature: &Feature,
    job: &Job,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.{}", feature.name, job.name);

    write_section_banner(
        p,
        &[
            format!("Job: {qualified_name}"),
            format!("  job {}", job.name),
        ],
    );

    emit_pattern_header(p, PATTERN_JOB_HANDLER);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, job.span_ref);
    p.line(&format!(
        "var {} = jobs.JobContract{{",
        job_var_name(&feature.name, &job.name)
    ));
    p.indent();

    let policy = effective_policy(job, feature);
    let mut kv_rows: Vec<(String, String)> = vec![
        (
            "Feature:".to_owned(),
            format!("\"{}\",", escape_string(&feature.name)),
        ),
        (
            "Name:".to_owned(),
            format!("\"{}\",", escape_string(&job.name)),
        ),
        ("Trigger:".to_owned(), format_trigger(feature, &job.trigger)),
    ];
    if let Some(queue) = &job.queue {
        kv_rows.push((
            "Queue:".to_owned(),
            format!("\"{}\",", escape_string(queue)),
        ));
    }
    if let Some(policy) = policy {
        kv_rows.push((
            "Policy:".to_owned(),
            format!("\"{}\",", policy_string(policy)),
        ));
    }
    if let Some(timeout) = timeout_expr(job.timeout.as_deref()) {
        kv_rows.push(("Timeout:".to_owned(), format!("{timeout},")));
    }
    if let JobBody::Handler(handler) = &job.body {
        kv_rows.push((
            "HandlerPath:".to_owned(),
            format!("\"{}\",", escape_string(&handler.path.path)),
        ));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "job", &job.name, job.span_ref);
    emit_gate_annotations(p, emit_ctx.gates_for("job", &job.name));

    if let Some(tenant_from) = &job.tenant_from {
        emit_tenant_from(p, tenant_from);
    }
    if let Some(fanout) = &job.fanout {
        emit_fanout(p, fanout);
    }
    if let Some(idempotency) = &job.idempotency {
        emit_idempotency(p, idempotency);
    }
    if let Some(retry) = &job.retry {
        emit_retry(p, retry);
    }
    if !job.external_calls.is_empty() {
        emit_external_calls(p, &job.external_calls);
    }
    if !job.emits.is_empty() {
        emit_emits(p, &job.emits);
    }
    if job.timeout.is_some() && timeout_expr(job.timeout.as_deref()).is_none() {
        p.line(&format!(
            "// TODO(runtime): JobContract.Timeout is time.Duration; cannot preserve authored duration \"{}\" without a parser helper.",
            escape_comment(job.timeout.as_deref().unwrap_or_default())
        ));
    }
    emit_body_runtime_gaps(p, &job.body, ctx);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

#[cfg(test)]
mod feature_emit_tests;
