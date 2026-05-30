//! Event-group + event-decl + payload-field collectors.
//!
//! `collect_event_groups` indexes every `event_group <prefix>* { ... }`
//! block in a feature; `collect_event_decls` flattens both standalone
//! `event ...` lines and the group-qualified shorthand; the helpers
//! around them (`event_kind_and_name`, `is_event_group_start`,
//! `event_group_pattern`, `qualify_group_event_name`,
//! `skip_nested_block`, `expand_payload_entry`) are small enough that
//! they stay co-located.
//!
//! Lifted out of the `expand` god-file in the rails-style split.

use super::text_utils::leading_spaces;

#[derive(Debug, Clone)]
pub(crate) struct EventGroup {
    pub(crate) pattern: String,
    pub(crate) prefix: String,
    pub(crate) payload: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct EventDecl {
    pub(crate) kind: &'static str,
    pub(crate) name: String,
    pub(crate) payload: Vec<String>,
}

pub(crate) fn collect_event_groups(lines: &[String]) -> Vec<EventGroup> {
    let mut groups = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];

        if !is_event_group_start(line) {
            index += 1;
            continue;
        }

        let Some((pattern, prefix)) = event_group_pattern(line.trim_start()) else {
            index += 1;
            continue;
        };

        let mut payload = Vec::new();
        let mut payload_block = false;
        let mut child_index = index + 1;

        while child_index < lines.len() {
            let child = &lines[child_index];
            let child_trimmed = child.trim_start();

            if child_trimmed.is_empty() {
                child_index += 1;
                continue;
            }

            if leading_spaces(child) <= 4 {
                break;
            }

            if leading_spaces(child) == 6 {
                payload_block = child_trimmed == "payload";
            } else if payload_block && leading_spaces(child) == 8 && !child_trimmed.is_empty() {
                payload.push(child_trimmed.to_owned());
            }

            child_index += 1;
        }

        groups.push(EventGroup {
            pattern,
            prefix,
            payload,
        });
        index = child_index;
    }

    groups
}

pub(crate) fn collect_event_decls(lines: &[String]) -> Vec<EventDecl> {
    let mut events = Vec::new();
    let mut current_group: Option<(usize, String)> = None;

    for index in 0..lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if is_event_group_start(line) {
            if let Some((_, prefix)) = event_group_pattern(trimmed) {
                current_group = Some((leading, prefix));
            }
            continue;
        }

        if let Some((group_indent, _)) = current_group.as_ref()
            && !trimmed.is_empty()
            && leading <= *group_indent
        {
            current_group = None;
        }

        if let Some((kind, raw_name)) = event_kind_and_name(trimmed) {
            let name = if let Some((group_indent, prefix)) = current_group.as_ref() {
                if leading == *group_indent + 2 {
                    qualify_group_event_name(prefix, raw_name)
                } else {
                    raw_name.to_owned()
                }
            } else {
                raw_name.to_owned()
            };
            events.push(EventDecl {
                kind,
                name,
                payload: collect_event_payload_fields(lines, index),
            });
        }
    }

    events
}

pub(super) fn collect_event_payload_fields(lines: &[String], event_index: usize) -> Vec<String> {
    let event_indent = leading_spaces(&lines[event_index]);
    let mut fields = Vec::new();
    let mut index = event_index + 1;

    while index < lines.len() && leading_spaces(&lines[index]) > event_indent {
        if leading_spaces(&lines[index]) == event_indent + 2 {
            let trimmed = lines[index].trim_start();
            if super::super::field_name_from_typed_line(trimmed).is_some() {
                fields.push(trimmed.to_owned());
            }
        }
        index += 1;
    }

    fields
}

pub(super) fn qualify_group_event_name(prefix: &str, raw_name: &str) -> String {
    if raw_name.starts_with(prefix) {
        raw_name.to_owned()
    } else {
        format!("{prefix}{raw_name}")
    }
}

pub(super) fn is_event_group_start(line: &str) -> bool {
    leading_spaces(line) == 4
        && matches!(
            line.split_whitespace().next(),
            Some("event_group" | "events")
        )
}

pub(super) fn event_group_pattern(trimmed_line: &str) -> Option<(String, String)> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }

    let pattern = parts.next()?;
    pattern
        .strip_suffix('*')
        .map(|prefix| (pattern.to_owned(), prefix.to_owned()))
}

pub(super) fn skip_nested_block(lines: &[String], start: usize, parent_indent: usize) -> usize {
    let mut index = start + 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if !trimmed.is_empty() && leading_spaces(&lines[index]) <= parent_indent {
            break;
        }
        index += 1;
    }

    index
}

pub(super) fn event_name(trimmed_line: &str) -> Option<&str> {
    event_kind_and_name(trimmed_line).map(|(_, name)| name)
}

pub(super) fn event_kind_and_name(trimmed_line: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = trimmed_line.strip_prefix("event.trace ") {
        rest.split_whitespace()
            .next()
            .map(|name| ("event.trace", name))
    } else {
        let rest = trimmed_line.strip_prefix("event ")?;
        rest.split_whitespace().next().map(|name| ("event", name))
    }
}

pub(super) fn expand_payload_entry(entry: &str) -> String {
    let Some((name, expression)) = entry.split_once('=') else {
        return entry.to_owned();
    };
    let name = name.trim();
    let expression = expression
        .split_once(" when ")
        .map(|(value, _)| value)
        .unwrap_or(expression)
        .trim();
    let ty = if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    };

    format!("{name}: {ty}")
}
