//! Line-shape parsers shared across the `event` diagnostics family.
//!
//! These tiny extractors recognize one syntactic shape each and return
//! the captured name (or names). They are the leaf-most layer of the
//! event cluster — no diagnostics emitted, no source-wide state — so
//! every other event sub-module can call them without re-tangling the
//! cluster's call graph.
//!
//! | Parser | Recognizes |
//! |---|---|
//! | [`event_decl_name`] | `event <name>` / `event.trace <name>` |
//! | [`event_group_prefix`] | `event_group <prefix>*` / `events <prefix>*` |
//! | [`qualify_group_event_name`] | apply group prefix to a child event |
//! | [`tenancy_axis`] | `tenancy <axis>` |
//! | [`resource_name`] | `resource <name>` |
//! | [`events_resource_name`] | `event_group ... on <resource>` |
//! | [`field_name`] | `<field>: <type>` |
//! | [`payload_assignment_field`] | `<field> = <rhs>` (LHS) |
//! | [`payload_assignment_rhs`] | `<field> = <rhs>` (RHS) |
//! | [`payload_field_references`] | every `payload.<field>` in a line |
//! | [`resource_field_reference`] | bare identifier RHS, rejecting keywords |

pub(crate) fn event_decl_name(trimmed_line: &str) -> Option<&str> {
    if trimmed_line.starts_with("event.trace ") || trimmed_line.starts_with("event ") {
        trimmed_line.split_whitespace().nth(1)
    } else {
        None
    }
}

pub(crate) fn event_group_prefix(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }
    parts.next()?.strip_suffix('*')
}

pub(crate) fn qualify_group_event_name(prefix: &str, raw_name: &str) -> String {
    if raw_name.starts_with(prefix) {
        raw_name.to_owned()
    } else {
        format!("{prefix}{raw_name}")
    }
}

pub(crate) fn tenancy_axis(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "tenancy" {
        parts.next()
    } else {
        None
    }
}

pub(crate) fn resource_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "resource" {
        parts.next()
    } else {
        None
    }
}

pub(crate) fn events_resource_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }

    while let Some(part) = parts.next() {
        if part == "on" {
            return parts.next();
        }
    }

    None
}

pub(crate) fn field_name(trimmed_line: &str) -> Option<&str> {
    let (head, _) = trimmed_line.split_once(':')?;
    let name = head.trim().split_whitespace().next()?;

    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(name)
    } else {
        None
    }
}

pub(crate) fn payload_assignment_field(trimmed_line: &str) -> Option<&str> {
    let (field, _) = trimmed_line.split_once('=')?;
    let field = field.trim();
    (!field.is_empty()).then_some(field)
}

pub(crate) fn payload_assignment_rhs(trimmed_line: &str) -> Option<&str> {
    let (_, rhs) = trimmed_line.split_once('=')?;
    Some(rhs.trim())
}

pub(crate) fn payload_field_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("payload.") {
        let after_prefix = &rest[start + "payload.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let field = &after_prefix[..end];

        if !field.is_empty() {
            references.push(field.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}

pub(crate) fn resource_field_reference(expression: &str) -> Option<&str> {
    let first = expression.bytes().next()?;

    if first == b'"' || first.is_ascii_digit() || first.is_ascii_uppercase() {
        return None;
    }

    let end = expression
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
        .unwrap_or(expression.len());
    let segment = &expression[..end];

    if segment.is_empty()
        || matches!(
            segment,
            "ctx"
                | "event"
                | "ext"
                | "input"
                | "nil"
                | "null"
                | "params"
                | "payload"
                | "route"
                | "self"
                | "true"
                | "false"
        )
    {
        None
    } else {
        Some(segment)
    }
}
