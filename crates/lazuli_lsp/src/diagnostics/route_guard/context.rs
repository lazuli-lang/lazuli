//! Context-awareness helpers for the IR Route-Guards LSP layer.
//!
//! These figure out *where* the cursor (or a given line) sits relative
//! to the route-guard surface: inside an `app.route_guard` block,
//! inside a view's body, inside an audience's body, inside the body of
//! a guard `policy` declaration. The completion and hover layers
//! consume these predicates to decide what to surface.
//!
//! Pure read-side facts — no diagnostics emitted from this file.

use tower_lsp::lsp_types::Position;

use crate::leading_spaces;

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardViewBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
    pub(crate) feature_hint: Option<String>,
}

pub(crate) fn route_guard_context_feature(source: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            for prefix in ["feature ", "surface ", "experience "] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        return Some(name.to_owned());
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn in_app_body_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            return trimmed.starts_with("app ");
        }
        if indent < cursor_indent && trimmed.starts_with("route_guard") {
            return false;
        }
    }
    false
}

pub(crate) fn at_app_child_completion_line(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    leading_spaces(line) == 2 && in_app_body_context(source, position)
}

pub(crate) fn in_app_route_guard_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardBlock> {
    let block = app_route_guard_block(source)?;
    let line_idx = position.line as usize;
    let line = source.lines().nth(line_idx).unwrap_or("");
    let indent = leading_spaces(line);
    if line_idx > block.header_line && line_idx < block.end_line && indent > block.header_indent {
        Some(block)
    } else {
        None
    }
}

pub(crate) fn app_route_guard_block(source: &str) -> Option<RouteGuardBlock> {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed == "route_guard" || trimmed.starts_with("route_guard ") {
            let header_indent = leading_spaces(line);
            let end_line = find_block_end(&lines, idx, header_indent);
            return Some(RouteGuardBlock {
                header_line: idx,
                header_indent,
                end_line,
            });
        }
    }
    None
}

pub(crate) fn in_view_or_audience_guard_context(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let trimmed = line.trim_start();
    if trimmed.starts_with("policy ") {
        return enclosing_view_block(source, position).is_some()
            || enclosing_audience_block(source, position).is_some();
    }
    enclosing_view_block(source, position).is_some()
        || enclosing_audience_block(source, position).is_some()
}

pub(crate) fn in_guard_policy_child_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with("policy ") {
            let pos = Position {
                line: idx as u32,
                character: indent as u32,
            };
            return in_view_or_audience_guard_context(source, pos);
        }
        return false;
    }
    false
}

pub(crate) fn enclosing_view_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "view")
}

pub(crate) fn enclosing_audience_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "audience")
}

pub(crate) fn enclosing_named_block(
    source: &str,
    position: Position,
    keyword: &str,
) -> Option<RouteGuardViewBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if idx == cursor_line_idx {
            if !trimmed.starts_with(&format!("{keyword} ")) {
                continue;
            }
        } else if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with(&format!("{keyword} ")) {
            let end_line = find_block_end(&lines, idx, indent);
            return Some(RouteGuardViewBlock {
                header_line: idx,
                header_indent: indent,
                end_line,
                feature_hint: route_guard_context_feature(
                    source,
                    Position {
                        line: idx as u32,
                        character: indent as u32,
                    },
                ),
            });
        }
        if indent == 0 {
            return None;
        }
    }
    None
}

pub(crate) fn find_block_end(lines: &[&str], header_line: usize, header_indent: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(header_line + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= header_indent {
            return idx;
        }
    }
    lines.len()
}

pub(crate) fn first_quoted_value(value: &str) -> Option<String> {
    let open = value.find('"')?;
    let rest = &value[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_owned())
}

pub(crate) fn collect_route_paths(source: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("path ") else {
            continue;
        };
        if let Some(path) = first_quoted_value(rest) {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}
