//! `@owner_axis(through: ...)` FK-field completions.
//!
//! `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5 — when
//! the cursor sits inside `@owner_axis(through: <|>)`, offer the FK
//! fields of the current resource as completion candidates. "FK field"
//! here means a field on the surrounding `resource <Name>` block whose
//! `type_text` is a bare PascalCase identifier (the analyzer resolves
//! these to `TypeRef::UserDefined(QualifiedName)` — surface-level
//! references to other resources).
//!
//! Returns `None` outside `@owner_axis(through: ...)`; when inside but
//! no FK fields are visible on the surrounding resource, returns
//! `Some(vec![])` (the LSP suppresses the global keyword list in
//! favour of the empty context-specific list rather than offering noise).

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

pub(crate) fn owner_axis_through_completions(
    source: &str,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    // Cheap context check — only fire when we are inside an open
    // `@owner_axis(` on the same line and the cursor is positioned
    // after `through:` (the only keyword argument in this proposal —
    // see §7.1 grammar).
    let open = before.rfind("@owner_axis(")?;
    let after_open = &before[open + "@owner_axis(".len()..];
    let through_idx = after_open.rfind("through:")?;
    let after_through = &after_open[through_idx + "through:".len()..];
    // Accept either cursor right after `through:` (possibly with
    // whitespace) or mid-value with a partial identifier. Reject when
    // a comma intervenes (would mean we've moved to a different — and
    // currently non-existent — argument).
    if after_through.contains(',') {
        return None;
    }

    // Walk source lines backward from the cursor to find the
    // surrounding `resource <Name>` header. Indent-aware: the resource
    // header is at the feature's `resource` indent (2 spaces in
    // canonical authoring), and field lines sit one level deeper.
    let cursor_line = position.line as usize;
    let lines: Vec<&str> = source.lines().collect();
    let mut resource_start: Option<usize> = None;
    let mut resource_indent: Option<usize> = None;
    for idx in (0..=cursor_line.min(lines.len().saturating_sub(1))).rev() {
        let l = lines[idx];
        let trimmed = l.trim_start();
        if let Some(name) = trimmed.strip_prefix("resource ") {
            // `resource <Name>` header — anchors our field scan.
            // Ignore the trailing modifiers (none authored today).
            let _ = name;
            resource_start = Some(idx);
            resource_indent = Some(l.len() - trimmed.len());
            break;
        }
    }
    let start = resource_start?;
    let res_indent = resource_indent?;

    // Scan forward from the resource header collecting field names
    // whose `type_text` looks like a bare resource reference (PascalCase
    // identifier with no decorator chain and no builtin keyword).
    let mut fk_fields: Vec<String> = Vec::new();
    for l in lines.iter().skip(start + 1) {
        let trimmed = l.trim_start();
        let indent = l.len() - trimmed.len();
        // Stop at the next sibling/parent block (same or shallower indent
        // than the resource header), skipping blank lines.
        if !trimmed.is_empty() && indent <= res_indent {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Only consider direct-child lines of the resource (depth
        // exactly `res_indent + 2`). Deeper indents are sub-clauses
        // (e.g. `conventions [..]` children, lifecycle bodies).
        if indent != res_indent + 2 {
            continue;
        }
        // Field declarations have shape `<name>: <Type> ...`. Split
        // off the type half and discard everything past the first
        // whitespace / modifier / decorator.
        let Some((name, after_colon)) = trimmed.split_once(':') else {
            continue;
        };
        let field_name = name.trim();
        // Field names are snake_case identifiers; reject other lines
        // (e.g. `conventions [..]`).
        if !field_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || field_name.is_empty()
        {
            continue;
        }
        let type_text = after_colon.trim_start();
        // Take the first whitespace-delimited token as the type. FK
        // type refs are bare PascalCase identifiers with no `@` prefix
        // and no `.` (builtins like `Text`/`Integer` are filtered by
        // the closed-catalog skip-list below).
        let head = type_text
            .split(|c: char| c.is_ascii_whitespace())
            .next()
            .unwrap_or("");
        if head.is_empty() || head.starts_with('@') || head.contains('.') {
            continue;
        }
        let first_char = head.chars().next().unwrap_or('a');
        if !first_char.is_ascii_uppercase() {
            continue;
        }
        // Closed-catalog skip list — builtin PascalCase types that are
        // not FK references. `User`/`Org` are excluded so the synth's
        // tenant-keyed default is the canonical surface; authors who
        // want owner-scope on a tenant column would do it via the
        // `user: User required unique` field semantics, not @owner_axis.
        if matches!(
            head,
            "Text" | "Integer" | "Boolean" | "Date" | "DateTime" | "Decimal" | "Json" | "ID" | "Id"
        ) {
            continue;
        }
        fk_fields.push(field_name.to_owned());
    }

    Some(
        fk_fields
            .into_iter()
            .map(|name| CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("FK column on the current resource".to_owned()),
                ..CompletionItem::default()
            })
            .collect(),
    )
}
