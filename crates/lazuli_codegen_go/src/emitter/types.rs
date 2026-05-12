//! Closed-catalog `TypeRef` → Go type mapping. Centralised so per-kind
//! walkers in `module.rs` / `resource.rs` / etc. never inline the
//! mapping. Each entry returns `(go_type, optional_import_path)` so the
//! caller can register the import on the file's `ImportSet`.
//!
//! Per proposal §6.3 the `@semantic.GeoPoint` row would map to
//! `postgis.Point` from `github.com/cridenour/go-postgis`. The IR
//! currently has no `BuiltinType::SemanticGeoPoint` variant (verified
//! against `crates/lazuli_ir/src/lib.rs:445-471` on 2026-05-11); the
//! mapping ships once Hostpoint §9.1 lands the IR slot. See proposal
//! §10.1 for the binding-library decision still pending.

use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};

/// Resolve a `TypeRef` to its concrete Go-source form plus the import
/// path that must be present on the consuming file (if any). The
/// `'static str` lifetime on the optional import keeps the mapping
/// table data-only; user-defined types return `None` because their
/// declaration lives in the same generated package.
pub fn go_type_for(ty: &TypeRef) -> (String, Option<&'static str>) {
    match ty {
        TypeRef::Builtin(builtin) => go_type_for_builtin(*builtin),
        TypeRef::UserDefined(qname) => {
            // Same-package reference: render the bare PascalCase name.
            // Cross-feature references are not yet handled (Tier 4
            // surfaces `QualifiedName.feature`); when they land we
            // emit `<feature>.<Name>` plus the feature's import.
            (qname.name.clone(), None)
        }
        TypeRef::EnumRef(qname) => (qname.name.clone(), None),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_type_for(inner);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::Unresolved(name) => {
            // Surface as a string fallback; the §6.2.1 error catalog
            // (cell I4) will upgrade this to a hard failure under
            // `--check`. For now the unresolved name is preserved
            // verbatim as a Go-typed placeholder so the output is
            // still syntactically a Go identifier.
            (sanitise_go_ident(name), None)
        }
        TypeRef::Capability(cap) => go_type_for_capability(cap),
    }
}

fn go_type_for_builtin(builtin: BuiltinType) -> (String, Option<&'static str>) {
    match builtin {
        BuiltinType::Id => ("lazuli.ID".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Text => ("string".to_owned(), None),
        BuiltinType::Boolean => ("bool".to_owned(), None),
        BuiltinType::Integer => ("int64".to_owned(), None),
        BuiltinType::Decimal => ("float64".to_owned(), None),
        BuiltinType::Date => ("lazuli.Date".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::DateTime => ("lazuli.Time".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Json => ("lazuli.JSON".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticEmail => (
            "lazuli.Email".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticMoney => (
            "lazuli.Money".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticPhone => (
            "lazuli.Phone".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticUrl => ("lazuli.URL".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticUuid => (
            "lazuli.UUID".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::CapSecret => (
            "lazuli.Secret".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::CapFile => {
            // Legacy flat `@cap.File` (no args); modern typed form is
            // `TypeRef::Capability(CapabilityRef::File)`. Both resolve
            // to the same `storage.FileRef` Go type.
            (
                "storage.FileRef".to_owned(),
                Some("lazuli.dev/runtime/lazuli/storage"),
            )
        }
    }
}

fn go_type_for_capability(cap: &CapabilityRef) -> (String, Option<&'static str>) {
    match cap {
        CapabilityRef::File(_) => (
            "storage.FileRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli/storage"),
        ),
        CapabilityRef::Hashed(_) => (
            "lazuli.HashedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::Encrypted(_) => (
            "lazuli.EncryptedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::Token(_) => (
            "lazuli.TokenRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
    }
}

/// Cheap sanitiser for unresolved identifiers so we never emit raw
/// `@plugin/foo` text into Go source. The §6.2.1 error catalog (cell
/// I4) replaces this with a hard error.
fn sanitise_go_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "lazuliUnresolved".to_owned();
    }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::QualifiedName;

    #[test]
    fn id_maps_to_lazuli_id_with_lazuli_import() {
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Id));
        assert_eq!(go, "lazuli.ID");
        assert_eq!(import, Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn text_maps_to_string_without_import() {
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Text));
        assert_eq!(go, "string");
        assert_eq!(import, None);
    }

    #[test]
    fn semantic_email_maps_to_lazuli_email() {
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticEmail));
        assert_eq!(go, "lazuli.Email");
        assert_eq!(import, Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn many_wraps_inner_with_slice_prefix() {
        let inner = TypeRef::Builtin(BuiltinType::Id);
        let (go, import) = go_type_for(&TypeRef::Many(Box::new(inner)));
        assert_eq!(go, "[]lazuli.ID");
        assert_eq!(import, Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn user_defined_renders_bare_name_without_import() {
        let qname = QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname));
        assert_eq!(go, "Customer");
        assert_eq!(import, None);
    }

    #[test]
    fn capability_file_maps_to_storage_file_ref() {
        // Legacy flat variant carries no args and resolves to the same
        // typed alias as the modern `CapabilityRef::File` form.
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::CapFile));
        assert_eq!(go, "storage.FileRef");
        assert_eq!(import, Some("lazuli.dev/runtime/lazuli/storage"));
    }
}
