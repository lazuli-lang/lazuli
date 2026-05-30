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

/// Optional source-map bundle threaded from the CLI into the emitter.
///
/// When present, line directives and `SourceTag` fields can refer back
/// to the original `.lzi` location; when absent, the emitter still
/// produces valid Go but omits source attribution.
pub struct GoSourceContext<'a> {
    /// Shared source map for every `.lzi` file in the module.
    pub source_map: &'a SourceMap,
    /// Feature name → `FileId` so the per-feature emitter can pick the
    /// right file out of the source map.
    pub feature_file_ids: &'a BTreeMap<String, FileId>,
}

/// Per-callable emit context.
///
/// Carries the source map handle, the active feature/capsule name, the
/// output path (for line-directive reset), and the PG.C.1 gate table so
/// individual emitters can issue the runtime gate prelude without
/// re-resolving plan-gate facts. Built once per generated file and
/// passed by reference to every walker.
pub struct EmitContext<'a> {
    /// Source map for the whole module, when threaded.
    pub source_map: Option<&'a SourceMap>,
    /// `FileId` of the `.lzi` that produced this output, when known.
    pub current_file_id: Option<FileId>,
    /// Workspace-rooted path of the file currently being emitted.
    pub generated_path: &'a str,
    /// Active capsule (project) name.
    pub capsule_name: &'a str,
    /// Active feature name.
    pub current_feature: &'a str,
    /// PG.C.1 — per-callable gate directives. Populated when the caller
    /// provides `GoEmitOptions.plan_gate`; lookups go through
    /// `gates_for(<callable_kind>, <callable_name>)`. `None` when no
    /// plan-gate facts were threaded — emitters emit no prelude.
    pub gates: Option<&'a BTreeMap<String, Vec<Gate>>>,
}

impl<'a> EmitContext<'a> {
    /// Construct a source-less context for emitters that don't have a
    /// `.lzi` to map back to (root files, helper templates).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let ctx = EmitContext::no_source("dist/go/main.go");
    /// assert_eq!(ctx.generated_path, "dist/go/main.go");
    /// ```
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

    /// Construct a context for one feature's emit pass.
    ///
    /// Picks the right `FileId` out of the source context (when present)
    /// so `emit_line_directive` resolves spans against the correct
    /// `.lzi`. Gates default to `None`; chain
    /// [`EmitContext::with_gates`] to wire PG.C.1 facts.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let ctx = EmitContext::for_feature(None, "myapp", "billing", "dist/go/billing.gen.go");
    /// assert_eq!(ctx.current_feature, "billing");
    /// ```
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
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let gates = ctx.gates_for("command", "createBilling");
    /// assert!(gates.is_empty()); // typical un-gated callable
    /// ```
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
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let ctx = EmitContext::for_feature(None, "myapp", "billing", "dist/go/billing.gen.go")
    ///     .with_gates(Some(&gates));
    /// ```
    pub fn with_gates(mut self, gates: Option<&'a BTreeMap<String, Vec<Gate>>>) -> Self {
        self.gates = gates;
        self
    }

    /// Emit a `//line` directive resolving `span` to a `.lzi` location.
    /// Returns `true` when a directive was actually emitted (so the
    /// caller can later call [`EmitContext::reset_line_directive`] to
    /// hop back to the generated file).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let mut p = GoPrinter::new();
    /// if ctx.emit_line_directive(&mut p, Some(span)) {
    ///     /* body */
    ///     ctx.reset_line_directive(&mut p, true);
    /// }
    /// ```
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

    /// Restore the line cursor to the generated file after an
    /// `emit_line_directive` excursion. No-op when `emitted` is `false`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let emitted = ctx.emit_line_directive(&mut p, span);
    /// /* ... write body ... */
    /// ctx.reset_line_directive(&mut p, emitted);
    /// ```
    pub fn reset_line_directive(&self, p: &mut GoPrinter, emitted: bool) {
        if emitted {
            p.line_directive(self.generated_path, 1, 1);
        }
    }

    /// Build a `lazuli.SourceTag{...}` literal carrying the
    /// capsule/feature/kind/op plus the resolved `file:line:col` source
    /// pointer. The string is suitable for inlining inside a Go struct
    /// composite literal.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// let lit = ctx.source_tag_literal("command", "createBilling", Some(span));
    /// assert!(lit.starts_with("lazuli.SourceTag{"));
    /// ```
    pub fn source_tag_literal(&self, kind: &str, op: &str, span: Option<SpanRef>) -> String {
        let source = self.source_loc_string(span).unwrap_or_default();
        format!(
            "lazuli.SourceTag{{Capsule: {:?}, Feature: {:?}, Kind: {:?}, Op: {:?}, Source: {:?}}}",
            self.capsule_name, self.current_feature, kind, op, source
        )
    }

    /// Emit a `WithSource: func(ctx) ...` field that pushes a Lazuli
    /// `SourceTag` onto the context. Generators bind this into the
    /// `lazuli.OperationOpts` literal so runtime spans carry the right
    /// authoring location.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// ctx.emit_with_source_field(&mut p, "command", "createBilling", Some(span));
    /// // emitter writes `WithSource: func(ctx context.Context) context.Context { ... }`
    /// ```
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
            self.source_loc_string(span).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_source_context_carries_empty_capsule_and_no_gates() {
        let ctx = EmitContext::no_source("dist/go/main.go");
        assert_eq!(ctx.generated_path, "dist/go/main.go");
        assert_eq!(ctx.capsule_name, "");
        assert!(ctx.source_map.is_none());
        assert!(ctx.gates.is_none());
        // No gates threaded -> gates_for returns an empty slice for any
        // (kind, name) pair.
        assert!(ctx.gates_for("command", "anything").is_empty());
    }
}
