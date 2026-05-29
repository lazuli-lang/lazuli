//! TypeRef → Go-type emission helpers for the `returns_list_001` rule.
//!
//! Splits two concerns: rendering a `TypeRef` back as a human-readable
//! label that mirrors the .lzi surface (`type_ref_label` + `builtin_label`),
//! and rendering it as a Go type that matches the generated SDK stubs
//! (`go_type_for_stub` + `go_type_for_builtin` + `go_type_for_capability`
//! + `qualify_generated_stub_type` + `is_builtin_stub_type`).
//!
//! Also hosts the small name-mangling helpers (`gen_package_name`,
//! `path_name_for`, `exported_func_name`, `pascal_case`, `is_acronym`,
//! `split_words`) used to turn DSL handler names into Go identifiers.

use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};

pub(super) fn type_ref_label(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => builtin_label(builtin).to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("list {}", type_ref_label(inner)),
        TypeRef::Capability(_) => "@cap.*".to_owned(),
        TypeRef::Unresolved(raw) => raw.clone(),
    }
}

pub(super) fn builtin_label(builtin: &BuiltinType) -> &'static str {
    match builtin {
        BuiltinType::Id => "ID",
        BuiltinType::Text => "Text",
        BuiltinType::Boolean => "Boolean",
        BuiltinType::Integer => "Integer",
        BuiltinType::Decimal => "Decimal",
        BuiltinType::Date => "Date",
        BuiltinType::DateTime => "DateTime",
        BuiltinType::Json => "JSON",
        BuiltinType::SemanticEmail => "Email",
        BuiltinType::SemanticMoney { .. } => "Money",
        BuiltinType::SemanticPhone => "Phone",
        BuiltinType::SemanticUrl => "Url",
        BuiltinType::SemanticUuid => "Uuid",
        BuiltinType::SemanticCurrency => "Currency",
        BuiltinType::SemanticGeoPoint => "GeoPoint",
        BuiltinType::SemanticHexColor => "HexColor",
        BuiltinType::SemanticPercentage => "Percentage",
        BuiltinType::SemanticPluginType { .. } => "SemanticPluginType",
        BuiltinType::CapSecret => "@cap.Secret",
        BuiltinType::CapFile => "@cap.File",
    }
}

pub(super) fn go_type_for_stub(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => go_type_for_builtin(builtin),
        TypeRef::Capability(capability) => go_type_for_capability(capability).to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) if qn.name.trim() == "Empty" => {
            "struct{}".to_owned()
        }
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => pascal_case(&qn.name),
        TypeRef::Many(inner) => format!("[]{}", go_type_for_stub(inner)),
        TypeRef::Unresolved(raw) if raw.trim() == "Empty" => "struct{}".to_owned(),
        TypeRef::Unresolved(raw) => raw.trim().to_owned(),
    }
}

pub(super) fn go_type_for_builtin(builtin: &BuiltinType) -> String {
    match builtin {
        BuiltinType::Id => "lazuli.ID".to_owned(),
        BuiltinType::Text => "string".to_owned(),
        BuiltinType::Boolean => "bool".to_owned(),
        BuiltinType::Integer => "int64".to_owned(),
        BuiltinType::Decimal => "float64".to_owned(),
        BuiltinType::Date => "lazuli.Date".to_owned(),
        BuiltinType::DateTime => "lazuli.Time".to_owned(),
        BuiltinType::Json => "lazuli.JSON".to_owned(),
        BuiltinType::SemanticEmail => "lazuli.Email".to_owned(),
        BuiltinType::SemanticMoney { .. } => "lazuli.Money".to_owned(),
        BuiltinType::SemanticPhone => "lazuli.Phone".to_owned(),
        BuiltinType::SemanticUrl => "lazuli.URL".to_owned(),
        BuiltinType::SemanticUuid => "lazuli.UUID".to_owned(),
        BuiltinType::SemanticCurrency => "lazuli.Currency".to_owned(),
        BuiltinType::SemanticGeoPoint => "any".to_owned(),
        // W1 GAP-04/05 — named runtime carriers with built-in validation.
        BuiltinType::SemanticHexColor => "lazuli.HexColor".to_owned(),
        BuiltinType::SemanticPercentage => "lazuli.Percentage".to_owned(),
        BuiltinType::SemanticPluginType { carrier, .. } => go_type_for_builtin(carrier),
        BuiltinType::CapSecret => "lazuli.Secret".to_owned(),
        BuiltinType::CapFile => "any".to_owned(),
    }
}

pub(super) fn go_type_for_capability(capability: &CapabilityRef) -> &'static str {
    match capability {
        CapabilityRef::Hashed(_) => "lazuli.HashedRef",
        CapabilityRef::Encrypted(_) | CapabilityRef::E2ee(_) => "lazuli.EncryptedRef",
        CapabilityRef::Token(_) => "lazuli.TokenRef",
        CapabilityRef::PII(_) => "string",
        CapabilityRef::File(_) => "any",
    }
}

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

pub(super) fn is_builtin_stub_type(raw: &str) -> bool {
    matches!(
        raw,
        "any" | "string" | "bool" | "int" | "int64" | "float64" | "struct{}" | "error"
    )
}

pub(super) fn gen_package_name(feature: &str) -> String {
    format!("{feature}gen")
}

pub(super) fn path_name_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

pub(super) fn exported_func_name(name: &str) -> String {
    let pascal = pascal_case(name);
    if pascal.is_empty() {
        return "Handler".to_owned();
    }
    if pascal
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("Handler{pascal}");
    }
    pascal
}

pub(super) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in split_words(s) {
        if word.is_empty() {
            continue;
        }
        if is_acronym(&word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for upper in first.to_uppercase() {
                out.push(upper);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

pub(super) fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

pub(super) fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (idx, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_ascii_uppercase() {
            let prev_lower =
                idx > 0 && (chars[idx - 1].is_ascii_lowercase() || chars[idx - 1].is_ascii_digit());
            let next_lower = idx + 1 < chars.len() && chars[idx + 1].is_ascii_lowercase();
            if !current.is_empty() && (prev_lower || next_lower) {
                if !next_lower {
                    current.push(ch);
                    continue;
                }
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}
