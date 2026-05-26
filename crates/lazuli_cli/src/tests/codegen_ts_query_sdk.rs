    // Codegen-TS query/SDK shape + naming tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use super::test_support::*;
    use crate::emit_feature_sdk_ts;

    #[test]
    fn unresolved_bare_enum_name_recovers_to_typed_alias() {
        // Regression for the deeper fallback in `ts_type_for_type_ref`:
        // when the analyzer leaves a field as
        // `TypeRef::Unresolved("ItemType")` (no `@` prefix), the emitter
        // should still recover by walking the module's enum catalog
        // rather than emitting `unknown`. Without this branch, partial
        // analyzer failures would silently destroy the TS SDK's type
        // information.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        let resource = feature.resources.first_mut().expect("fixture resource");
        let type_field = resource
            .fields
            .iter_mut()
            .find(|f| f.name == "type")
            .expect("type field");
        type_field.type_ref = lazuli_ir::TypeRef::Unresolved("ItemType".to_owned());
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("  type: ItemType;"),
            "Unresolved-but-known-enum must self-heal to the typed alias; got:\n{output}"
        );
        assert!(!output.contains("  type: unknown;"));
    }

    #[test]
    fn dedup_enum_referenced_twice_emits_once() {
        let (feature, module) = enum_sdk_fixture(false, true);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert_eq!(occurrences(&output, "export const ITEM_TYPE_VALUES"), 1);
        assert_eq!(occurrences(&output, "export type ItemType"), 1);
    }

    #[test]
    fn query_view_sdk_uses_declared_returns_type() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "host".to_owned();
        feature.records.push(lazuli_ir::Record {
            name: "HostHomeRow".to_owned(),
            public_contract: None,
            fields: vec![field(
                "id",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
            )],
            discriminator_field: None,
            span_ref: None,
        });
        feature
            .queries
            .push(lazuli_ir::Query::Sql(lazuli_ir::SqlQuery {
                name: "host_home_view".to_owned(),
                sql_kind: lazuli_ir::SqlQueryKind::View,
                public_contract: None,
                params: vec![lazuli_ir::TypedSlot {
                    name: "user_id".to_owned(),
                    type_ref: lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
                    required: true,
                    constraints: lazuli_ir::FieldConstraints::default(),
                    validate_skip: false,
                }],
                scope: Vec::new(),
                scope_override: false,
                returns: lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::UserDefined(
                    local_qn("HostHomeRow"),
                ))),
                sql_path: "app/features/host/queries/host_home_view.sql".to_owned(),
                cache: None,
                policy: lazuli_ir::PolicyRef::None,
                policy_expr: None,
                policy_when_denied: None,
                previous_names: Vec::new(),
                span_ref: None,
            }));
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        // A.6 pluralization renames `Item` (the default fixture's resource)
        // → `Items`, so the canonical export is `listHostHomeViewItems`;
        // the legacy `listHostHomeViewHosts` is preserved as a deprecation
        // alias. Test asserts the typed `returns list <Record>` shape on
        // the canonical export.
        assert!(
            output.contains(
                "export const listHostHomeViewItems = defineQuery<{ user_id: ID }, HostHomeRow[]>(\"host.host_home_view\");"
            ),
            "query.view SDK should use the declared typed returns shape; got:\n{output}"
        );
    }

    #[test]
    fn feature_sdk_query_names_pluralize_resource_subjects_and_alias_legacy_exports() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "category".to_owned();
        feature.resources = vec![resource("Category", vec![])];
        feature.queries = vec![list_query("custom_service")];
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const listCustomServiceCategories = defineQuery"),
            "expected pluralized category list export, got:\n{output}"
        );
        assert!(
            output.contains("/** @deprecated use `listCustomServiceCategories` */"),
            "expected deprecated const alias doc, got:\n{output}"
        );
        assert!(
            output
                .contains("export const listCustomServiceCategorys = listCustomServiceCategories;"),
            "expected legacy const alias, got:\n{output}"
        );

        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "catalog".to_owned();
        feature.resources = vec![
            resource("CustomServiceCategory", vec![]),
            resource("Property", vec![]),
        ];
        feature.queries = vec![
            list_query("list_custom_service_categorys"),
            list_query("list_propertys"),
        ];
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const listCustomServiceCategories = defineQuery"),
            "expected legacy categorys shortname to normalize, got:\n{output}"
        );
        assert!(
            output
                .contains("export const listCustomServiceCategorys = listCustomServiceCategories;"),
            "expected legacy categorys alias, got:\n{output}"
        );
        assert!(
            output.contains("export const listProperties = defineQuery"),
            "expected legacy propertys shortname to normalize, got:\n{output}"
        );
        assert!(
            output.contains("export const listPropertys = listProperties;"),
            "expected legacy propertys alias, got:\n{output}"
        );
    }

    #[test]
    fn feature_sdk_query_names_dedup_resource_suffixes() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "host".to_owned();
        feature.resources = vec![resource("Host", vec![])];
        feature.queries = vec![list_query("pending_basic_details_hosts")];
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const listPendingBasicDetailsHosts = defineQuery"),
            "expected deduped host suffix, got:\n{output}"
        );
        assert!(
            output.contains(
                "export const listPendingBasicDetailsHostsHosts = listPendingBasicDetailsHosts;"
            ),
            "expected legacy suffix alias, got:\n{output}"
        );

        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "operations".to_owned();
        feature.resources = vec![resource("ServiceTransaction", vec![])];
        feature.queries = vec![list_query("mine_transactions_as_host")];
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const listMineHostServiceTransactions = defineQuery"),
            "expected embedded transaction noun cleanup, got:\n{output}"
        );
        assert!(
            output.contains(
                "export const listMineTransactionsAsHostOperationss = listMineHostServiceTransactions;"
            ),
            "expected legacy operations alias, got:\n{output}"
        );
    }

    #[test]
    fn feature_sdk_pure_read_list_commands_pluralize_return_resource() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "payment".to_owned();
        feature.resources = vec![resource("Payment", vec![])];
        feature.commands = vec![pure_read_list_command("list_payments", "Payment")];
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const listPayments = defineQuery"),
            "expected listPayments pure-read command export, got:\n{output}"
        );
        assert!(
            output.contains("export const listPaymentPayments = listPayments;"),
            "expected legacy pure-read command alias, got:\n{output}"
        );
    }
