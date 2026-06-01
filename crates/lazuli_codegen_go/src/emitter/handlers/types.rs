//! Type-string shapers — lower IR `TypeRef` shapes into the Go
//! identifiers the emitted stub will use.
//!
//! Two concerns live here:
//! - `go_type_for_stub` (+ its `_builtin` / `_capability` / `_unresolved`
//!   helpers) — lowers an IR `TypeRef` into a Go type literal as it
//!   should appear in the stub signature (e.g. `lazuli.Email`,
//!   `[]string`, `CreateCustomerInput`).
//! - `qualify_generated_stub_type` + `is_builtin_stub_type` — decide
//!   whether a raw Go type already lives in the runtime / stdlib
//!   namespace (left verbatim) or needs the `<feature>gen.` alias prefix
//!   so the stub can compile without importing the gen package
//!   directly.
//!
//! Boundary: nothing here touches the IR walker — it only consumes
//! `TypeRef` shapes and emits strings. The walker in `collect.rs`
//! drives the calls.

use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};

use super::paths::pascal_case;

pub(super) fn qualify_generated_stub_type(raw: &str, gen_alias: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("[]") {
        let (qualified, used) = qualify_generated_stub_type(inner, gen_alias);
        return (format!("[]{qualified}"), used);
    }
    if is_builtin_stub_type(trimmed) || trimmed.contains('.') || trimmed.starts_with("map[") {
        return (trimmed.to_owned(), false);
    }
    if trimmed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return (format!("{gen_alias}.{trimmed}"), true);
    }
    (trimmed.to_owned(), false)
}

fn is_builtin_stub_type(raw: &str) -> bool {
    matches!(
        raw,
        "any" | "string" | "bool" | "int" | "int64" | "float64" | "struct{}" | "error"
    )
}

pub(super) fn go_type_for_stub(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => go_type_for_builtin(builtin),
        TypeRef::Capability(capability) => go_type_for_capability(capability).to_owned(),
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) if qname.name.trim() == "Empty" => {
            "struct{}".to_string()
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => pascal_case(&qname.name),
        TypeRef::Many(inner) => format!("[]{}", go_type_for_stub(inner)),
        TypeRef::Unresolved(raw) if raw.trim() == "Empty" => "struct{}".to_string(),
        TypeRef::Unresolved(raw) => go_type_for_unresolved(raw),
    }
}

fn go_type_for_builtin(builtin: &BuiltinType) -> String {
    match builtin {
        BuiltinType::Id => "lazuli.ID".to_string(),
        BuiltinType::Text => "string".to_string(),
        BuiltinType::Boolean => "bool".to_string(),
        BuiltinType::Integer => "int64".to_string(),
        BuiltinType::Decimal => "float64".to_string(),
        BuiltinType::Date => "lazuli.Date".to_string(),
        BuiltinType::DateTime => "lazuli.Time".to_string(),
        BuiltinType::Json => "lazuli.JSON".to_string(),
        BuiltinType::SemanticEmail => "lazuli.Email".to_string(),
        BuiltinType::SemanticMoney { .. } => "lazuli.Money".to_string(),
        BuiltinType::SemanticPhone => "lazuli.Phone".to_string(),
        BuiltinType::SemanticUrl => "lazuli.URL".to_string(),
        BuiltinType::SemanticUuid => "lazuli.UUID".to_string(),
        BuiltinType::SemanticCurrency => "lazuli.Currency".to_string(),
        BuiltinType::SemanticGeoPoint => "any".to_string(),
        // W1 GAP-04/05 — named runtime carriers with built-in validation.
        BuiltinType::SemanticHexColor => "lazuli.HexColor".to_string(),
        BuiltinType::SemanticPercentage => "lazuli.Percentage".to_string(),
        // Batch E — strict-positive / non-negative range carriers.
        BuiltinType::SemanticPositiveDecimal => "lazuli.PositiveDecimal".to_string(),
        BuiltinType::SemanticNonNegativeInt => "lazuli.NonNegativeInt".to_string(),
        // B3 — plugin-contributed `@semantic.<Name>` lowers to the
        // carrier's Go type for handler stubs (no plugin import here;
        // the validate tag wired in resource emission handles the
        // adapter dispatch).
        BuiltinType::SemanticPluginType { carrier, .. } => go_type_for_builtin(carrier),
        BuiltinType::CapSecret => "lazuli.Secret".to_string(),
        BuiltinType::CapFile => "any".to_string(),
    }
}

fn go_type_for_capability(capability: &CapabilityRef) -> &'static str {
    match capability {
        CapabilityRef::Hashed(_) => "lazuli.HashedRef",
        CapabilityRef::Encrypted(_) => "lazuli.EncryptedRef",
        // `@cap.E2ee` and `@cap.Encrypted` share the byte envelope.
        CapabilityRef::E2ee(_) => "lazuli.EncryptedRef",
        CapabilityRef::Token(_) => "lazuli.TokenRef",
        CapabilityRef::PII(_) => "string",
        CapabilityRef::File(_) => "any",
    }
}

fn go_type_for_unresolved(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_suffix("[]") {
        return format!("[]{}", go_type_for_unresolved(inner));
    }

    match trimmed {
        "ID" | "Id" => "lazuli.ID".to_owned(),
        "Text" | "String" => "string".to_owned(),
        "Boolean" | "Bool" => "bool".to_owned(),
        "Integer" | "Int" => "int64".to_owned(),
        "Decimal" | "Float" => "float64".to_owned(),
        "Date" => "lazuli.Date".to_owned(),
        "DateTime" => "lazuli.Time".to_owned(),
        "Json" | "JSON" => "lazuli.JSON".to_owned(),
        "@semantic.Email" => "lazuli.Email".to_owned(),
        "@semantic.Money" => "lazuli.Money".to_owned(),
        "@semantic.Phone" => "lazuli.Phone".to_owned(),
        "@semantic.Url" | "@semantic.URL" => "lazuli.URL".to_owned(),
        "@semantic.Uuid" | "@semantic.UUID" => "lazuli.UUID".to_owned(),
        "@semantic.Currency" => "lazuli.Currency".to_owned(),
        "@cap.Secret" => "lazuli.Secret".to_owned(),
        _ if trimmed.starts_with("@cap.Hashed") => "lazuli.HashedRef".to_owned(),
        _ if trimmed.starts_with("@cap.Encrypted") => "lazuli.EncryptedRef".to_owned(),
        _ if trimmed.starts_with("@cap.Token") => "lazuli.TokenRef".to_owned(),
        _ if trimmed.starts_with("@cap.File") => "any".to_owned(),
        _ if trimmed.starts_with('@') => "any".to_owned(),
        _ => pascal_case(trimmed),
    }
}
