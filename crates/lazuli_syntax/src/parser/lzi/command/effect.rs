//! `creates|updates|deletes <resource>`, `emits <event>`, `target
//! query.<name>(args)`, and `let <name> = <expr>` parsing.
//!
//! These four shapes share the "header + optional grandchild
//! assignment block" pattern, so they live together. `parse_command_
//! effect`, `parse_let_binding`, and `parse_target_expr` are
//! `pub(in crate::parser::lzi)` because `job.rs` reuses them for its
//! declarative-typed body grammar.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD, parse_named_args, split_call_signature,
};

use crate::ast::{
    AssignmentDecl, CommandEffectDecl, CommandEffectKindDecl, CommandEmit, LetBindingDecl, Span,
    TargetExprDecl,
};

/// `target query.<name>(args)` — single-line; args are name=expr pairs
/// inside the parens. The parser keeps the dotted query reference
/// verbatim so the analyzer's namespace resolver decides between
/// local/cross-feature.
pub(in crate::parser::lzi) fn parse_target_expr(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<TargetExprDecl, ParseError> {
    let rest = rest.trim();
    let (query_part, args_part) = split_call_signature(line, rest)?;
    let args = parse_named_args(line, args_part)?;
    Ok(TargetExprDecl {
        query: query_part.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

pub(in crate::parser::lzi) fn parse_let_binding(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LetBindingDecl, ParseError> {
    let rest = rest.trim();
    let (name, value) = rest.split_once('=').ok_or_else(|| {
        line_error(
            line,
            "`let` requires `<name> = <expr>` (e.g. `let resolved = user.query.by_id(id: input.id)`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`let` requires a binding name before `=`"));
    }
    Ok(LetBindingDecl {
        name: name.to_owned(),
        value: value.trim().to_owned(),
        span: Span::new(line.start, line.end),
    })
}

/// Parse the `creates X`, `updates X`, `deletes X` family. Children at
/// AGENT_INDENT_GRANDCHILD (6) are `<field> = <expr>` assignments. The
/// `from input` shorthand collapses into `from_input: true` with no
/// assignment block.
pub(in crate::parser::lzi) fn parse_command_effect(
    lines: &[SourceLine<'_>],
    start: usize,
    kind: CommandEffectKindDecl,
    rest: &str,
) -> Result<(CommandEffectDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (resource, from_input) = if let Some(res) = rest.strip_suffix(" from input") {
        (res.trim().to_owned(), true)
    } else {
        (rest.to_owned(), false)
    };
    if resource.is_empty() {
        return Err(line_error(
            header,
            "`creates`/`updates`/`deletes` requires a resource name",
        ));
    }
    let mut assignments: Vec<AssignmentDecl> = Vec::new();
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
                "command effect children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "command effect assignments use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "command effect assignment requires a field name before `=`",
            ));
        }
        assignments.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((
        CommandEffectDecl {
            kind,
            resource,
            from_input,
            assignments,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `emits <event>[, <event>…]` line. Recognises a trailing
/// ` from creates` / ` from updates` / ` from deletes` axis that applies
/// to every name on the line. A single line may declare several events
/// separated by `,` (`emits paid, status_changed`); each becomes its own
/// [`CommandEmit`], so the caller `.extend`s rather than `.push`es — see
/// the sibling `parse_invalidates_block`, which returns many the same way.
///
/// An optional child block (six-space indent, `<key> = <expr>` payload
/// binds) is only legal on a **single-name** `emits`: with several names
/// it is ambiguous which event the binds populate, so a multi-name line
/// carrying a child block is a parse error rather than a silent guess.
pub(in crate::parser::lzi) fn parse_command_emit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(Vec<CommandEmit>, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (names_part, from) = if let Some(n) = rest.strip_suffix(" from creates") {
        (n.trim(), Some(CommandEffectKindDecl::Creates))
    } else if let Some(n) = rest.strip_suffix(" from updates") {
        (n.trim(), Some(CommandEffectKindDecl::Updates))
    } else if let Some(n) = rest.strip_suffix(" from deletes") {
        (n.trim(), Some(CommandEffectKindDecl::Deletes))
    } else {
        (rest, None)
    };
    if names_part.is_empty() {
        return Err(line_error(header, "`emits` requires an event name"));
    }
    let names: Vec<String> = names_part
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        // Only commas / whitespace after the `emits` keyword.
        return Err(line_error(header, "`emits` requires an event name"));
    }
    let mut fields: Vec<AssignmentDecl> = Vec::new();
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
                "`emits` children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "`emits` field children use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "`emits` field child requires a field name before `=`",
            ));
        }
        fields.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    if names.len() > 1 && !fields.is_empty() {
        return Err(line_error(
            header,
            "a multi-name `emits` (e.g. `emits a, b`) cannot carry a payload child block — \
             give each event its own `emits` line to bind fields",
        ));
    }
    let span = Span::new(header.start, header.end);
    let emits = names
        .into_iter()
        .map(|name| CommandEmit {
            name,
            from,
            fields: fields.clone(),
            span,
        })
        .collect();
    Ok((emits, i))
}
