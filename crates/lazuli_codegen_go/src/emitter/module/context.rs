//! `EmitContext` + `GoSourceContext` — the per-callable context bundle
//! threaded into every Go emitter. Carries the source map, the active
//! feature/capsule name, plus the PG.C.1 gate directives so individual
//! emitters can issue the runtime gate prelude without re-resolving the
//! plan-gate facts.
//!
//! `ResolvedLoc` + `resolve_source_loc` ride along as the small
//! source-map -> (file, line, column) helper consumed only by
//! `EmitContext::emit_line_directive` and `EmitContext::source_loc_string`.
//!
//! Wave R7-3 extract: lifted out of `module/mod.rs`.

use std::collections::BTreeMap;

use lazuli_ir::{FileId, Gate, SourceMap, SpanRef};

use super::super::printer::GoPrinter;

pub struct GoSourceContext<'a> {
    pub source_map: &'a SourceMap,
    pub feature_file_ids: &'a BTreeMap<String, FileId>,
}

pub struct EmitContext<'a> {
    pub source_map: Option<&'a SourceMap>,
    pub current_file_id: Option<FileId>,
    pub generated_path: &'a str,
    pub capsule_name: &'a str,
    pub current_feature: &'a str,
    /// PG.C.1 — per-callable gate directives. Populated when the caller
    /// provides `GoEmitOptions.plan_gate`; lookups go through
    /// `gates_for(<callable_kind>, <callable_name>)`. `None` when no
    /// plan-gate facts were threaded — emitters emit no prelude.
    pub gates: Option<&'a BTreeMap<String, Vec<Gate>>>,
}

impl<'a> EmitContext<'a> {
    pub fn no_source(generated_path: &'a str) -> Self {
        Self {
            source_map: None,
            current_file_id: None,
            generated_path,
            capsule_name: "",
            current_feature: "",
            gates: None,
        }
    }

    pub fn for_feature(
        source_context: Option<&'a GoSourceContext<'a>>,
        capsule_name: &'a str,
        feature_name: &'a str,
        generated_path: &'a str,
    ) -> Self {
        Self {
            source_map: source_context.map(|ctx| ctx.source_map),
            current_file_id: source_context
                .and_then(|ctx| ctx.feature_file_ids.get(feature_name))
                .copied(),
            generated_path,
            capsule_name,
            current_feature: feature_name,
            gates: None,
        }
    }

    /// PG.C.1 — look up the gate directives declared on a callable.
    /// `callable_kind` is the on-disk authoring kind (`command`,
    /// `query.list`, `query.lookup`, `query.sql`, `job`, `webhook`,
    /// `api`). Returns an empty slice when no gates apply (the common
    /// case: most callables are not gated).
    pub fn gates_for(&self, callable_kind: &str, callable_name: &str) -> &'a [Gate] {
        let Some(map) = self.gates else {
            return &[];
        };
        let key = format!(
            "{}/{}:{}",
            self.current_feature, callable_kind, callable_name
        );
        map.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// PG.C.1 — fluent setter so call sites can chain `EmitContext::for_feature(...).with_gates(...)`.
    pub fn with_gates(mut self, gates: Option<&'a BTreeMap<String, Vec<Gate>>>) -> Self {
        self.gates = gates;
        self
    }

    pub fn emit_line_directive(&self, p: &mut GoPrinter, span: Option<SpanRef>) -> bool {
        let (Some(source_map), Some(file_id), Some(span)) =
            (self.source_map, self.current_file_id, span)
        else {
            return false;
        };
        let Some(loc) = resolve_source_loc(source_map, file_id, span) else {
            return false;
        };
        p.line_directive(&loc.file, loc.line, loc.column);
        true
    }

    pub fn reset_line_directive(&self, p: &mut GoPrinter, emitted: bool) {
        if emitted {
            p.line_directive(self.generated_path, 1, 1);
        }
    }

    pub fn source_tag_literal(&self, kind: &str, op: &str, span: Option<SpanRef>) -> String {
        let source = self.source_loc_string(span).unwrap_or_else(String::new);
        format!(
            "lazuli.SourceTag{{Capsule: {:?}, Feature: {:?}, Kind: {:?}, Op: {:?}, Source: {:?}}}",
            self.capsule_name, self.current_feature, kind, op, source
        )
    }

    pub fn emit_with_source_field(
        &self,
        p: &mut GoPrinter,
        kind: &str,
        op: &str,
        span: Option<SpanRef>,
    ) {
        p.line("WithSource: func(ctx context.Context) context.Context {");
        p.indent();
        p.line("ctx = lazuli.WithSource(ctx, lazuli.SourceTag{");
        p.indent();
        p.line(&format!("Capsule: {:?},", self.capsule_name));
        p.line(&format!("Feature: {:?},", self.current_feature));
        p.line(&format!("Kind:    {:?},", kind));
        p.line(&format!("Op:      {:?},", op));
        p.line(&format!(
            "Source:  {:?},",
            self.source_loc_string(span).unwrap_or_else(String::new)
        ));
        p.dedent();
        p.line("})");
        p.line("return ctx");
        p.dedent();
        p.line("},");
    }

    fn source_loc_string(&self, span: Option<SpanRef>) -> Option<String> {
        let (Some(source_map), Some(file_id), Some(span)) =
            (self.source_map, self.current_file_id, span)
        else {
            return None;
        };
        let loc = resolve_source_loc(source_map, file_id, span)?;
        Some(format!("{}:{}:{}", loc.file, loc.line, loc.column))
    }
}

pub(super) struct ResolvedLoc {
    file: String,
    line: u32,
    column: u32,
}

pub(super) fn resolve_source_loc(
    source_map: &SourceMap,
    file_id: FileId,
    span: SpanRef,
) -> Option<ResolvedLoc> {
    let file = source_map.files.iter().find(|file| file.id == file_id)?;
    let source_len = file.line_offsets.last().copied()?;
    let start = u32::try_from(span.start).ok()?;
    if start > source_len {
        return None;
    }

    let searchable_offsets = if file.line_offsets.len() > 1 {
        &file.line_offsets[..file.line_offsets.len() - 1]
    } else {
        &file.line_offsets[..]
    };

    let line_idx = match searchable_offsets.binary_search(&start) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line_start = searchable_offsets.get(line_idx)?;
    Some(ResolvedLoc {
        file: file.path.clone(),
        line: (line_idx + 1) as u32,
        column: start - line_start + 1,
    })
}
