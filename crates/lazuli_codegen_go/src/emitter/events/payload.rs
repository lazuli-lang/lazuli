//! Payload struct shape for `<feature>/events.gen.go`. Holds the
//! intermediate `PayloadField` / `PayloadStruct` records that
//! `emit_events_file` accumulates per typed event, plus the helpers
//! that lower IR fields (group-level raw lines, typed event fields)
//! into those records.

use std::collections::BTreeSet;

use lazuli_ir::{Event, EventField, EventGroup, Feature};

use super::infer::{infer_group_payload_type, parse_raw_payload_line};
use super::{find_resource, payload_struct_name, pointer_type, register_imports_for_type};
use crate::emitter::casing;
use crate::emitter::imports::ImportSet;
use crate::emitter::types::{self, TypeCtx};

#[derive(Clone)]
pub(super) struct PayloadField {
    pub(super) name: String,
    pub(super) go_type: String,
    pub(super) tag: String,
    pub(super) json_name: String,
}

pub(super) struct PayloadStruct {
    pub(super) event_name: String,
    pub(super) name: String,
    pub(super) fields: Vec<PayloadField>,
    pub(super) comments: Vec<String>,
    seen_json: BTreeSet<String>,
    seen_comments: BTreeSet<String>,
}

impl PayloadStruct {
    pub(super) fn new(event_name: &str) -> Self {
        Self {
            event_name: event_name.to_owned(),
            name: payload_struct_name(event_name),
            fields: Vec::new(),
            comments: Vec::new(),
            seen_json: BTreeSet::new(),
            seen_comments: BTreeSet::new(),
        }
    }

    pub(super) fn push_field(&mut self, field: PayloadField) {
        if self.seen_json.insert(field.json_name.clone()) {
            self.fields.push(field);
        }
    }

    pub(super) fn push_comment(&mut self, comment: String) {
        if self.seen_comments.insert(comment.clone()) {
            self.comments.push(comment);
        }
    }
}

pub(super) fn group_payload_fields(
    group: &EventGroup,
    feature: &Feature,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) -> Vec<PayloadField> {
    let resource = group
        .on_resource
        .as_deref()
        .and_then(|name| find_resource(feature, name));

    let mut fields = Vec::new();
    for raw in &group.raw_payload {
        let Some(parsed) = parse_raw_payload_line(raw) else {
            continue;
        };
        let mut inferred = infer_group_payload_type(&parsed, resource, ctx, imports);
        if parsed.optional {
            inferred.go_type = pointer_type(&inferred.go_type);
        }
        fields.push(payload_field(
            &parsed.name,
            &inferred.go_type,
            parsed.optional,
        ));
    }
    fields
}

pub(super) fn typed_event_fields(
    event: &Event,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) -> Vec<PayloadField> {
    event
        .payload
        .iter()
        .map(|field| typed_payload_field(field, ctx, imports))
        .collect()
}

pub(super) fn typed_payload_field(
    field: &EventField,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) -> PayloadField {
    register_imports_for_type(&field.type_ref, ctx, imports);
    let (go_type, _import) = types::go_type_for(&field.type_ref, ctx);
    let final_type = if field.optional {
        pointer_type(&go_type)
    } else {
        go_type
    };
    payload_field(&field.name, &final_type, field.optional)
}

fn payload_field(name: &str, go_type: &str, optional: bool) -> PayloadField {
    let json_suffix = if optional {
        format!("{name},omitempty")
    } else {
        name.to_owned()
    };
    PayloadField {
        name: casing::pascal_case(name),
        go_type: go_type.to_owned(),
        tag: format!("`json:\"{}\"`", json_suffix),
        json_name: name.to_owned(),
    }
}
