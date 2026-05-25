//! `event_group` feature child — domain-event surface.
//!
//! Sits inside a feature's `domain` block (typically at indent 4 or
//! deeper depending on the domain depth). The parser tracks the
//! header's own indent so it works at any nesting level; children
//! live at `header.indent + 2`, grandchildren at `header.indent + 4`.
//!
//! ## Grammar (closed at the keyword level, payload free-form)
//!
//! ```text
//! event_group <pattern> [on <Resource>]
//!   payload
//!     <free-form rows — analyzer interprets>
//!   audit <id>
//!   event <name>
//!     outbox guaranteed | none           # EVENT-OUTBOX §3.3
//!     <field>: <Type> [required|optional]
//!   event.trace <name>                   # outbox-incompatible
//!     <field>: <Type> [required|optional]
//! ```
//!
//! `event.trace` cannot carry an `outbox` guarantee — trace events
//! bypass the outbox entirely. The parser enforces that pairing.
//!
//! ## Variant payload rows (B5 framework gap 1)
//!
//! Typed payload field rows under an `event <name>` body share the
//! surface shape of resource fields (`<name>: <Type> [required|optional]`),
//! but reject defaults / constraints / `unique` / `slug` / `@full_text` —
//! event payloads are projection-only.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — `event_group` grammar.
//! - `lazuli_ir::nodes::event` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, line_error_owned, strip_inline_comment};
use super::super::error::ParseError;

use crate::ast::{EventGroup, EventVariantFieldDecl, EventVariantKindAst, Span};

pub(super) fn parse_event_group(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(EventGroup, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let rest = header_trimmed
        .strip_prefix("event_group ")
        .ok_or_else(|| line_error(header, "event_group header must be `event_group <pattern>`"))?
        .trim();
    if rest.is_empty() {
        return Err(line_error(header, "event_group header requires a pattern"));
    }
    let (pattern, on_resource) = if let Some(idx) = rest.find(" on ") {
        let (lhs, rhs) = rest.split_at(idx);
        let resource = rhs[" on ".len()..].trim().to_owned();
        if resource.is_empty() {
            return Err(line_error(
                header,
                "`event_group ... on <Resource>` requires a resource name",
            ));
        }
        (lhs.trim().to_owned(), Some(resource))
    } else {
        (rest.to_owned(), None)
    };

    // event_group sits inside `domain`, so its children typically live
    // at `header.indent + 2`. We track that floor here rather than the
    // global agent indent because the group can appear at any depth
    // depending on whether `domain` is nested.
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut payload: Vec<String> = Vec::new();
    let mut audit: Option<String> = None;
    let mut events: Vec<String> = Vec::new();
    let mut events_outbox_guaranteed: Vec<bool> = Vec::new();
    let mut event_variants: Vec<Vec<EventVariantFieldDecl>> = Vec::new();
    let mut event_variant_kinds: Vec<EventVariantKindAst> = Vec::new();
    let mut in_payload = false;
    // EVENT-OUTBOX §3.3 — when set, grandchild `outbox guaranteed` lines
    // toggle the most recently authored event's outbox flag.
    let mut current_event_idx: Option<usize> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= header_indent {
            break;
        }

        if line.indent == child_indent {
            in_payload = false;
            current_event_idx = None;
            if trimmed == "payload" {
                in_payload = true;
            } else if let Some(rest) = trimmed.strip_prefix("audit ") {
                audit = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("event ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    events.push(name);
                    events_outbox_guaranteed.push(false);
                    event_variants.push(Vec::new());
                    event_variant_kinds.push(EventVariantKindAst::Committed);
                    current_event_idx = Some(events.len() - 1);
                }
            } else if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    events.push(name);
                    events_outbox_guaranteed.push(false);
                    event_variants.push(Vec::new());
                    event_variant_kinds.push(EventVariantKindAst::Trace);
                    // Trace events cannot carry an outbox guarantee;
                    // grandchild `outbox` lines are rejected below, but
                    // typed field rows are still collected.
                    current_event_idx = Some(events.len() - 1);
                }
            } else {
                // Unknown child — Tier 4 may extend this; skip silently
                // to match Phase L's existing fall-through behaviour.
            }
        } else if line.indent >= grandchild_indent && in_payload {
            payload.push(trimmed.to_owned());
        } else if line.indent >= grandchild_indent
            && let Some(idx) = current_event_idx
            && let Some(rest) = trimmed.strip_prefix("outbox ")
        {
            if matches!(event_variant_kinds[idx], EventVariantKindAst::Trace) {
                return Err(line_error(
                    line,
                    "`event.trace` cannot carry an `outbox` guarantee; trace events bypass the outbox",
                ));
            }
            // EVENT-OUTBOX §3.3 — accept `outbox guaranteed` under
            // `event <name>`. Closed catalog: only `guaranteed` is
            // authored; `outbox none` is implicit (the default).
            match rest.trim() {
                "guaranteed" => events_outbox_guaranteed[idx] = true,
                "none" => events_outbox_guaranteed[idx] = false,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!(
                            "`outbox` only accepts `guaranteed` (or `none`); got `{}`",
                            other
                        ),
                    ));
                }
            }
        } else if line.indent >= grandchild_indent
            && let Some(idx) = current_event_idx
            && trimmed.contains(':')
            && !trimmed.starts_with('#')
        {
            // B5 framework gap 1 — typed payload field row under an
            // event variant. Surface shape mirrors resource fields:
            // `<name>: <Type> [required|optional]`. The type literal
            // is kept verbatim; the analyzer lifts to `ir::TypeRef`.
            let field = parse_event_variant_field(line, trimmed)?;
            event_variants[idx].push(field);
        } else {
            // Continuation of a non-payload child (anything else falls
            // through here; legacy fallthrough kept for fixtures that
            // author lines we haven't taught the parser to lift yet).
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        EventGroup {
            pattern,
            on_resource,
            payload,
            audit,
            events,
            events_outbox_guaranteed,
            event_variants,
            event_variant_kinds,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// B5 framework gap 1 — parse a single `<name>: <Type> [required|optional]`
/// row inside an `event_group`'s `event <name>` body. Mirrors the
/// minimum slot count of `ResourceFieldDecl` but rejects modifiers
/// outside the closed `required` / `optional` pair (no defaults, no
/// constraints, no `unique`/`slug`/`@full_text` because event payloads
/// are projection-only).
fn parse_event_variant_field(
    line: &SourceLine<'_>,
    trimmed: &str,
) -> Result<EventVariantFieldDecl, ParseError> {
    let raw = strip_inline_comment(trimmed).trim_end();
    let (name_part, after) = raw.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "event variant field must be `<name>: <Type> [required|optional]`",
        )
    })?;
    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a name before `:`",
        ));
    }
    let after = after.trim();
    if after.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a type after `:`",
        ));
    }
    // Strip trailing modifiers (`required` / `optional`) from the type
    // literal so the analyzer's `type_ref_from_syntax` sees the bare
    // type token. Keep the split conservative: only the last
    // whitespace-separated token is considered a modifier candidate,
    // matching the resource-field convention.
    let mut required = false;
    let mut optional = false;
    let mut type_text = after.to_owned();
    loop {
        let trimmed_type = type_text.trim_end();
        // Last whitespace-separated token without scanning forward.
        let tail = match trimmed_type.rfind(|c: char| c.is_whitespace()) {
            Some(idx) => &trimmed_type[idx + 1..],
            None => trimmed_type,
        };
        match tail {
            "required" => {
                required = true;
                let cut = trimmed_type.len() - tail.len();
                type_text = trimmed_type[..cut].trim_end().to_owned();
            }
            "optional" => {
                optional = true;
                let cut = trimmed_type.len() - tail.len();
                type_text = trimmed_type[..cut].trim_end().to_owned();
            }
            _ => break,
        }
    }
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a type literal before the modifier",
        ));
    }
    Ok(EventVariantFieldDecl {
        name: name.to_owned(),
        type_text,
        required,
        optional,
        span: Span::new(line.start, line.end),
    })
}
