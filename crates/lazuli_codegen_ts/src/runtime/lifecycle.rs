//! Lifecycle action-map emitter (`docs/proposals/lifecycle-vocab.md §6.2`).
//!
//! Each resource that authored a `lifecycle` block gets one TS object
//! shaped `{ <transitionName>: useLazuliCommand<Input, void>(<cmd>), … }`.
//! The object is `const`-asserted so consumers get exhaustive narrowing
//! on the transition keys.
//!
//! Lifecycle emission is driven from the canonical `ir::Feature` rather
//! than `RuntimeFeature` because the runtime-spec projection does not
//! yet carry `Resource.lifecycle`; the IR-backed SDK emitter calls this
//! helper and appends the result to the generated `<feature>.gen.ts`.
//! `type_ref_ts` handles the IR→TS type lowering that the SDK emitter
//! also reuses for related transition input shapes.

use std::fmt::Write;

use lazuli_ir as ir;

use super::header::write_section_banner;
use crate::lzx::{lower_camel as lzx_lower_camel, pascal_case as lzx_pascal_case};

pub(super) fn write_lifecycle_action_maps(s: &mut String, feature: &ir::Feature) {
    let lifecycle_resources: Vec<&ir::Resource> = feature
        .resources
        .iter()
        .filter(|resource| resource.lifecycle.is_some())
        .collect();

    if lifecycle_resources.is_empty() {
        return;
    }

    write_section_banner(s, &["Lifecycle action maps".to_owned()]);
    writeln!(
        s,
        "// Auto-generated from lifecycle. Per docs/proposals/lifecycle-vocab.md §6.2."
    )
    .ok();

    for resource in lifecycle_resources {
        let lifecycle = resource
            .lifecycle
            .as_ref()
            .expect("filtered lifecycle resources have lifecycle");
        let resource_pascal = lzx_pascal_case(&resource.name);
        let object_name = resource.name.to_ascii_lowercase();
        writeln!(s, "export const {object_name} = {{").ok();
        for transition in &lifecycle.transitions {
            let action_key = lzx_lower_camel(&transition.name);
            let command_ident = lifecycle_command_ident(&transition.name, &resource_pascal);
            let input_type = lifecycle_transition_input_type(feature, &transition.name);
            writeln!(
                s,
                "  {action_key}: useLazuliCommand<{input_type}, void>({command_ident}),"
            )
            .ok();
        }
        writeln!(s, "}} as const;").ok();
        writeln!(s).ok();
    }
}

fn lifecycle_command_ident(transition_name: &str, resource_pascal: &str) -> String {
    format!("{}{resource_pascal}", lzx_lower_camel(transition_name))
}

fn lifecycle_transition_input_type(feature: &ir::Feature, transition_name: &str) -> String {
    let mut fields = vec![("id".to_owned(), "ID".to_owned())];

    if let Some(command) = feature
        .commands
        .iter()
        .find(|command| command.name == transition_name)
    {
        for route in &command.route {
            push_lifecycle_input_field(&mut fields, &route.name, &route.type_ref);
        }
        if let ir::CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                push_lifecycle_input_field(&mut fields, &slot.name, &slot.type_ref);
            }
        }
    }

    let parts: Vec<String> = fields
        .into_iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect();
    format!("{{ {} }}", parts.join("; "))
}

fn push_lifecycle_input_field(
    fields: &mut Vec<(String, String)>,
    name: &str,
    type_ref: &ir::TypeRef,
) {
    let key = if name.eq_ignore_ascii_case("id") {
        "id".to_owned()
    } else {
        lzx_lower_camel(name)
    };
    if fields.iter().any(|(existing, _)| existing == &key) {
        return;
    }
    fields.push((key, type_ref_ts(type_ref).to_owned()));
}

fn type_ref_ts(type_ref: &ir::TypeRef) -> &'static str {
    match type_ref {
        ir::TypeRef::Builtin(ir::BuiltinType::Id) => "ID",
        ir::TypeRef::Builtin(ir::BuiltinType::Boolean) => "boolean",
        ir::TypeRef::Builtin(ir::BuiltinType::Integer | ir::BuiltinType::Decimal) => "number",
        ir::TypeRef::Builtin(
            ir::BuiltinType::Text
            | ir::BuiltinType::Date
            | ir::BuiltinType::DateTime
            | ir::BuiltinType::SemanticEmail
            | ir::BuiltinType::SemanticPhone
            | ir::BuiltinType::SemanticUrl
            | ir::BuiltinType::SemanticUuid
            | ir::BuiltinType::SemanticCurrency
            | ir::BuiltinType::SemanticGeoPoint,
        ) => "string",
        ir::TypeRef::Builtin(ir::BuiltinType::SemanticMoney { .. }) => "string",
        // B3 — plugin-contributed `@semantic.<Name>` resolves through
        // the carrier (always `Text` in v1, per the proposal closed
        // carrier catalog), so the wire surface is a string. The
        // SDK emitter at `lazuli_cli::main::emit_feature_sdk_ts`
        // additionally produces a `type <Name> = string` brand alias
        // for richer typing in app code.
        ir::TypeRef::Builtin(ir::BuiltinType::SemanticPluginType { .. }) => "string",
        ir::TypeRef::Builtin(ir::BuiltinType::Json) => "unknown",
        ir::TypeRef::Builtin(ir::BuiltinType::CapSecret | ir::BuiltinType::CapFile) => "unknown",
        ir::TypeRef::EnumRef(_) | ir::TypeRef::UserDefined(_) | ir::TypeRef::Unresolved(_) => {
            "string"
        }
        ir::TypeRef::Many(_) => "unknown[]",
        ir::TypeRef::Capability(_) => "unknown",
    }
}

/// router-w4 — emit one `<resource>LifecycleRoute(state)` helper. Each
/// helper is a flat switch over a `lifecycle_routes` table's arms;
/// `null`/`undefined` falls through to the `none` arm if present,
/// otherwise to the `*` wildcard. Routes that declared
/// `requires_lifecycle X = <state>` with
/// `on_lifecycle_pending dispatch_via X.lifecycle_route` consume this
/// helper from `routes.gen.tsx` beforeLoad closures.
pub(super) fn write_lifecycle_route_helper(
    s: &mut String,
    helper: &str,
    table: &ir::LifecycleRoutes,
) {
    let none_url = table
        .arms
        .iter()
        .find(|a| a.state == "none")
        .map(|a| a.url.clone());
    let wildcard_url = table
        .arms
        .iter()
        .find(|a| a.state == "*")
        .map(|a| a.url.clone());

    s.push_str(&format!(
        "export function {helper}(state: string | null | undefined): string {{\n"
    ));
    s.push_str("  if (state === null || state === undefined) {\n");
    let none_fallback = none_url
        .clone()
        .or(wildcard_url.clone())
        .unwrap_or_else(|| "/".to_owned());
    s.push_str(&format!("    return {:?};\n", none_fallback));
    s.push_str("  }\n");
    s.push_str("  switch (state) {\n");
    for arm in &table.arms {
        if arm.state == "*" || arm.state == "none" {
            continue;
        }
        s.push_str(&format!(
            "    case {:?}: return {:?};\n",
            arm.state, arm.url
        ));
    }
    let default = wildcard_url.or(none_url).unwrap_or_else(|| "/".to_owned());
    s.push_str(&format!("    default: return {:?};\n", default));
    s.push_str("  }\n");
    s.push_str("}\n");
}
