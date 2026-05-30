//! Cell G3b - `EventGroup` kind emission. Walks every `EventGroup`
//! and typed `Event` declared on a feature and emits event-group
//! contracts plus payload structs into `<feature>/events.gen.go`.
//!
//! Proposal references:
//! - Section 3.12 - `lazuli.EventGroup` / `lazuli.EventDescriptor`
//!   value shape.
//! - Section 4.6 - Lazuli Go lib currently lacks declarative
//!   `EventGroup` and `EventDescriptor` runtime types. We still emit
//!   the proposal shape and leave `// TODO(runtime): ...` comments
//!   inside the value literal so the gap is visible without touching
//!   `runtime/go/lazuli/**`.
//!
//! Current IR note: this worktree carries the Phase L Tier 3
//! `EventGroup` shape (`pattern`, `on_resource`, `raw_payload`,
//! `events: Vec<String>`), not the future descriptor-rich shape.
//! Group payload fields are therefore inferred from raw assignment
//! lines where possible; event-specific typed fields come from
//! `Feature.events`.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{Event, EventGroup, EventVariant, Feature, Resource, TypeRef};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

mod infer;
mod payload;

use payload::{PayloadStruct, group_payload_fields, typed_event_fields, typed_payload_field};

/// Emit `<feature>/events.gen.go` for a feature, or `None` when the
/// feature declares no event groups and no standalone typed events.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_events_file("billing.lzi", &feature, "demo", &cross_index);
/// ```
pub fn emit_events_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    if feature.event_groups.is_empty() && feature.events.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut groups: Vec<&EventGroup> = feature.event_groups.iter().collect();
    groups.sort_by(|a, b| {
        a.pattern
            .cmp(&b.pattern)
            .then_with(|| a.on_resource.cmp(&b.on_resource))
    });

    let events_by_name: BTreeMap<&str, &Event> = feature
        .events
        .iter()
        .map(|event| (event.name.as_str(), event))
        .collect();
    let mut matched_events: BTreeSet<&str> = BTreeSet::new();
    let mut payloads: BTreeMap<String, PayloadStruct> = BTreeMap::new();

    if !groups.is_empty() {
        imports.add("lazuli.dev/runtime/lazuli");
    }

    for group in &groups {
        let group_fields = group_payload_fields(group, feature, &type_ctx, &mut imports);
        // B5 framework gap 1 — when the event_group authored typed
        // variants, prefer them; otherwise fall back to the legacy
        // `Feature.events` lookup so pre-gap fixtures keep working.
        let variants_by_short: BTreeMap<&str, &EventVariant> = group
            .variants
            .iter()
            .map(|v| (v.name.as_str(), v))
            .collect();
        for (full_name, short_name) in sorted_group_event_names(group) {
            let payload = payloads
                .entry(full_name.clone())
                .or_insert_with(|| PayloadStruct::new(&full_name));
            for field in &group_fields {
                payload.push_field(field.clone());
            }
            if let Some(variant) = variants_by_short.get(short_name) {
                // Typed-variant path — pull per-event fields straight
                // from the group's variant record. Trace variants
                // surface a `// TODO(runtime)` comment the same way
                // the legacy `event_runtime_gap_comments` does.
                for field in &variant.fields {
                    let lowered = typed_payload_field(field, &type_ctx, &mut imports);
                    payload.push_field(lowered);
                }
                if matches!(variant.kind, lazuli_ir::EventVariantKind::Trace) {
                    payload.push_comment(format!(
                        "// TODO(runtime): event.trace {} metadata is not represented by lazuli.EventDescriptor yet.",
                        full_name
                    ));
                }
                continue;
            }
            if let Some(event) = matching_event(&events_by_name, &full_name, short_name) {
                matched_events.insert(event.name.as_str());
                for field in typed_event_fields(event, &type_ctx, &mut imports) {
                    payload.push_field(field);
                }
                for comment in event_runtime_gap_comments(event) {
                    payload.push_comment(comment);
                }
            }
        }
    }

    let mut standalone_events: Vec<&Event> = feature
        .events
        .iter()
        .filter(|event| !matched_events.contains(event.name.as_str()))
        .collect();
    standalone_events.sort_by(|a, b| a.name.cmp(&b.name));
    for event in standalone_events {
        let payload = payloads
            .entry(event.name.clone())
            .or_insert_with(|| PayloadStruct::new(&event.name));
        for field in typed_event_fields(event, &type_ctx, &mut imports) {
            payload.push_field(field);
        }
        for comment in event_runtime_gap_comments(event) {
            payload.push_comment(comment);
        }
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    if !imports.is_empty() {
        imports.emit(&mut p);
        p.blank();
    }

    let mut first_block = true;
    for group in &groups {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_group(&mut p, feature, group);
    }
    for payload in payloads.values() {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_payload_struct(&mut p, payload);
    }

    Some(p.finish())
}

fn emit_group(p: &mut GoPrinter, feature: &Feature, group: &EventGroup) {
    let qualified_name = format!("{}.{}", feature.name, group.pattern);

    write_section_banner(
        p,
        &[
            format!("EventGroup: {qualified_name}"),
            format!("  event_group {}", group.pattern),
        ],
    );

    p.line(&format!(
        "var {} = lazuli.EventGroup{{",
        event_group_var_name(&feature.name, &group.pattern)
    ));
    p.indent();

    let resource = group.on_resource.as_deref().unwrap_or("");
    let kv_rows = vec![
        ("Pattern:".to_owned(), go_string(&group.pattern)),
        ("Resource:".to_owned(), go_string(resource)),
    ];
    emit_kv_rows(p, &kv_rows);

    let event_names = sorted_group_event_names(group);
    if event_names.is_empty() {
        p.line("Events: []lazuli.EventDescriptor{},");
    } else {
        p.line("Events: []lazuli.EventDescriptor{");
        p.indent();
        for (full_name, _short_name) in event_names {
            p.line(&format!(
                "{{Name: \"{}\", PayloadType: \"{}\"}},",
                escape_string(&full_name),
                payload_struct_name(&full_name)
            ));
        }
        p.dedent();
        p.line("},");
    }

    if let Some(raw_audit) = &group.raw_audit {
        p.line(&format!(
            "// TODO(runtime): EventGroup audit `{}` is not represented in Lazuli Go lib.",
            escape_comment(raw_audit)
        ));
    }

    p.dedent();
    p.line("}");
}

fn emit_payload_struct(p: &mut GoPrinter, payload: &PayloadStruct) {
    write_section_banner(
        p,
        &[
            format!("EventPayload: {}", payload.name),
            format!("  event {}", payload.event_name),
        ],
    );

    for comment in &payload.comments {
        p.line(comment);
    }
    p.line(&format!("type {} struct {{", payload.name));
    p.indent();
    if !payload.fields.is_empty() {
        let rows: Vec<(&str, &str, &str)> = payload
            .fields
            .iter()
            .map(|field| {
                (
                    field.name.as_str(),
                    field.go_type.as_str(),
                    field.tag.as_str(),
                )
            })
            .collect();
        p.aligned_struct_rows(&rows);
    }
    p.dedent();
    p.line("}");
}

fn emit_kv_rows(p: &mut GoPrinter, rows: &[(String, String)]) {
    let key_width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
}

pub(super) fn find_resource<'a>(feature: &'a Feature, name: &str) -> Option<&'a Resource> {
    feature
        .resources
        .iter()
        .find(|resource| resource.name == name || pascal_case(&resource.name) == pascal_case(name))
}

fn sorted_group_event_names(group: &EventGroup) -> Vec<(String, &str)> {
    let mut seen = BTreeSet::new();
    for short_name in &group.events {
        let full_name = event_name_for_group(&group.pattern, short_name);
        seen.insert((full_name, short_name.as_str()));
    }
    seen.into_iter().collect()
}

fn matching_event<'a>(
    events_by_name: &BTreeMap<&'a str, &'a Event>,
    full_name: &str,
    short_name: &str,
) -> Option<&'a Event> {
    events_by_name
        .get(full_name)
        .copied()
        .or_else(|| events_by_name.get(short_name).copied())
}

fn event_name_for_group(pattern: &str, short_name: &str) -> String {
    let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
    if prefix.is_empty() || short_name.starts_with(prefix) {
        short_name.to_owned()
    } else {
        format!("{prefix}{short_name}")
    }
}

fn event_runtime_gap_comments(event: &Event) -> Vec<String> {
    let mut comments = Vec::new();
    if matches!(event.kind, lazuli_ir::EventKind::Trace) {
        comments.push(format!(
            "// TODO(runtime): event.trace {} metadata is not represented by lazuli.EventDescriptor yet.",
            event.name
        ));
    }
    if let Some(level) = &event.level {
        comments.push(format!(
            "// TODO(runtime): event level `{}` is not represented by lazuli.EventDescriptor yet.",
            escape_comment(level)
        ));
    }
    comments
}

pub(super) fn register_imports_for_type(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}

pub(super) fn pointer_type(go_type: &str) -> String {
    if go_type.starts_with('*') {
        go_type.to_owned()
    } else {
        format!("*{go_type}")
    }
}

fn event_group_var_name(feature_name: &str, pattern: &str) -> String {
    let pattern_label = pattern_identifier_part(pattern);
    let raw = if pattern_label.is_empty() {
        format!("{feature_name}_events")
    } else {
        format!("{feature_name}_{pattern_label}_events")
    };
    lower_camel(&raw)
}

fn pattern_identifier_part(pattern: &str) -> String {
    let trimmed = pattern.trim().trim_end_matches('*').trim_matches('_');
    let mut out = String::with_capacity(trimmed.len());
    let mut last_was_sep = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    out.trim_matches('_').to_owned()
}

pub(super) fn payload_struct_name(event_name: &str) -> String {
    format!("{}Payload", pascal_case(event_name))
}

pub(super) fn is_payload_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn go_string(raw: &str) -> String {
    format!("\"{}\",", escape_string(raw))
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_comment(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn pascal_case(s: &str) -> String {
    super::casing::pascal_case(s)
}

fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod feature_emit_tests;
