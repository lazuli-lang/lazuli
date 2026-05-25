//! Runtime-gap discipline for job bodies.
//!
//! When the Lazuli Go lib exposes no field for a captured IR intent,
//! the emitter keeps the output Go-valid by surfacing a
//! `// TODO(runtime): ...` comment inside the value literal. Per
//! proposal 4, that comment is the structured evidence the
//! runtime/codegen feedback loop polls.
//!
//! - `emit_body_runtime_gaps` dispatches by body kind.
//! - `emit_handler_runtime_gaps` covers handler-style bodies (return
//!   types not yet expressible in `jobs.JobContract`).
//! - `emit_declarative_runtime_gaps` covers declarative bodies whose
//!   `target` / `let` / `effect` slots have no runtime contract slot
//!   yet.
//! - `effective_policy` picks the job's policy, falling back to the
//!   feature defaults but treating `policy_ref None` as unset.

use lazuli_ir::{Feature, Job, JobBody, JobDeclarative, JobHandler, PolicyRef};

use crate::emitter::printer::GoPrinter;
use crate::emitter::types::{self, TypeCtx};

use super::format::{effect_summary, escape_comment, format_qname};

pub(super) fn emit_body_runtime_gaps(p: &mut GoPrinter, body: &JobBody, ctx: &TypeCtx<'_>) {
    match body {
        JobBody::Handler(handler) => emit_handler_runtime_gaps(p, handler, ctx),
        JobBody::Declarative(declarative) => emit_declarative_runtime_gaps(p, declarative, ctx),
    }
}

fn emit_handler_runtime_gaps(p: &mut GoPrinter, handler: &JobHandler, ctx: &TypeCtx<'_>) {
    if let Some(return_type) = &handler.returns {
        let (go_type, _import) = types::go_type_for(return_type, ctx);
        p.line(&format!(
            "// TODO(runtime): handler returns {go_type}, but jobs.JobContract has no return type field."
        ));
    }
}

fn emit_declarative_runtime_gaps(
    p: &mut GoPrinter,
    declarative: &JobDeclarative,
    ctx: &TypeCtx<'_>,
) {
    p.line("// TODO(runtime): declarative job bodies are not represented in jobs.JobContract yet.");
    if let Some(target) = &declarative.target {
        p.line(&format!(
            "// TODO(runtime): preserve declarative target {}.",
            escape_comment(&format_qname(None, &target.query))
        ));
    }
    if !declarative.lets.is_empty() {
        let mut names: Vec<&str> = declarative
            .lets
            .iter()
            .map(|binding| binding.name.as_str())
            .collect();
        names.sort();
        p.line(&format!(
            "// TODO(runtime): preserve declarative let bindings {}.",
            escape_comment(&names.join(", "))
        ));
    }
    p.line(&format!(
        "// TODO(runtime): preserve declarative effect {}.",
        escape_comment(&effect_summary(&declarative.effect, ctx))
    ));
}

pub(super) fn effective_policy<'a>(job: &'a Job, feature: &'a Feature) -> Option<&'a PolicyRef> {
    job.policy
        .as_ref()
        .or(feature.defaults.policy.as_ref())
        .filter(|policy| !matches!(policy, PolicyRef::None))
}
