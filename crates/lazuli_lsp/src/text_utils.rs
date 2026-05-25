//! Text + position utilities used across the LSP — line/column math,
//! UTF-16 character indexing, word-at-cursor lookup, range builders,
//! and the small `CodeAction` envelope helper.
//!
//! These are the load-bearing primitives that every diagnostic producer
//! and completion provider calls into. Keeping them clustered in one
//! file makes the file-local boundary obvious: no diagnostic rules,
//! no LSP plumbing, only `(&str, position)` -> `(Range | usize | …)`
//! transforms.
//!
//! Wave R7-3 extract: lifted out of `lib.rs`.

use std::collections::HashMap;

use lazuli_syntax::Span;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, DiagnosticSeverity, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};

pub(crate) fn simple_canonical_diagnostic(
    line_index: usize,
    line: &str,
    severity: DiagnosticSeverity,
    code: &str,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: line_index as u32,
                character: leading_spaces(line) as u32,
            },
            end: Position {
                line: line_index as u32,
                character: line.len().max(leading_spaces(line) + 1) as u32,
            },
        },
        severity: Some(severity),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            code.to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: message.to_owned(),
        related_information: None,
        tags: None,
        data: None,
    }
}

pub(crate) fn is_trivia_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

pub(crate) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

pub(crate) fn feature_name(trimmed_line: &str) -> String {
    trimmed_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("<anonymous>")
        .to_owned()
}

pub(crate) fn range_from_span(source: &str, span: Span) -> Range {
    let len = source.len();
    let start = span.start.min(len);
    let end = span.end.max(span.start.saturating_add(1)).min(len);

    Range {
        start: position_for_offset(source, start),
        end: position_for_offset(source, end),
    }
}

pub(crate) fn first_line_range(source: &str) -> Range {
    let end = source.lines().next().map(str::len).unwrap_or(1).max(1);
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: end as u32,
        },
    }
}

pub(crate) fn full_document_range(source: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: position_for_offset(source, source.len()),
    }
}

pub(crate) fn position_for_offset(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position { line, character }
}

pub(crate) fn word_at_position(source: &str, position: Position) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let target = byte_index_for_utf16_position(line, position.character);
    let mut start = target.min(line.len());
    let mut end = target.min(line.len());
    let bytes = line.as_bytes();

    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }

    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }

    if start == end {
        None
    } else {
        Some(line[start..end].to_owned())
    }
}

pub(crate) fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

pub(crate) fn line_prefix_at_position(line: &str, character: u32) -> &str {
    let byte_index = byte_index_for_utf16_position(line, character);
    &line[..byte_index]
}

pub(crate) fn byte_index_for_utf16_position(line: &str, character: u32) -> usize {
    let mut utf16 = 0u32;
    for (byte_index, ch) in line.char_indices() {
        if utf16 >= character {
            return byte_index;
        }
        let next = utf16 + ch.len_utf16() as u32;
        if next > character {
            return byte_index;
        }
        utf16 = next;
    }
    line.len()
}

pub(crate) fn is_design_lzi_uri(uri: &Url) -> bool {
    uri.path().ends_with("design.lzi")
}

pub(crate) fn is_lzx_uri(uri: &Url) -> bool {
    uri.path().ends_with(".lzx")
}

pub(crate) fn simple_edit_action(
    uri: &Url,
    title: &str,
    kind: CodeActionKind,
    edits: Vec<TextEdit>,
    preferred: bool,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    CodeAction {
        title: title.to_owned(),
        kind: Some(kind),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(preferred),
        disabled: None,
        data: None,
    }
}

/// Position at the start of `line_idx` (character 0). Used as both the
/// start and end of an inserting `TextEdit` (zero-width range).
pub(crate) fn position_at_line_start(line_idx: usize) -> Position {
    Position {
        line: line_idx as u32,
        character: 0,
    }
}
