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

use lazuli_ir::{Event, EventField, EventGroup, EventVariant, Feature, Resource, TypeRef};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

/// Emit `<feature>/events.gen.go` for a feature, or `None` when the
/// feature declares no event groups and no standalone typed events.
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

    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
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
    p.line(
        "// TODO(runtime): lazuli.EventGroup is missing in Lazuli Go lib (proposal section 4.6).",
    );
    p.line(
        "// TODO(runtime): lazuli.EventDescriptor is missing in Lazuli Go lib (proposal section 4.6).",
    );

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

#[derive(Clone)]
struct PayloadField {
    name: String,
    go_type: String,
    tag: String,
    json_name: String,
}

struct PayloadStruct {
    event_name: String,
    name: String,
    fields: Vec<PayloadField>,
    comments: Vec<String>,
    seen_json: BTreeSet<String>,
    seen_comments: BTreeSet<String>,
}

impl PayloadStruct {
    fn new(event_name: &str) -> Self {
        Self {
            event_name: event_name.to_owned(),
            name: payload_struct_name(event_name),
            fields: Vec::new(),
            comments: Vec::new(),
            seen_json: BTreeSet::new(),
            seen_comments: BTreeSet::new(),
        }
    }

    fn push_field(&mut self, field: PayloadField) {
        if self.seen_json.insert(field.json_name.clone()) {
            self.fields.push(field);
        }
    }

    fn push_comment(&mut self, comment: String) {
        if self.seen_comments.insert(comment.clone()) {
            self.comments.push(comment);
        }
    }
}

fn group_payload_fields(
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

fn typed_event_fields(
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

fn typed_payload_field(
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
        name: pascal_case(name),
        go_type: go_type.to_owned(),
        tag: format!("`json:\"{}\"`", json_suffix),
        json_name: name.to_owned(),
    }
}

struct RawPayloadLine {
    name: String,
    expr: Option<String>,
    optional: bool,
}

fn parse_raw_payload_line(line: &str) -> Option<RawPayloadLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }

    if let Some((lhs, rhs)) = trimmed.split_once('=') {
        let name = lhs.trim();
        if !is_payload_name(name) {
            return None;
        }
        let (expr, optional) = split_when(rhs.trim());
        return Some(RawPayloadLine {
            name: name.to_owned(),
            expr: Some(expr.to_owned()),
            optional,
        });
    }

    if let Some((lhs, _rhs)) = trimmed.split_once(':') {
        let name = lhs.trim();
        if !is_payload_name(name) {
            return None;
        }
        return Some(RawPayloadLine {
            name: name.to_owned(),
            expr: None,
            optional: false,
        });
    }

    let name = trimmed.split_whitespace().next().unwrap_or("");
    if !is_payload_name(name) {
        return None;
    }
    Some(RawPayloadLine {
        name: name.to_owned(),
        expr: None,
        optional: false,
    })
}

fn split_when(raw: &str) -> (&str, bool) {
    if let Some((expr, _when)) = raw.split_once(" when ") {
        (expr.trim(), true)
    } else {
        (raw.trim(), false)
    }
}

struct InferredType {
    go_type: String,
}

fn infer_group_payload_type(
    parsed: &RawPayloadLine,
    resource: Option<&Resource>,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) -> InferredType {
    let Some(expr) = parsed.expr.as_deref() else {
        return infer_by_name(&parsed.name, imports);
    };
    let expr = expr.trim();
    if expr == "id" || expr.ends_with(".id") || parsed.name.ends_with("_id") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.ID".to_owned(),
        };
    }
    if expr == "ctx.now" || parsed.name.ends_with("_at") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.Time".to_owned(),
        };
    }
    if let Some(resource) = resource {
        let head = expr.split('.').next().unwrap_or(expr);
        if let Some(field) = resource.fields.iter().find(|field| field.name == head) {
            register_imports_for_type(&field.type_ref, ctx, imports);
            let (go_type, _import) = types::go_type_for(&field.type_ref, ctx);
            return InferredType { go_type };
        }
    }
    infer_by_name(&parsed.name, imports)
}

fn infer_by_name(name: &str, imports: &mut ImportSet) -> InferredType {
    if name.ends_with("_id") || name == "id" {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.ID".to_owned(),
        };
    }
    if name.ends_with("_at") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.Time".to_owned(),
        };
    }
    InferredType {
        go_type: "any".to_owned(),
    }
}

fn find_resource<'a>(feature: &'a Feature, name: &str) -> Option<&'a Resource> {
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

fn register_imports_for_type(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}

fn pointer_type(go_type: &str) -> String {
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

fn payload_struct_name(event_name: &str) -> String {
    format!("{}Payload", pascal_case(event_name))
}

fn is_payload_name(name: &str) -> bool {
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
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BuiltinType, Defaults, EventKind, Field, Module, OutboxMode, Policies,
        Resource, TypeRef,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
        lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            headers: None,
            cookie: None,
            proxy: None,
            limits: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_feature(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        emit_events_file("examples/x.lzi", feature, "lazuli/test", &index)
    }

    fn event_group(pattern: &str, events: Vec<&str>) -> EventGroup {
        let events: Vec<String> = events.into_iter().map(str::to_owned).collect();
        let events_outbox = vec![OutboxMode::None; events.len()];
        EventGroup {
            pattern: pattern.to_owned(),
            on_resource: Some("Customer".to_owned()),
            raw_payload: vec![
                "customer_id = id".to_owned(),
                "by_id = ctx.user.id when @actor.user".to_owned(),
            ],
            raw_audit: None,
            events,
            events_outbox,
            // B5 framework gap 1 — legacy fixtures author no typed
            // variants; `variants: Vec::new()` keeps the codegen on
            // the legacy `Feature.events` lookup path.
            variants: Vec::new(),
            span_ref: None,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![Field {
                name: "email".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn typed_event(name: &str, payload: Vec<EventField>) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload,
            payload_none: false,
            level: None,
            outbox: OutboxMode::None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn event_field(name: &str, builtin: BuiltinType, optional: bool) -> EventField {
        EventField {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            optional,
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn event_group_emits_runtime_todos_descriptors_and_inferred_payload() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource("Customer"));
        feature
            .event_groups
            .push(event_group("customer_*", vec!["zebra", "created"]));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("var customerCustomerEvents = lazuli.EventGroup{"));
        assert!(out.contains("// TODO(runtime): lazuli.EventGroup is missing"));
        assert!(out.contains("// TODO(runtime): lazuli.EventDescriptor is missing"));
        assert!(out.contains("Pattern:  \"customer_*\","));
        assert!(out.contains("Resource: \"Customer\","));

        let created_pos = out.find("Name: \"customer_created\"").unwrap();
        let zebra_pos = out.find("Name: \"customer_zebra\"").unwrap();
        assert!(created_pos < zebra_pos);
        assert!(out.contains("type CustomerCreatedPayload struct {"));
        assert!(out.contains("CustomerID lazuli.ID  `json:\"customer_id\"`"));
        assert!(out.contains("ByID       *lazuli.ID `json:\"by_id,omitempty\"`"));
    }

    #[test]
    fn typed_event_payload_merges_with_matching_group_payload() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource("Customer"));
        feature
            .event_groups
            .push(event_group("customer_*", vec!["created"]));
        feature.events.push(typed_event(
            "customer_created",
            vec![
                event_field("email", BuiltinType::SemanticEmail, false),
                event_field("source", BuiltinType::Text, true),
            ],
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerCreatedPayload struct {"));
        assert!(out.contains("CustomerID lazuli.ID    `json:\"customer_id\"`"));
        assert!(out.contains("Email      lazuli.Email `json:\"email\"`"));
        assert!(out.contains("Source     *string      `json:\"source,omitempty\"`"));
    }

    #[test]
    fn standalone_typed_event_emits_payload_struct_without_group_literal() {
        let mut feature = base_feature("session");
        feature.events.push(typed_event(
            "customer_logged_in",
            vec![
                event_field("customer_id", BuiltinType::Id, false),
                event_field("provider", BuiltinType::Text, false),
            ],
        ));

        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("lazuli.EventGroup"));
        assert!(out.contains("type CustomerLoggedInPayload struct {"));
        assert!(out.contains("CustomerID lazuli.ID `json:\"customer_id\"`"));
        assert!(out.contains("Provider   string    `json:\"provider\"`"));
    }

    /// B5 framework gap 1 — when the event_group carries typed
    /// variants the emitter writes per-variant payload struct shapes
    /// straight from `EventGroup.variants[i].fields`, no longer
    /// requiring `Feature.events` to be populated. This is the
    /// hostpoint failure mode: the legacy lookup returned the
    /// envelope-only struct.
    #[test]
    fn typed_variants_under_group_drive_per_variant_payload_shapes() {
        let mut feature = base_feature("payments");
        feature.resources.push(simple_resource("Charge"));
        let mut group = event_group("charge_*", vec!["confirmed", "failed"]);
        group.variants = vec![
            lazuli_ir::EventVariant {
                name: "confirmed".to_owned(),
                kind: lazuli_ir::EventVariantKind::Committed,
                outbox: OutboxMode::Guaranteed,
                fields: vec![
                    event_field("amount", BuiltinType::Decimal, false),
                    event_field("provider_payment_id", BuiltinType::Text, false),
                ],
                span_ref: None,
            },
            lazuli_ir::EventVariant {
                name: "failed".to_owned(),
                kind: lazuli_ir::EventVariantKind::Committed,
                outbox: OutboxMode::Guaranteed,
                fields: vec![
                    event_field("reason", BuiltinType::Text, false),
                    event_field("provider_error_code", BuiltinType::Text, true),
                ],
                span_ref: None,
            },
        ];
        feature.event_groups.push(group);
        let out = emit(&feature).expect("must emit");
        // Confirmed gets typed fields.
        assert!(
            out.contains("type ChargeConfirmedPayload struct {"),
            "ChargeConfirmedPayload missing:\n{}",
            out
        );
        assert!(
            out.contains("ProviderPaymentID") && out.contains("`json:\"provider_payment_id\"`"),
            "ProviderPaymentID payload field missing:\n{out}"
        );
        // Failed gets its OWN typed fields (different from confirmed).
        assert!(out.contains("type ChargeFailedPayload struct {"));
        assert!(out.contains("Reason"));
        assert!(out.contains("ProviderErrorCode"));
    }

    #[test]
    fn deterministic_across_runs_and_sorts_groups_by_pattern() {
        let mut feature = base_feature("customer");
        feature
            .event_groups
            .push(event_group("zebra_*", vec!["created"]));
        feature
            .event_groups
            .push(event_group("alpha_*", vec!["created"]));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);

        let alpha_pos = a.find("EventGroup: customer.alpha_*").unwrap();
        let zebra_pos = a.find("EventGroup: customer.zebra_*").unwrap();
        assert!(alpha_pos < zebra_pos);
    }
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Event, EventField, EventKind, Feature, Module, OutboxMode, Policies,
        TypeRef,
    };

    fn feature_with_standalone_event() -> Feature {
        Feature {
            name: "customer".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: vec![Event {
                name: "customer_signed_up".to_owned(),
                kind: EventKind::Domain,
                payload: vec![
                    EventField {
                        name: "customer_id".to_owned(),
                        type_ref: TypeRef::Builtin(BuiltinType::Id),
                        optional: false,
                    },
                    EventField {
                        name: "email".to_owned(),
                        type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
                        optional: false,
                    },
                ],
                payload_none: false,
                level: None,
                outbox: OutboxMode::None,
                previous_names: Vec::new(),
                span_ref: None,
            }],
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    #[test]
    fn feature_emit_entry_point_emits_standalone_event_payload() {
        let feature = feature_with_standalone_event();
        let module = module_with_feature(feature.clone());
        let cross_index = CrossFeatureIndex::build(&module);

        let out = emit_events_file(
            "features/customer/customer.lzi",
            &feature,
            "lazuli/test",
            &cross_index,
        )
        .expect("feature with a standalone event must emit events.gen.go");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("type CustomerSignedUpPayload struct {"));
        assert!(out.contains("CustomerID lazuli.ID    `json:\"customer_id\"`"));
        assert!(out.contains("Email      lazuli.Email `json:\"email\"`"));
    }
}
