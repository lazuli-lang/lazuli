    // Codegen-TS React hook emission tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use super::test_support::*;
    use crate::{emit_feature_barrel_ts, emit_feature_react_hooks_ts};

    #[test]
    fn react_hooks_emit_query_and_command_wrappers() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.commands.push(command(
            "create_item",
            lazuli_ir::CommandKind::Create,
            lazuli_ir::CommandInput::Typed(vec![typed_slot(
                "title",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text),
                true,
            )]),
            lazuli_ir::CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: local_qn("Item"),
                from_input: true,
                assignments: vec![],
            }),
        ));
        feature
            .commands
            .last_mut()
            .expect("create command")
            .previous_names
            .push("add_item".to_owned());
        feature.commands.push(command(
            "list_item_inbox",
            lazuli_ir::CommandKind::Returns,
            lazuli_ir::CommandInput::Typed(vec![typed_slot(
                "owner_id",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
                true,
            )]),
            lazuli_ir::CommandEffect::Returns(lazuli_ir::ReturnsEffect {
                return_type: lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::UserDefined(
                    local_qn("Item"),
                ))),
            }),
        ));
        feature
            .queries
            .push(lazuli_ir::Query::List(lazuli_ir::ListQuery {
                name: "list_items".to_owned(),
                public_contract: None,
                params: vec![typed_slot(
                    "limit",
                    lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Integer),
                    false,
                )],
                scope: vec![],
                scope_override: false,
                filters: vec![],
                order: vec![],
                paginate: None,
                modifier: None,
                cache: None,
                policy: lazuli_ir::PolicyRef::None,
                policy_expr: None,
                policy_when_denied: None,
                previous_names: vec![],
                span_ref: None,
                owner_scope_sql: None,
            }));
        feature
            .queries
            .push(lazuli_ir::Query::Lookup(lazuli_ir::LookupQuery {
                name: "lookup_my_item".to_owned(),
                public_contract: None,
                params: vec![],
                keys: vec![],
                scope: vec![],
                scope_override: false,
                filters: vec![],
                policy: lazuli_ir::PolicyRef::None,
                policy_expr: None,
                policy_when_denied: None,
                previous_names: vec![],
                span_ref: None,
                owner_scope_sql: None,
            }));
        feature
            .queries
            .push(lazuli_ir::Query::Lookup(lazuli_ir::LookupQuery {
                name: "by_id".to_owned(),
                public_contract: None,
                params: vec![],
                keys: vec![],
                scope: vec![],
                scope_override: false,
                filters: vec![],
                policy: lazuli_ir::PolicyRef::None,
                policy_expr: None,
                policy_when_denied: None,
                previous_names: vec![],
                span_ref: None,
                owner_scope_sql: None,
            }));
        module.features = vec![feature.clone()];

        let output = emit_feature_react_hooks_ts(&feature, &module);

        assert!(
            output.contains(
                "export function useListItemInbox(\n  args: QueryArgs<typeof listItemInbox>,\n  options: QueryOptions<typeof listItemInbox> = {},\n) {\n  return useLazuliQuery(listItemInbox, args, options);\n}"
            ),
            "pure-read commands must bind to useLazuliQuery; got:\n{output}"
        );
        assert!(
            output.contains(
                "export function useCreateItem(\n  options: CommandOptions<typeof createItem> = {},\n) {\n  return useLazuliCommand(createItem, options);\n}"
            ),
            "mutating commands must bind to useLazuliCommand; got:\n{output}"
        );
        assert!(
            output.contains(
                "export function useListItems(\n  args: QueryArgs<typeof listItems>,\n  options: QueryOptions<typeof listItems> = {},\n) {\n  return useLazuliQuery(listItems, args, options);\n}"
            ),
            "queries with args must expose a typed args parameter; got:\n{output}"
        );
        assert!(
            output.contains(
                "export function useLookupMyItem(\n  options: QueryOptions<typeof lookupMyItem> = {},\n) {\n  return useLazuliQuery(lookupMyItem, {}, options);\n}"
            ),
            "queries without args must pass an empty args object; got:\n{output}"
        );
        assert!(
            output.contains("/** @deprecated Use `useCreateItem` instead. */\nexport const useAddItem = useCreateItem;"),
            "renamed commands must keep deprecated hook aliases; got:\n{output}"
        );
        assert!(
            output.contains("/** @deprecated Use `useLookupItemByID` instead. */\nexport const useItemByID = useLookupItemByID;"),
            "legacy lookup hook aliases must stay available; got:\n{output}"
        );
    }

    #[test]
    fn react_hooks_omit_unused_runtime_imports_for_single_kind_features() {
        let (mut query_feature, mut query_module) = enum_sdk_fixture(false, false);
        query_feature
            .queries
            .push(lazuli_ir::Query::List(lazuli_ir::ListQuery {
                name: "list_items".to_owned(),
                public_contract: None,
                params: vec![],
                scope: vec![],
                scope_override: false,
                filters: vec![],
                order: vec![],
                paginate: None,
                modifier: None,
                cache: None,
                policy: lazuli_ir::PolicyRef::None,
                policy_expr: None,
                policy_when_denied: None,
                previous_names: vec![],
                span_ref: None,
                owner_scope_sql: None,
            }));
        query_module.features = vec![query_feature.clone()];

        let query_only = emit_feature_react_hooks_ts(&query_feature, &query_module);

        assert!(query_only.contains("  useLazuliQuery,"));
        assert!(query_only.contains("  type UseLazuliQueryOptions,"));
        assert!(!query_only.contains("useLazuliCommand"));
        assert!(!query_only.contains("UseLazuliCommandOptions"));
        assert!(!query_only.contains("type CommandInput"));

        let (mut command_feature, mut command_module) = enum_sdk_fixture(false, false);
        command_feature.commands.push(command(
            "create_item",
            lazuli_ir::CommandKind::Create,
            lazuli_ir::CommandInput::Empty,
            lazuli_ir::CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: local_qn("Item"),
                from_input: true,
                assignments: vec![],
            }),
        ));
        command_module.features = vec![command_feature.clone()];

        let command_only = emit_feature_react_hooks_ts(&command_feature, &command_module);

        assert!(command_only.contains("  useLazuliCommand,"));
        assert!(command_only.contains("  type UseLazuliCommandOptions,"));
        assert!(!command_only.contains("useLazuliQuery"));
        assert!(!command_only.contains("UseLazuliQueryOptions"));
        assert!(!command_only.contains("type QueryArgs"));
    }

    #[test]
    fn feature_barrel_reexports_generated_hooks() {
        let (mut feature, _) = enum_sdk_fixture(false, false);
        feature.commands.push(command(
            "create_item",
            lazuli_ir::CommandKind::Create,
            lazuli_ir::CommandInput::Empty,
            lazuli_ir::CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: local_qn("Item"),
                from_input: true,
                assignments: vec![],
            }),
        ));

        let output = emit_feature_barrel_ts(&feature);

        assert_eq!(
            output,
            "// Code generated by lazuli; DO NOT EDIT.\n\
             export * from \"./item.gen.js\";\n\
             export * from \"./item.react.gen.js\";\n"
        );
    }
