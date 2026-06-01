//! Schema emission — `components.schemas` entries and inline shapes
//! used by request/response bodies, path parameters, and slot types.
//!
//! Pure consumer of `lazuli_ir`: walks resources, records, enums, and
//! `TypeRef`s; emits the OpenAPI fragments into the shared
//! `YamlEmitter`. No transport mechanics live here — capabilities
//! surface as `x-lazuli-capability: <variant>` extensions only.

use lazuli_ir as ir;

use crate::yaml::{YamlEmitter, quote_value};

pub(crate) fn emit_command_input_schema(out: &mut YamlEmitter, cmd: &ir::Command) {
    match &cmd.input {
        ir::CommandInput::Empty => {}
        ir::CommandInput::Short(fields) => {
            out.line("type: object");
            out.line("properties:");
            out.indent();
            for f in fields {
                out.line(&format!("{}:", f));
                out.indent();
                out.line("type: string");
                out.dedent();
            }
            out.dedent();
        }
        ir::CommandInput::Typed(slots) => {
            out.line("type: object");
            let required: Vec<&str> = slots
                .iter()
                .filter(|s| s.required)
                .map(|s| s.name.as_str())
                .collect();
            if !required.is_empty() {
                out.line(&format!("required: [{}]", required.join(", ")));
            }
            out.line("properties:");
            out.indent();
            for slot in slots {
                out.line(&format!("{}:", slot.name));
                out.indent();
                emit_schema_inline(out, &slot.type_ref);
                out.dedent();
            }
            out.dedent();
        }
    }
}

pub(crate) fn emit_schema_inline(out: &mut YamlEmitter, ty: &ir::TypeRef) {
    match ty {
        ir::TypeRef::Builtin(b) => {
            let (kind, fmt) = builtin_to_openapi(b);
            out.line(&format!("type: {}", kind));
            if let Some(fmt) = fmt {
                out.line(&format!("format: {}", fmt));
            }
        }
        ir::TypeRef::UserDefined(qn) => {
            out.line(&format!("$ref: '#/components/schemas/{}'", qn.name));
        }
        ir::TypeRef::EnumRef(qn) => {
            out.line(&format!("$ref: '#/components/schemas/{}'", qn.name));
        }
        ir::TypeRef::Many(inner) => {
            out.line("type: array");
            out.line("items:");
            out.indent();
            emit_schema_inline(out, inner);
            out.dedent();
        }
        ir::TypeRef::Capability(cap) => match cap {
            ir::CapabilityRef::File(_) => {
                out.line("type: string");
                out.line("format: binary");
                out.line("x-lazuli-capability: File");
            }
            ir::CapabilityRef::PII(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: PII");
            }
            ir::CapabilityRef::Hashed(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Hashed");
            }
            ir::CapabilityRef::Encrypted(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Encrypted");
            }
            ir::CapabilityRef::E2ee(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: E2ee");
            }
            ir::CapabilityRef::Token(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Token");
            }
        },
        ir::TypeRef::Unresolved(s) => {
            out.line("type: string");
            out.line(&format!("x-lazuli-unresolved: {}", quote_value(s)));
        }
    }
}

pub(crate) fn emit_resource_schema(out: &mut YamlEmitter, r: &ir::Resource) {
    out.line(&format!("{}:", r.name));
    out.indent();
    out.line("type: object");
    out.line("properties:");
    out.indent();
    for f in &r.fields {
        out.line(&format!("{}:", f.name));
        out.indent();
        emit_schema_inline(out, &f.type_ref);
        if !f.required {
            out.line("nullable: true");
        }
        out.dedent();
    }
    out.dedent();
    out.dedent();
}

pub(crate) fn emit_record_schema(out: &mut YamlEmitter, r: &ir::Record) {
    out.line(&format!("{}:", r.name));
    out.indent();
    out.line("type: object");
    out.line("properties:");
    out.indent();
    for f in &r.fields {
        out.line(&format!("{}:", f.name));
        out.indent();
        emit_schema_inline(out, &f.type_ref);
        if !f.required {
            out.line("nullable: true");
        }
        out.dedent();
    }
    out.dedent();
    out.dedent();
}

pub(crate) fn emit_enum_schema(out: &mut YamlEmitter, e: &ir::EnumDecl) {
    out.line(&format!("{}:", e.name));
    out.indent();
    out.line("type: string");
    out.line(&format!(
        "enum: [{}]",
        e.variants
            .iter()
            .map(|v| v.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.dedent();
}

pub(crate) fn builtin_to_openapi(b: &ir::BuiltinType) -> (&'static str, Option<&'static str>) {
    use ir::BuiltinType::*;
    match b {
        Text => ("string", None),
        Integer => ("integer", None),
        Decimal => ("number", None),
        Boolean => ("boolean", None),
        Id => ("string", Some("uuid")),
        DateTime => ("string", Some("date-time")),
        Date => ("string", Some("date")),
        Json => ("object", None),
        SemanticEmail => ("string", Some("email")),
        SemanticPhone => ("string", None),
        SemanticUrl => ("string", Some("uri")),
        SemanticUuid => ("string", Some("uuid")),
        SemanticMoney { .. } => ("number", None),
        SemanticCurrency => ("string", None),
        // W1 GAP-04 — HexColor surfaces as a plain string (OpenAPI has no
        // `color` format; the `#RRGGBB`/`#RGB` shape is enforced at the
        // runtime decode boundary, not the schema format hint).
        SemanticHexColor => ("string", None),
        // W1 GAP-05 — Percentage surfaces as a number; the 0..=100 range
        // guard lives in the runtime carrier, not the OpenAPI format slot.
        SemanticPercentage => ("number", None),
        // Batch E — PositiveDecimal surfaces as a number, NonNegativeInt as
        // an integer; the `> 0` / `>= 0` guards live in the runtime carrier.
        SemanticPositiveDecimal => ("number", None),
        SemanticNonNegativeInt => ("integer", None),
        // GeoPoint follow-up — `@semantic.GeoPoint` carries
        // `{ lat, lng }`. OpenAPI does not have a `geography` format,
        // so we surface it as a generic `object` here; codegen-go is
        // the consumer that materialises the PostGIS-aware shape.
        SemanticGeoPoint => ("object", None),
        // B3 — plugin-contributed `@semantic.<Name>` projects to the
        // carrier's OpenAPI shape. The plugin owns validator + format
        // affinity; the wire surface here is the carrier alone.
        SemanticPluginType { carrier, .. } => builtin_to_openapi(carrier),
        CapSecret => ("string", None),
        CapFile => ("string", Some("binary")),
    }
}
