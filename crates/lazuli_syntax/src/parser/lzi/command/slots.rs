//! `route <name>: <Type>` slot parsing and `input { ... }` block / short
//! form parsing. These two surfaces share grammar shape (`<name>: <Type>
//! [required|optional]`) and live together so the indent-aware walkers
//! can call each other directly.
//!
//! `parse_command_route_slot` and `parse_command_input_block` are
//! `pub(super)` because `api.rs` reuses the same slot grammar for its
//! `api <name>` blocks. `split_command_input_modifiers` is reused by
//! `query.rs` for `params`-style input slot parsing.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::field_constraints::extract_field_constraints;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};

use crate::ast::{
    CommandInputDecl, CommandInputSlot, CommandRouteSlot, CommandRouteSlotKind, Span,
};

pub(in crate::parser::lzi) fn parse_command_route_slot(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandRouteSlot, ParseError> {
    let rest = rest.trim();
    if rest == "signed_token" {
        return Ok(CommandRouteSlot {
            name: "signed_token".to_owned(),
            type_text: "Text".to_owned(),
            from: None,
            kind: CommandRouteSlotKind::SignedToken,
            span: Span::new(line.start, line.end),
        });
    }
    let signed_token_rest;
    let (kind, rest) = if let Some(after) = rest.strip_prefix("opaque ") {
        (CommandRouteSlotKind::OpaqueToken, after.trim())
    } else if let Some(after) = rest.strip_prefix("signed_token:") {
        signed_token_rest = format!("signed_token:{}", after);
        (
            CommandRouteSlotKind::SignedToken,
            signed_token_rest.as_str(),
        )
    } else {
        (CommandRouteSlotKind::Plain, rest)
    };
    let (name, after) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`route` requires `<name>: <Type>` (e.g. `route id: ID`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`route` requires a slot name before `:`"));
    }
    let after = after.trim();
    let (type_text, from) = if let Some(idx) = after.find(" from ") {
        let from_expr = after[idx + " from ".len()..].trim().to_owned();
        (after[..idx].trim().to_owned(), Some(from_expr))
    } else {
        (after.to_owned(), None)
    };
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`route` requires a type after `:` (e.g. `ID`)",
        ));
    }
    Ok(CommandRouteSlot {
        name: name.to_owned(),
        type_text,
        from,
        kind,
        span: Span::new(line.start, line.end),
    })
}

pub(in crate::parser::lzi) fn parse_command_input_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandInputDecl, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
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
                "`command input` children use six-space indentation",
            ));
        }

        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "`command input` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a name before `:`",
            ));
        }
        let rest = type_part.trim();
        // L0 #3 §10 — peel inline constraints first so the residual
        // string is just `<Type> [required|optional]`. Constraint
        // combination rules are enforced in the analyzer.
        let (rest_after, constraints) = extract_field_constraints(line, rest)?;
        // Walk to find the `required` or `optional` token at the end,
        // honouring parenthesised type-arg lists.
        let (type_text, required, optional) = split_command_input_modifiers(&rest_after);
        if type_text.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a type after `:`",
            ));
        }
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((CommandInputDecl::Typed(slots), i))
}

pub(in crate::parser::lzi) fn split_command_input_modifiers(rest: &str) -> (String, bool, bool) {
    // Find the last whitespace-separated tokens. Walk from the right and
    // peel `required` / `optional` modifiers; whatever remains is the
    // type text.
    let mut type_text = rest.to_owned();
    let mut required = false;
    let mut optional = false;
    loop {
        let trimmed = type_text.trim_end();
        if trimmed.ends_with(" required") {
            required = true;
            type_text = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            optional = true;
            type_text = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else {
            type_text = trimmed.to_owned();
            break;
        }
    }
    (type_text, required, optional)
}
