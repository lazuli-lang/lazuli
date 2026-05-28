//! Resource lookups + field/type formatters.
//!
//! Canonical resolvers used everywhere the SDK emitter needs to know
//! whether a `UserDefined` ref collapses to `ID` (FK column) or
//! carries the full row.
//!
//! Lifted out of the `types` god-file in the rails-style R9 split.

use crate::casing::pascal_case;

use super::super::format::find_enum_decl;

pub(crate) fn find_resource<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::Resource> {
    module
        .features
        .iter()
        .filter(|feature| name.feature.as_ref().is_none_or(|n| n == &feature.name))
        .flat_map(|feature| feature.resources.iter())
        .find(|resource| resource.name.eq_ignore_ascii_case(&name.name))
}

pub(super) fn resource_ts_name(
    name: &lazuli_ir::QualifiedName,
    module: &lazuli_ir::Module,
) -> String {
    find_resource(module, name)
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| pascal_case(&name.name))
}

pub(crate) fn resource_field_ts_name(
    field: &lazuli_ir::Field,
    module: &lazuli_ir::Module,
) -> String {
    if is_resource_ref(&field.type_ref, module) && !field.name.ends_with("_id") {
        format!("{}_id", field.name)
    } else {
        field.name.clone()
    }
}

pub(crate) fn resource_field_ts_type(
    field: &lazuli_ir::Field,
    module: &lazuli_ir::Module,
) -> String {
    if is_resource_ref(&field.type_ref, module) {
        "ID".to_owned()
    } else {
        ts_type_for_type_ref(&field.type_ref, module)
    }
}

pub(crate) fn is_resource_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> bool {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) => module
            .features
            .iter()
            .flat_map(|feature| feature.resources.iter())
            .any(|resource| resource.name.eq_ignore_ascii_case(&name.name)),
        _ => false,
    }
}

pub(crate) fn ts_type_for_type_ref(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Id => "ID".to_owned(),
            lazuli_ir::BuiltinType::Text
            | lazuli_ir::BuiltinType::SemanticEmail
            | lazuli_ir::BuiltinType::SemanticPhone
            | lazuli_ir::BuiltinType::SemanticUrl
            | lazuli_ir::BuiltinType::SemanticUuid
            | lazuli_ir::BuiltinType::SemanticCurrency
            // W1 GAP-04 — HexColor is a text carrier.
            | lazuli_ir::BuiltinType::SemanticHexColor
            | lazuli_ir::BuiltinType::CapSecret => "string".to_owned(),
            // B3 — plugin-contributed `@semantic.<Name>` projects to
            // the brand alias name (e.g. `BrazilianCPF`). The SDK
            // emitter (`emit_feature_sdk_ts`) writes the
            // `export type <Name> = string;` line at file head so
            // every consuming interface picks up the alias.
            lazuli_ir::BuiltinType::SemanticPluginType { name, .. } => pascal_case(name),
            lazuli_ir::BuiltinType::Boolean => "boolean".to_owned(),
            // W1 GAP-05 — Percentage is Decimal-backed; TS wire is number.
            lazuli_ir::BuiltinType::Integer
            | lazuli_ir::BuiltinType::Decimal
            | lazuli_ir::BuiltinType::SemanticPercentage => "number".to_owned(),
            // Per `semantic-types-money-brazilian.md` v0.3 — Money is
            // the rich struct on the TS side too. `Money` interface
            // lives in `@lazuli/runtime`; downstream consumers get the
            // shape via the typed import.
            lazuli_ir::BuiltinType::SemanticMoney { .. } => "Money".to_owned(),
            lazuli_ir::BuiltinType::Date | lazuli_ir::BuiltinType::DateTime => "Time".to_owned(),
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::Hashed(_)
            | lazuli_ir::CapabilityRef::Encrypted(_)
            | lazuli_ir::CapabilityRef::E2ee(_)
            | lazuli_ir::CapabilityRef::Token(_)
            | lazuli_ir::CapabilityRef::PII(_) => "string".to_owned(),
            lazuli_ir::CapabilityRef::File(_) => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Many(inner) => format!("{}[]", ts_type_for_type_ref(inner, module)),
        lazuli_ir::TypeRef::EnumRef(name) => find_enum_decl(module, name)
            .map(|enum_decl| pascal_case(&enum_decl.name))
            .unwrap_or_else(|| "unknown".to_owned()),
        lazuli_ir::TypeRef::UserDefined(name) => {
            if is_resource_ref(type_ref, module) {
                "ID".to_owned()
            } else if let Some(enum_decl) = find_enum_decl(module, name) {
                // Enum referenced via UserDefined path. The parser
                // sometimes tags an enum field as UserDefined when the
                // analyzer hasn't promoted it to EnumRef (review bug #3,
                // 2026-05-15: `tier: CustomerTier = free` and
                // `source: CustomerSource = manual` both flowed as
                // UserDefined-with-no-record-match and lowered to
                // `unknown` — even though `CustomerTier`/`CustomerSource`
                // are declared above the resource block).
                pascal_case(&enum_decl.name)
            } else {
                module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(&name.name))
                    .map(|record| pascal_case(&record.name))
                    .unwrap_or_else(|| "unknown".to_owned())
            }
        }
        lazuli_ir::TypeRef::Unresolved(raw) => {
            if raw.starts_with("@cap.Hashed")
                || raw.starts_with("@cap.Encrypted")
                || raw.starts_with("@cap.Token")
                || raw == "@semantic.Email"
            {
                return "string".to_owned();
            }
            // Bare PascalCase fallback: the analyzer occasionally leaves
            // a `Unresolved("Foo")` even when `Foo` is a declared enum /
            // record / resource somewhere in the module (review bug #3,
            // 2026-05-15: `tier: CustomerTier = manual` flowed as
            // `Unresolved("CustomerTier")` and lowered to `unknown`
            // even though `CustomerTier` is declared three lines above).
            // Recover by walking the module's catalogs here so the TS
            // SDK preserves typing instead of falling to opaque
            // `unknown` whenever the analyzer's resolve pass misses an
            // edge case.
            if !raw.starts_with('@') {
                let synthetic = lazuli_ir::QualifiedName {
                    feature: None,
                    name: raw.clone(),
                };
                if let Some(enum_decl) = find_enum_decl(module, &synthetic) {
                    return pascal_case(&enum_decl.name);
                }
                if let Some(record) = module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(raw))
                {
                    return pascal_case(&record.name);
                }
                if module
                    .features
                    .iter()
                    .flat_map(|feature| feature.resources.iter())
                    .any(|resource| resource.name.eq_ignore_ascii_case(raw))
                {
                    return "ID".to_owned();
                }
            }
            "unknown".to_owned()
        }
    }
}
