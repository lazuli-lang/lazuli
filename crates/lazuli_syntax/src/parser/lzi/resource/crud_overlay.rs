//! Spec 0018 — `crud` overlay block parser.
//!
//! Parses the `crud` child block authored under a `conventions [crud]`
//! resource:
//!
//! ```text
//! resource Customer
//!   conventions [crud]
//!   crud
//!     create
//!       policy @policy.edit
//!       validate @validator.percentage
//!       input excludes situation, is_active, is_defaulter
//!       assign situation = prospect
//!       assign is_active = true
//!       assign category = input.category_id
//!       emits customer_created
//!     update
//!       policy @policy.edit
//!     delete
//!       policy @policy.remove
//! ```
//!
//! The block is **analyzer-only**: the conventions synth pass merges it
//! into the synthesized `create_<r>` / `update_<r>` / `delete_<r>`
//! commands before lowering. It never reaches `ir::Resource` (no new IR
//! resource field, no struct-literal ripple). Absent block = today's bare
//! synth, byte-identical.
//!
//! Indentation contract (relative to the `resource` header indent `R`):
//!
//! * `crud`              at `R + 2`  (resource-body child; dispatched in `mod.rs`).
//! * `create`/`update`/`delete` sub-headers at `R + 4`.
//! * clauses (`policy` / `validate` / `input excludes` / `assign` / `emits`)
//!   at `R + 6`.
//!
//! `assign <field> = <expr>` reuses the SAME RHS grammar the hand-rolled
//! `creates`/`updates` effect assignment block accepts — captured as raw
//! text the analyzer lowers via `lower_raw_expr` (no re-implementation).

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::super::error::ParseError;

use crate::ast::{AssignmentDecl, CrudEffectOverlayAst, CrudOverlayAst, Span};

/// Parse the `crud` overlay block. `start` indexes the `crud` header
/// line; `header_indent` is the resource header's indent (so sub-headers
/// land at `header_indent + 4`, clauses at `header_indent + 6`). Returns
/// the parsed overlay and the index of the first unconsumed line.
pub(super) fn parse_resource_crud_overlay(
    lines: &[SourceLine<'_>],
    start: usize,
    header_indent: usize,
) -> Result<(CrudOverlayAst, usize), ParseError> {
    let header = &lines[start];
    let crud_indent = header.indent;
    let effect_indent = header_indent + 4;
    let clause_indent = header_indent + 6;

    let mut overlay = CrudOverlayAst {
        create: None,
        update: None,
        delete: None,
        span: Span::new(header.start, header.end),
    };
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        // Anything at or shallower than the `crud` header ends the block.
        if line.indent <= crud_indent {
            break;
        }
        if line.indent != effect_indent {
            return Err(line_error(
                line,
                "`crud` overlay sub-blocks (`create`/`update`/`delete`) use \
                 one indentation level deeper than the `crud` header",
            ));
        }
        let effect_kind = match trimmed {
            "create" => CrudEffectKind::Create,
            "update" => CrudEffectKind::Update,
            "delete" => CrudEffectKind::Delete,
            other => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "`crud` overlay sub-blocks are `create`, `update`, or `delete` (got `{other}`)"
                    ),
                ));
            }
        };
        if overlay.has(effect_kind) {
            return Err(line_error_owned(
                line,
                format!(
                    "duplicate `{}` sub-block in the `crud` overlay — declare each effect at most once",
                    effect_kind.label()
                ),
            ));
        }
        let (effect, next) = parse_effect_overlay(lines, i, clause_indent)?;
        overlay.set(effect_kind, effect);
        last_end = lines[next.saturating_sub(1).max(i)].end;
        i = next;
    }

    if overlay.create.is_none() && overlay.update.is_none() && overlay.delete.is_none() {
        return Err(line_error(
            header,
            "`crud` overlay requires at least one `create`/`update`/`delete` sub-block — \
             omit the `crud` block entirely if there is no per-effect overlay",
        ));
    }

    overlay.span = Span::new(header.start, last_end);
    Ok((overlay, i))
}

#[derive(Clone, Copy)]
enum CrudEffectKind {
    Create,
    Update,
    Delete,
}

impl CrudEffectKind {
    fn label(self) -> &'static str {
        match self {
            CrudEffectKind::Create => "create",
            CrudEffectKind::Update => "update",
            CrudEffectKind::Delete => "delete",
        }
    }
}

impl CrudOverlayAst {
    fn has(&self, kind: CrudEffectKind) -> bool {
        match kind {
            CrudEffectKind::Create => self.create.is_some(),
            CrudEffectKind::Update => self.update.is_some(),
            CrudEffectKind::Delete => self.delete.is_some(),
        }
    }
    fn set(&mut self, kind: CrudEffectKind, effect: CrudEffectOverlayAst) {
        match kind {
            CrudEffectKind::Create => self.create = Some(effect),
            CrudEffectKind::Update => self.update = Some(effect),
            CrudEffectKind::Delete => self.delete = Some(effect),
        }
    }
}

/// Parse one `create`/`update`/`delete` sub-block's clauses. `start`
/// indexes the sub-header; clauses live at `clause_indent`.
fn parse_effect_overlay(
    lines: &[SourceLine<'_>],
    start: usize,
    clause_indent: usize,
) -> Result<(CrudEffectOverlayAst, usize), ParseError> {
    let header = &lines[start];
    let mut effect = CrudEffectOverlayAst {
        policy: None,
        validate: Vec::new(),
        input_excludes: Vec::new(),
        assigns: Vec::new(),
        emits: Vec::new(),
        span: Span::new(header.start, header.end),
    };
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != clause_indent {
            return Err(line_error(
                line,
                "`crud` overlay clauses use one indentation level deeper than the \
                 `create`/`update`/`delete` sub-header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(line, "`policy` requires a policy reference"));
            }
            if effect.policy.is_some() {
                return Err(line_error(
                    line,
                    "a `crud` overlay sub-block may declare at most one `policy`",
                ));
            }
            effect.policy = Some(value.to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("validate ") {
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(
                    line,
                    "`validate` requires a validator reference",
                ));
            }
            effect.validate.push(value.to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("input ") {
            let rest = rest.trim();
            let Some(list) = rest.strip_prefix("excludes ") else {
                return Err(line_error(
                    line,
                    "`crud` overlay `input` clause must be `input excludes <field>, ...`",
                ));
            };
            let mut any = false;
            for raw in list.split(',') {
                let field = raw.trim();
                if field.is_empty() {
                    continue;
                }
                effect.input_excludes.push(field.to_owned());
                any = true;
            }
            if !any {
                return Err(line_error(
                    line,
                    "`input excludes` requires at least one field name",
                ));
            }
        } else if let Some(rest) = trimmed.strip_prefix("assign ") {
            let rest = rest.trim();
            let (field, value) = rest
                .split_once('=')
                .ok_or_else(|| line_error(line, "`assign` requires `<field> = <expr>`"))?;
            let field = field.trim();
            let value = value.trim();
            if field.is_empty() {
                return Err(line_error(
                    line,
                    "`assign` requires a field name before `=`",
                ));
            }
            if value.is_empty() {
                return Err(line_error(
                    line,
                    "`assign` requires an expression after `=`",
                ));
            }
            effect.assigns.push(AssignmentDecl {
                field: field.to_owned(),
                value: value.to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            // Allow comma-separated `emits a, b` parity with the command form.
            for raw in value.split(',') {
                let name = raw.trim();
                if !name.is_empty() {
                    effect.emits.push(name.to_owned());
                }
            }
        } else {
            return Err(line_error(
                line,
                "`crud` overlay clauses are `policy`, `validate`, `input excludes`, \
                 `assign`, or `emits`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    effect.span = Span::new(header.start, last_end);
    Ok((effect, i))
}
