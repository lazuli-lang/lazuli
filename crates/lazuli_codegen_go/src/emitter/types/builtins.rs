//! Closed-catalog mapping for `BuiltinType` and `CapabilityRef`.
//!
//! Both kernels return `(go_type, Option<&'static str>)` so the top-level
//! [`super::go_type_for`] can lift the import path into an owned `String`
//! before handing it to the per-feature `ImportSet`.

use lazuli_ir::{BuiltinType, CapabilityRef};

pub(super) fn go_type_for_builtin(builtin: &BuiltinType) -> (String, Option<&'static str>) {
    match builtin {
        BuiltinType::Id => ("lazuli.ID".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Text => ("string".to_owned(), None),
        BuiltinType::Boolean => ("bool".to_owned(), None),
        BuiltinType::Integer => ("int64".to_owned(), None),
        BuiltinType::Decimal => ("float64".to_owned(), None),
        BuiltinType::Date => ("lazuli.Date".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::DateTime => ("lazuli.Time".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Json => ("lazuli.JSON".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticEmail => {
            ("lazuli.Email".to_owned(), Some("lazuli.dev/runtime/lazuli"))
        }
        // Per proposal `semantic-types-money-brazilian.md` v0.3:
        // `Money` is the currency-aware semantic type. The Go field
        // type is the rich struct `lazuli.MoneyValue` (decimal +
        // currency), not the legacy `int64` alias `lazuli.Money`
        // which is preserved for backward compatibility.
        BuiltinType::SemanticMoney { .. } => (
            "lazuli.MoneyValue".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticPhone => {
            ("lazuli.Phone".to_owned(), Some("lazuli.dev/runtime/lazuli"))
        }
        BuiltinType::SemanticUrl => ("lazuli.URL".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticUuid => ("lazuli.UUID".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticCurrency => (
            "lazuli.Currency".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // GeoPoint follow-up — `@semantic.GeoPoint` resolves to
        // `postgis.Point` via the lightweight `cridenour/go-postgis`
        // binding (chosen per `codegen-lazuli-go.md` §10.1; revisit if
        // a future bucket needs `twpayne/go-geom`'s broader WKT/WKB
        // roundtrip). Resource fields carrying this type also emit a
        // `db:"…,type:geography(point,4326)"` tag (proposal §3.1) and
        // a `GIST` index in the DDL migration (proposal §9.2).
        BuiltinType::SemanticGeoPoint => (
            "postgis.Point".to_owned(),
            Some("github.com/cridenour/go-postgis"),
        ),
        // W1 GAP-04 — `@semantic.HexColor` resolves to the named runtime
        // type `lazuli.HexColor` (string carrier). Its `UnmarshalJSON`
        // emits the server-side regex guard at the decode boundary, so a
        // malformed `#GGG` surfaces as a `validation_failed` envelope.
        BuiltinType::SemanticHexColor => (
            "lazuli.HexColor".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // W1 GAP-05 — `@semantic.Percentage` resolves to the named runtime
        // type `lazuli.Percentage` (float64 carrier). Its `UnmarshalJSON`
        // emits the `0 <= value <= 100` range check at the decode boundary.
        BuiltinType::SemanticPercentage => (
            "lazuli.Percentage".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // B3 — plugin-contributed `@semantic.<Name>` materialises as
        // the carrier's Go type. The validate tag (emitted at
        // resource-field-tag time per resource.rs) drives the
        // runtime adapter dispatch via the validator key
        // `<plugin.name>.<validator>`. v1 closed-catalog carrier is
        // `String` → Go `string` (no import). Wider carriers gated by
        // a separate proposal.
        BuiltinType::SemanticPluginType { carrier, .. } => go_type_for_builtin(carrier),
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

pub(super) fn go_type_for_capability(cap: &CapabilityRef) -> (String, Option<&'static str>) {
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
        // `@cap.E2ee` shares the byte envelope shape with
        // `@cap.Encrypted` — both store opaque ciphertext. The
        // semantic distinction (server cannot decrypt) is enforced
        // at codegen call-site time, not by the column type.
        CapabilityRef::E2ee(_) => (
            "lazuli.EncryptedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::Token(_) => (
            "lazuli.TokenRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::PII(_) => ("string".to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::{cross_ref_module, type_ctx};
    use super::super::{go_type_for, go_return_type_for};
    use lazuli_ir::{BuiltinType, TypeRef};

    #[test]
    fn id_maps_to_lazuli_id_with_lazuli_import() {
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Id), &ctx);
        assert_eq!(go, "lazuli.ID");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn text_maps_to_string_without_import() {
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Text), &ctx);
        assert_eq!(go, "string");
        assert_eq!(import, None);
    }

    #[test]
    fn semantic_email_maps_to_lazuli_email() {
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticEmail), &ctx);
        assert_eq!(go, "lazuli.Email");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn semantic_currency_maps_to_lazuli_currency() {
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticCurrency), &ctx);
        assert_eq!(go, "lazuli.Currency");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn semantic_hexcolor_maps_to_lazuli_hexcolor() {
        // W1 GAP-04 — Text-backed colour semantic resolves to the named
        // runtime type whose UnmarshalJSON enforces the `#RRGGBB`/`#RGB`
        // regex server-side.
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticHexColor), &ctx);
        assert_eq!(go, "lazuli.HexColor");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn semantic_percentage_maps_to_lazuli_percentage() {
        // W1 GAP-05 — Decimal-backed ratio semantic resolves to the named
        // runtime type whose UnmarshalJSON enforces `0 <= value <= 100`.
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticPercentage), &ctx);
        assert_eq!(go, "lazuli.Percentage");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn many_wraps_inner_with_slice_prefix() {
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let inner = TypeRef::Builtin(BuiltinType::Id);
        let (go, import) = go_type_for(&TypeRef::Many(Box::new(inner)), &ctx);
        assert_eq!(go, "[]lazuli.ID");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn capability_file_maps_to_storage_file_ref() {
        // Legacy flat variant carries no args and resolves to the same
        // typed alias as the modern `CapabilityRef::File` form.
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::CapFile), &ctx);
        assert_eq!(go, "storage.FileRef");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli/storage"));
    }

    #[test]
    fn semantic_geopoint_maps_to_postgis_point() {
        // GeoPoint follow-up — geo codegen materialises via cridenour/go-postgis.
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticGeoPoint), &ctx);
        assert_eq!(go, "postgis.Point");
        assert_eq!(import.as_deref(), Some("github.com/cridenour/go-postgis"));
    }

    #[test]
    fn semantic_plugin_type_emits_carrier_go_type() {
        // B3 — plugin-contributed `@semantic.<Name>` materialises as
        // the carrier's Go type (v1 closed catalog: `String` → `string`,
        // no import). The validate-tag emission lives in `resource.rs`
        // and is golden-tested via `plugin_semantic_validate_tag` plus
        // the the canonical pilot pipeline.
        // See `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let plugin_type = TypeRef::Builtin(BuiltinType::SemanticPluginType {
            plugin: "@lazuli/plugin-scalars-br".to_owned(),
            name: "BrazilianCPF".to_owned(),
            carrier: Box::new(BuiltinType::Text),
            validator: "ValidateCPF".to_owned(),
            go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
            ts_package: "@lazuli/plugin-scalars-br".to_owned(),
            error_code: "cpf_invalid".to_owned(),
            message_key: String::new(),
            ts_validator: String::new(),
        });
        let (go, import) = go_type_for(&plugin_type, &ctx);
        assert_eq!(go, "string");
        assert!(import.is_none());
    }

    #[test]
    fn implicit_empty_output_maps_to_struct_literal() {
        // `output Empty` is the canonical no-body response shape for
        // APIs/commands that deliberately return no payload. When the
        // app has not declared a real `Empty` record, emit Go's unit
        // shape directly instead of requiring dummy app records.
        use lazuli_ir::QualifiedName;
        let module = cross_ref_module();
        let index = crate::emitter::cross_feature::CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "Empty".to_owned(),
        };
        let (go, import) = go_return_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "struct{}");
        assert_eq!(import, None);
    }
}
