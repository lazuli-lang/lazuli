    // Codegen-TS enum/zod tests — split from `crates/lazuli_cli/src/tests.rs`.
    // Shared fixtures live in `tests/test_support.rs`; we pull them in via the
    // parent module's `test_support` re-export.

    use super::test_support::*;
    use crate::emit_feature_sdk_ts;

    #[test]
    fn positive_enum_emits_const_and_type_alias() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const ITEM_TYPE_VALUES = [\"doc\", \"decision\"] as const;")
        );
        assert!(output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"));
    }

    #[test]
    fn enum_metadata_options_golden_emits_typed_literal() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        let item_type = feature
            .enums
            .iter_mut()
            .find(|decl| decl.name == "ItemType")
            .expect("ItemType enum");
        item_type.variants[0].label_key = Some("item_doc".to_owned());
        item_type.variants[0].icon_key = Some("file-text".to_owned());
        item_type.variants[1].label_key = Some("item_decision".to_owned());
        item_type.variants[1].hint_key = Some("item_decision_hint".to_owned());
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const ITEM_TYPE_VALUES = [\"doc\", \"decision\"] as const;")
        );
        assert!(output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"));
        assert!(output.contains(
            "export const ITEM_TYPE_OPTIONS: ReadonlyArray<{\n  value: ItemType;\n  labelKey: string;\n  hintKey?: string;\n  iconKey?: string;\n}> = ["
        ));
        assert!(
            output
                .contains("  { value: \"doc\", labelKey: \"item_doc\", iconKey: \"file-text\" },")
        );
        assert!(output.contains(
            "  { value: \"decision\", labelKey: \"item_decision\", hintKey: \"item_decision_hint\" },"
        ));
    }

    #[test]
    fn enum_without_metadata_golden_omits_options() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("export const ITEM_TYPE_VALUES"));
        assert!(!output.contains("ITEM_TYPE_OPTIONS"));
    }

    #[test]
    fn positive_enum_field_uses_lifted_type() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  type: ItemType;"));
        assert!(!output.contains("  type: unknown;"));
    }

    #[test]
    fn positive_list_of_text_emits_array() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  tags: string[];"));
    }

    #[test]
    fn positive_list_of_enum_emits_typed_array() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  categories: ItemType[];"));
    }

    #[test]
    fn rich_zod_base_emits_enum_catalog() {
        let (_feature, module) = enum_sdk_fixture(false, false);
        let schema = crate::zod_base_for_type_ref(
            &lazuli_ir::TypeRef::EnumRef(local_qn("ItemType")),
            &module,
        );

        assert_eq!(schema, "z.enum([\"doc\", \"decision\"])");
    }

    #[test]
    fn rich_zod_base_emits_core_semantic_validators() {
        let (_feature, module) = enum_sdk_fixture(false, false);
        let cases = [
            (lazuli_ir::BuiltinType::SemanticEmail, "z.string().email()"),
            (
                lazuli_ir::BuiltinType::SemanticPhone,
                "/* TODO(@semantic.Phone): replace with pluggable locale-aware validator */ z.string().min(10).max(15)",
            ),
            (lazuli_ir::BuiltinType::SemanticUuid, "z.string().uuid()"),
            (lazuli_ir::BuiltinType::SemanticUrl, "z.string().url()"),
            // W1 GAP-04 — HexColor emits the `#RRGGBB`/`#RGB` regex.
            (
                lazuli_ir::BuiltinType::SemanticHexColor,
                "z.string().regex(/^#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})$/)",
            ),
            // W1 GAP-05 — Percentage emits the 0..=100 range guard.
            (
                lazuli_ir::BuiltinType::SemanticPercentage,
                "z.number().min(0).max(100)",
            ),
        ];

        for (builtin, expected) in cases {
            assert_eq!(
                crate::zod_base_for_type_ref(&lazuli_ir::TypeRef::Builtin(builtin), &module),
                expected
            );
        }
    }

    #[test]
    fn rich_zod_base_emits_plugin_semantic_digit_patterns() {
        let (_feature, module) = enum_sdk_fixture(false, false);
        let cpf = lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
            plugin: "@lazuli/plugin-scalars-br".to_owned(),
            name: "BrazilianCPF".to_owned(),
            carrier: Box::new(lazuli_ir::BuiltinType::Text),
            validator: "ValidateCPF".to_owned(),
            go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
            ts_package: "@lazuli/plugin-scalars-br".to_owned(),
            error_code: "cpf_invalid".to_owned(),
            message_key: String::new(),
            ts_validator: String::new(),
        });
        let cnpj = lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
            plugin: "@lazuli/plugin-scalars-br".to_owned(),
            name: "BrazilianCNPJ".to_owned(),
            carrier: Box::new(lazuli_ir::BuiltinType::Text),
            validator: "ValidateCNPJ".to_owned(),
            go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
            ts_package: "@lazuli/plugin-scalars-br".to_owned(),
            error_code: "cnpj_invalid".to_owned(),
            message_key: String::new(),
            ts_validator: String::new(),
        });
        let other = lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
            plugin: "@lazuli/plugin-scalars-br".to_owned(),
            name: "BrazilianCEP".to_owned(),
            carrier: Box::new(lazuli_ir::BuiltinType::Text),
            validator: "ValidateCEP".to_owned(),
            go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
            ts_package: "@lazuli/plugin-scalars-br".to_owned(),
            error_code: "cep_invalid".to_owned(),
            message_key: String::new(),
            ts_validator: String::new(),
        });

        assert_eq!(
            crate::zod_base_for_type_ref(&cpf, &module),
            "/* @semantic.BrazilianCPF: basic digit-only pattern; checksum validator belongs to the plugin */ z.string().regex(/^\\d{11}$/)"
        );
        assert_eq!(
            crate::zod_base_for_type_ref(&cnpj, &module),
            "/* @semantic.BrazilianCNPJ: basic digit-only pattern; checksum validator belongs to the plugin */ z.string().regex(/^\\d{14}$/)"
        );
        assert_eq!(
            crate::zod_base_for_type_ref(&other, &module),
            "/* TODO(@semantic.BrazilianCEP): pluggable Zod validator */ z.string()"
        );
    }

    #[test]
    fn feature_zod_emits_enum_and_semantic_command_schema() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.commands.push(command_with_typed_input(
            "create",
            vec![
                typed_slot(
                    "type",
                    lazuli_ir::TypeRef::EnumRef(local_qn("ItemType")),
                    true,
                ),
                typed_slot(
                    "email",
                    lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticEmail),
                    true,
                ),
            ],
        ));
        module.features = vec![feature.clone()];

        let output = crate::emit_feature_zod_ts(&feature, &module);

        assert!(
            output.contains("type: z.enum([\"doc\", \"decision\"]),"),
            "expected enum zod schema, got:\n{output}"
        );
        assert!(
            output.contains("email: z.string().email(),"),
            "expected email zod schema, got:\n{output}"
        );
        assert!(
            !output.contains("type: z.unknown()"),
            "enum slot must not fall back to unknown, got:\n{output}"
        );
    }

    #[test]
    fn negative_unreferenced_enum_not_emitted() {
        let (feature, module) = enum_sdk_fixture(true, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(!output.contains("UNUSED_VALUES"));
        assert!(!output.contains("export type Unused"));
    }

    #[test]
    fn user_defined_tagged_enum_field_still_lifts_to_typed_alias() {
        // Regression for review bug #3 (2026-05-15): fields like
        // `tier: CustomerTier = free` arrive as
        // `TypeRef::UserDefined({name: "ItemType"})` instead of
        // `EnumRef(...)` because the analyzer's resolve pass doesn't
        // always promote them. Before the fix, `ts_type_for_type_ref`
        // checked records but not enums under that arm and emitted
        // `tier: unknown` — making the SDK lose enum typing.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        // Replace the EnumRef-tagged `type` field with a UserDefined-
        // tagged one. Everything else identical.
        let resource = feature.resources.first_mut().expect("fixture resource");
        let type_field = resource
            .fields
            .iter_mut()
            .find(|f| f.name == "type")
            .expect("type field");
        type_field.type_ref = lazuli_ir::TypeRef::UserDefined(local_qn("ItemType"));
        // Module must mirror the feature's resource for the lookup.
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("  type: ItemType;"),
            "UserDefined-tagged enum field must resolve to the typed alias; got:\n{output}"
        );
        assert!(
            !output.contains("  type: unknown;"),
            "UserDefined-tagged enum field must not fall through to `unknown`; got:\n{output}"
        );
        assert!(
            output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"),
            "alias must still be emitted at the top of the file when only a UserDefined ref drives it; got:\n{output}"
        );
    }
