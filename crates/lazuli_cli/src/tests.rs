//! `lazuli_cli` test suite — pulled from main.rs's `#[cfg(test)] mod tests`
//! block as part of the W4.5 R2 split. Kept as `mod tests { ... }` so the
//! inner string-literal content (raw and non-raw) preserves its original
//! indentation; de-indenting would corrupt the multi-line .lzi fixture
//! strings the tests assert against.

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;
    use tempfile::TempDir;

    use crate::{
        Cli, Commands, DesignCommand, DesignExportTarget, DesignImportFormat, ExpandSet,
        GenerateKind, MigrateCommand, REGISTRY_TEMPLATE, add_missing_go_work_use_entries,
        app_template, default_module_name, emit_feature_barrel_ts, emit_feature_react_hooks_ts,
        emit_feature_sdk_ts, expand_canonical_source, inspect_canonical_source, inspect_json_value,
        new_command, parse_expand_set, pascal_case, pascal_case_project_name,
        render_inspect_symbol_lazuli, scaffold_bare, scaffold_from_template, templates,
        write_go_work_preserving_entries,
    };

    // NOTE: tests for `query_ident` / `strip_query_verb_prefix` (the
    // verb-prefix dedup added alongside the Hostpoint bug fix) cannot
    // live here because the `lazuli_cli` test binary currently fails to
    // compile on this branch's base (pre-existing `doctor::lzx::ir_stub`
    // field mismatches, unrelated to this change — see `cargo test -p
    // lazuli_cli` baseline). The behaviour is covered by the matching
    // tests in `lazuli_codegen_ts::lzx::tests` (the helper logic is
    // identical and was factored to mirror the CLI's local copy).

    #[test]
    fn go_work_preserve_adds_dist_go_without_dropping_runtime() {
        let original = "go 1.26.0\n\nuse (\n\t.\n\tc:/Users/lucas/lazuli/runtime/go\n)\n";
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";
        let updated = add_missing_go_work_use_entries(
            original,
            &crate::extract_go_work_use_entries(generated),
        );

        assert!(updated.contains("\t.\n"));
        assert!(updated.contains("\t./dist/go\n"));
        assert!(updated.contains("\tc:/Users/lucas/lazuli/runtime/go\n"));
        assert_eq!(updated.matches("./dist/go").count(), 1);
    }

    #[test]
    fn go_work_preserve_creates_missing_file_from_generated_contents() {
        let root = TempDir::new().unwrap();
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";

        write_go_work_preserving_entries(root.path(), generated).unwrap();

        let written = fs::read_to_string(root.path().join("go.work")).unwrap();
        assert_eq!(written, generated);
    }

    #[test]
    fn migrate_action_up_parses_target_flag() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "up",
            "--target",
            "20260513_001_account_user",
            "--yes",
        ])
        .unwrap();

        let Commands::Migrate {
            sub: MigrateCommand::Up { target, yes: true },
        } = cli.command
        else {
            panic!("expected migrate up command");
        };
        assert_eq!(target.as_deref(), Some("20260513_001_account_user"));
    }

    #[test]
    fn migrate_dsl_parses_from_to_and_dry_run() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "dsl",
            "--from",
            "v0.11",
            "--to",
            "v0.12",
            "--dry-run",
        ])
        .unwrap();
        let Commands::Migrate {
            sub:
                MigrateCommand::Dsl {
                    from,
                    to,
                    dry_run,
                    path,
                },
        } = cli.command
        else {
            panic!("expected migrate dsl command");
        };
        assert_eq!(from, "v0.11");
        assert_eq!(to, "v0.12");
        assert!(dry_run);
        assert!(path.is_none());
    }

    #[test]
    fn migrate_dsl_bootstrap_recipe_rewrites_real_source_end_to_end() {
        // End-to-end: stand up a tempdir project with the bootstrap
        // recipe (mirrored from
        // `migrations/recipes/v0.11-to-v0.12/00-rename-validates-resource.md`)
        // plus a real-shaped .lzi file using the legacy
        // `validates resource @validator.X` form. After
        // `run_migrate_dsl`, the file must (a) reflect the modern
        // form (b) parse cleanly via
        // `lazuli_syntax::parse_feature_skeletons` (c) survive a
        // second migrate pass as a no-op.
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root: PathBuf = std::env::temp_dir().join(format!(
            "lazuli-migrate-dsl-e2e-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        let bootstrap = "---\n\
                         name: rename-validates-resource-keyword\n\
                         applies_to: .lzi\n\
                         match: |\n\
                         \x20\x20${indent:ws}validates resource @validator.${ref}\n\
                         replace: |\n\
                         \x20\x20${indent}validates @validator.${ref}\n\
                         description: Tier-4 cleanup.\n\
                         ---\n";
        fs::write(
            recipe_dir.join("00-rename-validates-resource.md"),
            bootstrap,
        )
        .unwrap();

        let feature_dir = root.join("features/customer");
        fs::create_dir_all(&feature_dir).unwrap();
        let original = "feature customer\n\
                        \x20\x20resource Customer\n\
                        \x20\x20\x20\x20name: Text\n\
                        \x20\x20\x20\x20validates resource @validator.row_check\n";
        let lzi_path = feature_dir.join("customer.lzi");
        fs::write(&lzi_path, original).unwrap();

        let report = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("migrate dsl");
        assert_eq!(report.changed.len(), 1, "report = {report:?}");
        assert!(report.rolled_back.is_empty(), "report = {report:?}");
        let after = fs::read_to_string(&lzi_path).unwrap();
        assert!(after.contains("validates @validator.row_check"));
        assert!(!after.contains("validates resource"));

        // Survives a sanity reparse via the canonical feature-skeleton parser.
        lazuli_syntax::parse_feature_skeletons(&after).expect("reparse rewritten .lzi");

        // Second pass is a no-op: no legacy form left to match.
        let report2 = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("second migrate dsl");
        assert!(report2.changed.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

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

    #[test]
    fn command_sdk_emits_policy_rate_limit_audit_metadata() {
        // Regression for review bug #7 (2026-05-15): the TS SDK
        // previously emitted only `invalidates:` on `defineCommand`,
        // losing the Go-side Policy/RateLimit/Audit. Clients had to
        // call a separate metadata RPC (which didn't exist) to drive
        // policy-aware affordances or rate-limit-aware backoff.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.policies = lazuli_ir::Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@role.admin".to_owned(), "@role.sales".to_owned()],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: vec![],
            span_ref: None,
        };
        feature.commands.push(lazuli_ir::Command {
            name: "update_item".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::Atom("policy.update".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: Some(lazuli_ir::RateLimitSpec::from_default(
                "30 per hour per user".to_owned(),
            )),
            audit: Some(lazuli_ir::AuditSpec {
                subjects: vec![],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }),
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("policy: { name: \"@policy.update\", atoms: ["),
            "policy name must qualify with @policy. prefix; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"admin\" }"),
            "policy atoms must resolve via feature.policies dictionary; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"sales\" }"),
            "all atoms from the matching category must be emitted; got:\n{output}"
        );
        assert!(
            output.contains("rateLimit: \"30 per hour per user\""),
            "rateLimit must surface to the TS SDK; got:\n{output}"
        );
        assert!(
            output.contains("audit: \"default\""),
            "empty-subject AuditSpec must lower to the \"default\" sentinel; got:\n{output}"
        );
    }

    #[test]
    fn command_sdk_omits_metadata_when_absent() {
        // Counterpoint: when the DSL omits a piece of metadata the SDK
        // must omit the property entirely rather than emit it as
        // `undefined` (TS `exactOptionalPropertyTypes` discipline).
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.commands.push(lazuli_ir::Command {
            name: "bare".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            !output.contains("policy:"),
            "expected no policy line; got:\n{output}"
        );
        assert!(
            !output.contains("rateLimit:"),
            "expected no rateLimit line; got:\n{output}"
        );
        assert!(
            !output.contains("audit:"),
            "expected no audit line; got:\n{output}"
        );
        // invalidates is always emitted even when empty — that's the
        // existing contract that this test does not change.
        assert!(output.contains("invalidates: []"));
    }

    #[test]
    fn cap_file_request_upload_emits_command_spec_for_react_hook() {
        // Wave C.2 upload hooks call request_*_upload through
        // useLazuliCommand because minting a signed PUT URL is an
        // imperative upload step, not a cacheable read. The get-url
        // command remains query-shaped so the hook can expose photoUri
        // from TanStack Query state.
        let source = r#"feature host
  defaults
    tenancy org

  uses org
  uses account

  policies
    host_only: @scope.authenticated, @role.host

  domain
    resource Host
      org: Org required
      user: User required unique
      profile_photo: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
"#;
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        let feature = lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers");
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains(
                "export const requestHostProfilePhotoUpload = defineCommand<RequestHostProfilePhotoUploadInput, ProfilePhotoUploadIntent>(\"host.request_profile_photo_upload\", {"
            ),
            "request upload must remain a CommandSpec for useLazuliCommand; got:\n{output}"
        );
        assert!(
            output.contains(
                "export const getHostProfilePhotoURL = defineQuery<GetHostProfilePhotoURLInput, ProfilePhotoDisplayUrl>(\"host.get_profile_photo_url\");"
            ),
            "get-url stays query-shaped for photoUri cache state; got:\n{output}"
        );
    }

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

    fn enum_sdk_fixture(
        include_unused_enum: bool,
        include_second_resource: bool,
    ) -> (lazuli_ir::Feature, lazuli_ir::Module) {
        let mut enums = vec![lazuli_ir::EnumDecl {
            name: "ItemType".to_owned(),
            public_contract: None,
            variants: vec![
                lazuli_ir::EnumVariant {
                    name: "Doc".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
                lazuli_ir::EnumVariant {
                    name: "Decision".to_owned(),
                    storage_value: Some(lazuli_ir::StorageValue::String("decision".to_owned())),
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
            ],
            previous_names: vec![],
            span_ref: None,
        }];
        if include_unused_enum {
            enums.push(lazuli_ir::EnumDecl {
                name: "Unused".to_owned(),
                public_contract: None,
                variants: vec![lazuli_ir::EnumVariant {
                    name: "Legacy".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                }],
                previous_names: vec![],
                span_ref: None,
            });
        }

        let mut resources = vec![resource(
            "Item",
            vec![
                field("type", lazuli_ir::TypeRef::EnumRef(local_qn("ItemType"))),
                field(
                    "tags",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::Builtin(
                        lazuli_ir::BuiltinType::Text,
                    ))),
                ),
                field(
                    "categories",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::EnumRef(local_qn(
                        "ItemType",
                    )))),
                ),
            ],
        )];
        if include_second_resource {
            resources.push(resource(
                "Note",
                vec![field(
                    "type",
                    lazuli_ir::TypeRef::EnumRef(local_qn("ItemType")),
                )],
            ));
        }

        let feature = lazuli_ir::Feature {
            name: "item".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums,
            resources,
            events: vec![],
            rules: vec![],
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        };
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        (feature, module)
    }

    fn resource(name: &str, fields: Vec<lazuli_ir::Field>) -> lazuli_ir::Resource {
        lazuli_ir::Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: vec![],
            lifecycle_routes: None,
        }
    }

    fn field(name: &str, type_ref: lazuli_ir::TypeRef) -> lazuli_ir::Field {
        lazuli_ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    // typed_slot consolidated to a-1's 3-arg form (explicit required).
    // HEAD's 2-arg callers updated to pass `true` for the required flag.
    fn typed_slot(
        name: &str,
        type_ref: lazuli_ir::TypeRef,
        required: bool,
    ) -> lazuli_ir::TypedSlot {
        lazuli_ir::TypedSlot {
            name: name.to_owned(),
            type_ref,
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }
    }

    fn command_with_typed_input(
        name: &str,
        slots: Vec<lazuli_ir::TypedSlot>,
    ) -> lazuli_ir::Command {
        command(
            name,
            lazuli_ir::CommandKind::Update,
            lazuli_ir::CommandInput::Typed(slots),
            lazuli_ir::CommandEffect::None,
        )
    }

    fn command(
        name: &str,
        kind: lazuli_ir::CommandKind,
        input: lazuli_ir::CommandInput,
        effect: lazuli_ir::CommandEffect,
    ) -> lazuli_ir::Command {
        lazuli_ir::Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route: vec![],
            input,
            target: None,
            lets: vec![],
            effect,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        }
    }

    fn list_query(name: &str) -> lazuli_ir::Query {
        lazuli_ir::Query::List(lazuli_ir::ListQuery {
            name: name.to_owned(),
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
        })
    }

    fn pure_read_list_command(name: &str, resource_name: &str) -> lazuli_ir::Command {
        command(
            name,
            lazuli_ir::CommandKind::Returns,
            lazuli_ir::CommandInput::Typed(vec![]),
            lazuli_ir::CommandEffect::Returns(lazuli_ir::ReturnsEffect {
                return_type: lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::UserDefined(
                    local_qn(resource_name),
                ))),
            }),
        )
    }

    #[test]
    fn plugin_semantic_type_emits_ts_alias_and_field_reference() {
        // B3 — `@semantic.BrazilianCPF` lowers to a SemanticPluginType
        // with carrier = Text. The SDK emitter writes
        // `export type BrazilianCPF = string;` at the file head and
        // references it in every consuming interface. See
        // `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
        let mut feature = lazuli_ir::Feature {
            name: "host".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        };
        feature.resources.push(resource(
            "Host",
            vec![field(
                "cpf",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
                    plugin: "@lazuli/plugin-scalars-br".to_owned(),
                    name: "BrazilianCPF".to_owned(),
                    carrier: Box::new(lazuli_ir::BuiltinType::Text),
                    validator: "ValidateCPF".to_owned(),
                    go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
                    ts_package: "@lazuli/plugin-scalars-br".to_owned(),
                    error_code: "cpf_invalid".to_owned(),
                    message_key: String::new(),
                    ts_validator: String::new(),
                }),
            )],
        ));
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        let out = emit_feature_sdk_ts(&feature, &module);
        assert!(
            out.contains("export type BrazilianCPF = string;"),
            "expected brand alias, got:\n{out}"
        );
        assert!(
            out.contains("cpf: BrazilianCPF;"),
            "expected typed field, got:\n{out}"
        );
    }

    fn local_qn(name: &str) -> lazuli_ir::QualifiedName {
        lazuli_ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn occurrences(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }

    #[test]
    fn wave2_cli_dispatch_parses_new_surfaces() {
        let cli = Cli::try_parse_from(["lazuli", "generate", "feature", "billing"]).unwrap();
        let Commands::Generate {
            kind: GenerateKind::Feature,
            input,
            ..
        } = cli.command
        else {
            panic!("expected generate feature command");
        };
        assert_eq!(input, PathBuf::from("billing"));

        let cli =
            Cli::try_parse_from(["lazuli", "new", "demo", "--frontends", "web,mobile"]).unwrap();
        let Commands::New {
            frontends: Some(frontends),
            ..
        } = cli.command
        else {
            panic!("expected new command with frontends");
        };
        assert_eq!(frontends, "web,mobile");

        let cli =
            Cli::try_parse_from(["lazuli", "new", "--frontends", "web", "--in-place"]).unwrap();
        let Commands::New {
            project_name: None,
            frontends: Some(frontends),
            in_place: true,
            ..
        } = cli.command
        else {
            panic!("expected in-place new command without project name");
        };
        assert_eq!(frontends, "web");

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "import",
            "--from",
            "tokens.figma.json",
            "--format",
            "figma",
            "--overwrite",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Import {
                    format: DesignImportFormat::Figma,
                    overwrite: true,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design import command");
        };

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "export",
            "--target",
            "style-dictionary",
            "--out",
            "tokens.sd.json",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Export {
                    target: DesignExportTarget::StyleDictionary,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design export command");
        };

        let cli = Cli::try_parse_from(["lazuli", "design", "diff", "--against", "tokens.sd.json"])
            .unwrap();
        let Commands::Design {
            sub: DesignCommand::Diff { against },
        } = cli.command
        else {
            panic!("expected design diff command");
        };
        assert_eq!(against, PathBuf::from("tokens.sd.json"));
    }

    #[test]
    fn in_place_appends_manifest_block() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[lazuli]"));
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("target = \"tanstack-vite\""));
        assert!(manifest.contains("source = \"app/web\""));
    }

    #[test]
    fn in_place_preserves_existing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/tailwind.config.ts"),
            "// custom tailwind\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("app/web/tailwind.config.ts")).unwrap(),
            "// custom tailwind\n"
        );
    }

    #[test]
    fn in_place_writes_missing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert!(root.join("app/web/index.html").is_file());
        assert!(root.join("app/web/main.tsx").is_file());
        assert!(root.join("app/web/shell/root.tsx").is_file());
        assert!(root.join("app/web/shell/layout.tsx").is_file());
        assert!(root.join("app/web/theme/theme_provider.tsx").is_file());
        assert!(root.join("app/web/theme/globals.css").is_file());
        assert!(root.join("app/web/tailwind.config.ts").is_file());
        assert!(root.join("app/web/tsconfig.json").is_file());
        assert!(root.join("app/web/vite.config.ts").is_file());
    }

    #[test]
    fn in_place_without_manifest_errors() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let err = new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("no Lazurite project in")
                && err
                    .to_string()
                    .contains("run without --in-place to scaffold a new project"),
            "{err:#}"
        );
    }

    #[test]
    fn in_place_merges_package_json() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/package.json"),
            r#"{
  "name": "custom-app",
  "dependencies": {
    "left-pad": "1.3.0",
    "react": "18.0.0"
  },
  "devDependencies": {
    "custom-dev-tool": "0.1.0"
  }
}
"#,
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let package_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("app/web/package.json")).unwrap())
                .unwrap();
        assert_eq!(package_json["name"], "custom-app");
        assert_eq!(package_json["dependencies"]["left-pad"], "1.3.0");
        assert_eq!(package_json["dependencies"]["react"], "18.0.0");
        assert_eq!(package_json["devDependencies"]["custom-dev-tool"], "0.1.0");
        assert!(package_json["dependencies"]["@tanstack/react-query"].is_string());
        assert!(package_json["dependencies"]["@lazuli/runtime"].is_string());
        assert!(package_json["devDependencies"]["vite"].is_string());
    }

    #[test]
    fn pascal_case_converts_project_names() {
        assert_eq!(pascal_case("my-app"), "MyApp");
        assert_eq!(pascal_case("acme_crm"), "AcmeCrm");
        assert_eq!(pascal_case("123-api"), "App123Api");
    }

    #[test]
    fn pascal_case_project_name_handles_kebab_and_snake() {
        assert_eq!(
            pascal_case_project_name(Path::new("my-app")).unwrap(),
            "MyApp"
        );
        assert_eq!(
            pascal_case_project_name(Path::new("acme_crm")).unwrap(),
            "AcmeCrm"
        );
    }

    #[test]
    fn default_module_name_derives_from_project_name() {
        assert_eq!(default_module_name(Path::new("my-app")), "lazuli/my-app");
        assert_eq!(
            default_module_name(Path::new("acme_crm")),
            "lazuli/acme-crm"
        );
        assert_eq!(default_module_name(Path::new("AcmeCRM")), "lazuli/acme-crm");
    }

    #[test]
    fn scaffold_bare_writes_minimal_files() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("lazuli-bare-test-{}-{suffix}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let bare = root.join("bare-app");
        scaffold_bare(&bare, "BareApp").unwrap();
        assert_eq!(
            fs::read_to_string(bare.join("app.lzi")).unwrap(),
            app_template("BareApp")
        );
        assert_eq!(
            fs::read_to_string(bare.join("registry.lzi")).unwrap(),
            REGISTRY_TEMPLATE
        );
        assert!(bare.join("README.md").is_file());
        assert!(bare.join(".gitignore").is_file());
        assert!(bare.join("features").join(".gitkeep").is_file());
        assert!(!bare.join("Lazurite.toml").exists());
        assert!(!bare.join("features").join("account").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_substitutes_placeholders() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-substitute-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "github.com/me/myapp",
        )
        .unwrap();
        assert!(
            fs::read_to_string(root.join("app/app.lzi"))
                .unwrap()
                .contains("app MyApp")
        );
        assert!(
            fs::read_to_string(root.join("go.mod"))
                .unwrap()
                .contains("module github.com/me/myapp")
        );
        assert!(
            fs::read_to_string(root.join("README.md"))
                .unwrap()
                .contains("# MyApp")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_strips_tmpl_extension() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-extension-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        assert!(root.join("app/app.lzi").is_file());
        assert!(!root.join("app/app.lzi.tmpl").exists());
        assert!(root.join("app/features/account/account.lzi").is_file());
        assert!(!root.join("app/features/account/account.lzi.tmpl").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "smoke test for the complete embedded Lazurite scaffold tree"]
    fn scaffold_from_template_smoke_tree_matches_expected() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-smoke-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        // Handler starter `.go` files are not scaffolded — the codegen
        // handler-stub emitter (`crates/lazuli_codegen_go/src/emitter/
        // handlers.rs`) lays them down in `dist/go/<feature>/<name>.go`
        // on first `lazuli generate go`. The scaffold owns `.lzi` /
        // `.lzx` / `.tmpl` (notification templates) / config; user Go
        // handlers materialise via the codegen path.
        for relative in [
            ".gitignore",
            "README.md",
            "app/app.lzi",
            "app/design.lzi",
            "go.mod",
            "go.work",
            "Lazurite.toml",
            "app/registry.lzi",
            "app/features/account/account.lzi",
            "app/features/account/templates/welcome.en-US",
            "app/features/account/templates/welcome.pt-BR",
            "i18n/common.en-US.json",
            "scripts/seed.sh",
            ".env.example",
            "docker-compose.yml",
            "scripts/bootstrap-storage.sh",
        ] {
            assert!(root.join(relative).is_file(), "missing {relative}");
        }

        // The bootstrap-storage script substitutes `{{app_slug}}` as a
        // bash-default fallback; the `.tmpl` suffix is stripped.
        let bootstrap = fs::read_to_string(root.join("scripts/bootstrap-storage.sh"))
            .expect("read bootstrap-storage.sh");
        assert!(
            bootstrap.contains(":-my_app"),
            "bootstrap-storage.sh should embed app_slug as a default: {bootstrap}"
        );
        let env_example = fs::read_to_string(root.join(".env.example")).expect("read .env.example");
        assert!(
            env_example.contains("S3_ENDPOINT="),
            ".env.example should declare S3_ENDPOINT"
        );
        let compose =
            fs::read_to_string(root.join("docker-compose.yml")).expect("read docker-compose.yml");
        assert!(
            compose.contains("MINIO_ROOT_USER_FILE: \"\""),
            "docker-compose.yml should clear MinIO _FILE defaults"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_expand_rewrites_local_sugars() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

      event created
        email: @semantic.Email

  command create
    input name, email
    policy @policy.create
    creates Customer from input

  command rename
    route id: ID
    input name
    policy @policy.update
    updates Customer
      name = input.name

  workflow lifecycle on Customer.status
    policy @policy.update

    activate: lead -> active requires @policy.delete emits customer_activated
"#;

        let expanded = expand_canonical_source(source);

        assert!(expanded.contains("    query.lookup by_id\n      params\n        id: ID"));
        assert!(expanded.contains("    event customer_created\n      customer_id: ID\n      org_id: ID\n      email: @semantic.Email"));
        assert!(
            expanded.contains(
                "    creates Customer\n      name = input.name\n      email = input.email"
            )
        );
        assert!(
            expanded.contains("    target query.by_id(id: route.id)\n    policy @policy.update")
        );
        assert!(expanded.contains(
            "    activate: lead -> active\n      requires @policy.delete\n      emits customer_activated"
        ));
        assert!(!expanded.contains("event_group customer_* on Customer"));
        assert!(!expanded.contains("from input"));
    }

    #[test]
    fn inspect_json_reports_selected_expansions_with_origin() {
        let source = r#"
feature customer
  purpose "Customers"

  requires integration gateway: PaymentGateway

  refs
    core: @role, @policy, @semantic, @cap, @pii, @key

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id

      event created
        email: @semantic.Email @pii.contact

  policies
    update: @role.admin

  command rename
    route id: ID
    input name
    policy @policy.update
    idempotency by route.id, input.name
    retry 2 backoff exponential
    calls gateway.rename_customer
      customer_id = route.id
      name = input.name
    timeout "5s"
    updates Customer
      name = input.name
    emits customer_created
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        expansions.targets = true;
        expansions.policies = true;
        expansions.defaults = true;
        expansions.refs = true;
        expansions.summary = true;
        expansions.locators = true;
        expansions.dependencies = true;
        expansions.security = true;
        expansions.tests = true;

        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema\":\"lazuli.inspect.v0\""));
        assert!(json.contains("\"requirements\""));
        assert!(json.contains("\"kind\":\"integration\""));
        assert!(json.contains("\"name\":\"gateway\""));
        assert!(json.contains("\"contract\":\"PaymentGateway\""));
        assert!(json.contains("\"external_calls\""));
        assert!(json.contains("\"subject\":\"customer.command.rename\""));
        assert!(json.contains("\"slot\":\"gateway\""));
        assert!(json.contains("\"operation\":\"rename_customer\""));
        assert!(json.contains("\"timeout\":\"5s\""));
        assert!(json.contains("\"retry\":\"2 backoff exponential\""));
        assert!(json.contains("\"idempotency\":\"route.id, input.name\""));
        assert!(json.contains("\"origin\":\"event_group:customer_*\""));
        assert!(json.contains("\"refs\""));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"resources\":[\"Customer\"]"));
        assert!(json.contains("\"records\":[\"CustomerLtv\"]"));
        assert!(json.contains("\"provides\""));
        assert!(json.contains("\"types\":[\"Customer\",\"CustomerLtv\"]"));
        assert!(!json.contains("\"missing\""));
        assert!(
            json.contains("\"origin\":\"inferred from local route id and query.lookup by_id\"")
        );
        assert!(json.contains("\"origin\":\"explicit\""));
        assert!(json.contains("\"origin\":\"defaults\""));
        assert!(json.contains("\"name\":\"query_order\""));
        assert!(json.contains("\"name\":\"query_filter_index\""));
        assert!(json.contains("\"value\":\"org, name\""));
        assert!(json.contains("\"origin\":\"language default\""));
        assert!(json.contains("\"locators\""));
        assert!(json.contains("\"name\":\"route.id\""));
        assert!(json.contains("\"name\":\"target\""));
        assert!(json.contains("\"dependencies\""));
        assert!(json.contains("\"kind\":\"emits_event\""));
        assert!(json.contains("\"security\""));
        assert!(json.contains("\"markers\":[\"@pii.contact\""));
        assert!(json.contains("@cap.Encrypted(key:@key.tenant)"));
        assert!(json.contains("\"tests\""));
        assert!(json.contains("\"assertion\":\"permits @role.admin\""));
        assert!(json.contains("\"origin\":\"generated from command policy @policy.update\""));
    }

    #[test]
    fn inspect_json_reports_app_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"

  uses
    customer

  packs
    customer_import from registry.packs.customer_import

  bindings
    customer.gateway = integrations.crm

  targets
    backend go
    web react

  environments
    local
    production

  urls
    api production "https://api.acme.example"

  env
    server DATABASE_URL: Secret required
    group mailer
      server MAILER_API_KEY: Secret required in production

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET

  capabilities
    database postgres

  architecture
    mode modular_monolith
    service_ready true

  services
    service crm
      owns customer
      exposes
        query customer.query.list

  communication
    internal sync rpc
    propagate actor, tenant

  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"

  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#;

        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"app\""));
        assert!(json.contains("\"name\":\"AcmeCRM\""));
        assert!(json.contains("\"packs\""));
        assert!(json.contains("\"registry.packs.customer_import\""));
        assert!(json.contains("\"bindings\""));
        assert!(json.contains("\"target_feature\":\"customer\""));
        assert!(json.contains("\"source\":\"integrations.crm\""));
        assert!(json.contains("\"environments\":[\"local\",\"production\"]"));
        assert!(json.contains("\"url\":\"https://api.acme.example\""));
        assert!(json.contains("\"DATABASE_URL\""));
        assert!(json.contains("\"group\":\"mailer\""));
        assert!(json.contains("\"MAILER_API_KEY\""));
        assert!(json.contains("\"environments\":[\"production\"]"));
        assert!(json.contains("\"integrations\""));
        assert!(json.contains("\"kind\":\"CRMProvider\""));
        assert!(json.contains("\"adapter_provenance\":\"local\""));
        assert!(json.contains("\"webhook_secret\""));
        assert!(json.contains("\"architecture\""));
        assert!(json.contains("\"mode\":\"modular_monolith\""));
        assert!(json.contains("\"services\""));
        assert!(json.contains("\"communication\""));
        assert!(json.contains("\"runtime\""));
        assert!(json.contains("\"migrations\":\"before_deploy\""));
    }

    #[test]
    fn inspect_expand_caches_projects_feature_level_profiles() {
        // CL.C.3 — `--expand=caches` surfaces every feature-level
        // `cache <name>` profile typed end-to-end (key + ttl literal +
        // optional namespace/tags/SWR/coalesce/sliding). The query's
        // inline `cache` slot keeps its own projection.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m
    namespace catalog
    tags product, listing
    stale_while_revalidate 30s
    coalesce true
    sliding true

  domain
    resource Product
      id: ID required

    query.list list
      cache product_view
"#;
        let mut expansions = ExpandSet::default();
        expansions.caches = true;
        let report = inspect_canonical_source(source, Path::new("catalog.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // Expand label surfaces in the report header.
        assert!(
            json.contains("\"expand\":[\"caches\"]"),
            "expected expand label, got {json}"
        );
        // Profile shows up in the `caches` projection.
        assert!(
            json.contains("\"caches\":["),
            "expected caches array, got {json}"
        );
        assert!(
            json.contains("\"name\":\"product_view\""),
            "expected profile name, got {json}"
        );
        assert!(
            json.contains("\"namespace\":\"catalog\""),
            "expected namespace, got {json}"
        );
        assert!(json.contains("\"product\""), "expected tags, got {json}");
        assert!(json.contains("\"listing\""), "expected tags, got {json}");
        assert!(
            json.contains("\"coalesce\":true"),
            "expected coalesce, got {json}"
        );
        assert!(
            json.contains("\"sliding\":true"),
            "expected sliding, got {json}"
        );
    }

    #[test]
    fn inspect_emits_manifest_when_present() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-inspect-manifest-test-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        let app_path = root.join("app.lzi");
        fs::write(
            &app_path,
            r#"
app Marketplace
  title "Marketplace"
"#,
        )
        .unwrap();
        fs::write(
            root.join("Lazurite.toml"),
            r#"
[project]
name = "marketplace"
module = "github.com/acme/marketplace"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-example/payment-gateway" = { module = "github.com/lazuli-lang/lazuli-plugin-example-payment", version = "v0.2.0" }

[generate.go]
out = "dist/go"
submodule = true
emit_main = true

[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["buyer", "seller"]

[migrations]
generated = "dist/go/migrations"
manual = "migrations"
strategy = "auto"
"#,
        )
        .unwrap();

        let source = fs::read_to_string(&app_path).unwrap();
        let json =
            inspect_json_value(&source, &app_path, &root, ExpandSet::default(), &[]).unwrap();

        assert_eq!(json["manifest"]["origin"], "Lazurite.toml");
        assert_eq!(json["manifest"]["project"]["name"], "marketplace");
        assert_eq!(
            json["manifest"]["plugins"][0]["ref"],
            "@lazuli/plugin-example/payment-gateway"
        );
        assert_eq!(json["manifest"]["plugins"][0]["source"], "remote");
        assert_eq!(json["manifest"]["frontends"][0]["name"], "mobile");
        assert_eq!(json["manifest"]["frontends"][0]["target"], "expo");
        assert_eq!(json["manifest"]["migrations"]["strategy"], "auto");
        assert!(!json["ir"].is_null());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_json_reports_profiles() {
        let source = r#"
profile local
  urls
    web "http://localhost:3000"
  bindings
    customer_import.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
"#;

        let report =
            inspect_canonical_source(source, Path::new("profiles.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"profiles\""));
        assert!(json.contains("\"name\":\"local\""));
        assert!(json.contains("\"target\":\"web\""));
        assert!(json.contains("\"environment\":\"sandbox\""));
        assert!(json.contains("\"adapter\":\"@adapter.fake_crm\""));
        assert!(json.contains("\"adapter_provenance\":\"local\""));
        assert!(json.contains("\"topology\":\"monolith\""));
    }

    #[test]
    fn inspect_json_reports_registry_manifest() {
        let source = r#"
registry
  env
    group mercadopago
      server MERCADOPAGO_ACCESS_TOKEN: Secret required in production
  capabilities
    payment_gateway mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
      credentials platform
        access_token env.MERCADOPAGO_ACCESS_TOKEN
"#;

        let report =
            inspect_canonical_source(source, Path::new("registry.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"registry\""));
        assert!(json.contains("\"group\":\"mercadopago\""));
        assert!(json.contains("\"packs\""));
        assert!(json.contains("\"@runtime/payments\""));
        assert!(json.contains("\"provides\""));
        assert!(json.contains("\"contract\":\"PaymentGateway\""));
        assert!(json.contains("\"kind\":\"PaymentGateway\""));
        assert!(json.contains("\"adapter_provenance\":\"runtime\""));
        assert!(json.contains("\"access_token\""));
    }

    #[test]
    fn inspect_expand_webhook_events_projects_registry_events() {
        let source = r#"
registry
  webhook_event customer.created
    payload
      customer_id: ID
      email: @semantic.Email
    version 2
    previous_version 1
"#;

        let report = inspect_canonical_source(
            source,
            Path::new("registry.lzi"),
            parse_expand_set("webhook_events").unwrap(),
        );
        let json = serde_json::to_value(&report).unwrap();
        let event = &json["webhook_events"][0];

        assert_eq!(json["expand"][0], "webhook_events");
        assert_eq!(event["name"], "customer.created");
        assert_eq!(event["version"], 2);
        assert_eq!(event["previous_version"], 1);
        assert_eq!(event["payload"][1]["type_text"], "@semantic.Email");
    }

    #[test]
    fn inspect_expand_flags_are_explicit() {
        let expansions = parse_expand_set("events,targets,locators,dependencies,security").unwrap();

        assert!(expansions.events);
        assert!(expansions.targets);
        assert!(expansions.locators);
        assert!(expansions.dependencies);
        assert!(expansions.security);
        assert!(!expansions.tests);
        assert!(parse_expand_set("crud").is_err());
    }

    // CL.C.4 — `--expand=aggregates` projection test (spec wave-c-cl4).
    #[test]
    fn inspect_expand_aggregates_projects_root_contains_invariants() {
        let expansions = parse_expand_set("aggregates").unwrap();
        assert!(expansions.aggregates);

        let source = "
feature billing
  resource Order
    total: Integer required

  resource OrderLine
    amount: Integer required

  aggregate OrderBoundary
    root Order
    contains OrderLine
    invariants
      invariant total_non_negative
        when total >= 0
        message \"order total cannot be negative\"
";
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"aggregates\":["),
            "expected aggregates projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"OrderBoundary\""),
            "aggregate name should surface: {json}"
        );
        assert!(
            json.contains("\"root\":\"Order\""),
            "root should surface verbatim: {json}"
        );
        assert!(
            json.contains("\"contains\":[\"OrderLine\"]"),
            "contains list should surface: {json}"
        );
        assert!(
            json.contains("\"name\":\"total_non_negative\""),
            "invariant name should surface: {json}"
        );
        assert!(
            json.contains("\"when\":\"total >= 0\""),
            "predicate text should round-trip: {json}"
        );
        assert!(
            json.contains("\"when_kind\":\"closed\""),
            "closed predicate kind should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4b/4cd — per-axis inspect projections (synth Wave 1 cell 01).
    // Mirrors the aggregates/caches template. Each test exercises one
    // `--expand=<axis>` flag in isolation and asserts the lifted IR slice
    // surfaces verbatim in the JSON projection.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_commands_projects_lifted_commands_with_rate_limit_and_audit() {
        let expansions = parse_expand_set("commands").unwrap();
        assert!(expansions.commands);

        let source = r#"
feature billing
  domain
    event_group audit_stream on Order

  resource Order
    total: Integer required

  command pay
    route id: ID
    input
      amount: Integer required
    policy @policy.create
    rate_limit "30 per hour per ip"
    audit actor, target.id, input.amount
      emit_to audit_stream
    creates Order
      total = input.amount
    emits order_paid
    invalidates
      query.list
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"commands\":["),
            "expected commands projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"pay\""),
            "command name should surface: {json}"
        );
        assert!(
            json.contains("\"rate_limit\":{\"default\":\"30 per hour per ip\""),
            "rate_limit verbatim: {json}"
        );
        assert!(
            json.contains("\"audit\""),
            "audit spec should surface: {json}"
        );
        assert!(
            json.contains("\"emit_to\":\"audit_stream\""),
            "audit emit_to should surface: {json}"
        );
        assert!(
            json.contains("\"invalidates\""),
            "invalidates list should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_apis_alias_accepts_api_and_apis_tokens() {
        // Both tokens must populate the same boolean.
        let expansions_plural = parse_expand_set("apis").unwrap();
        assert!(expansions_plural.apis);
        let expansions_singular = parse_expand_set("api").unwrap();
        assert!(expansions_singular.apis);

        let source = r#"
feature billing
  api export
    method GET
    path "/api/billing/export"
    output Text
    policy @scope.public
    handler "./api/export.go"
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions_plural,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"apis\":["),
            "expected apis projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"export\""),
            "api name should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"/api/billing/export\""),
            "api path should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"./api/export.go\""),
            "api handler path should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_resources_projects_lifted_resources() {
        let expansions = parse_expand_set("resources").unwrap();
        assert!(expansions.resources);

        let source = r#"
feature billing
  resource Order
    customer_id: ID required
    total: Integer required
    is_high_value: Boolean derived from total > 1000
    retention 7y then anonymize
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"resources\":["),
            "expected resources projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"Order\""),
            "resource name should surface: {json}"
        );
        assert!(
            json.contains("\"retention\""),
            "retention slot should surface: {json}"
        );
        assert!(
            json.contains("\"derived_from\""),
            "derived_from slot should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_queries_projects_lifted_queries() {
        let expansions = parse_expand_set("queries").unwrap();
        assert!(expansions.queries);

        let source = r#"
feature billing
  domain
    query.lookup by_id by id: ID

    query.list list
      params
        status: Text optional
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"queries\":["),
            "expected queries projection in JSON: {json}"
        );
        assert!(
            json.contains("\"by_id\""),
            "lookup query name should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_records_projects_lifted_records() {
        let expansions = parse_expand_set("records").unwrap();
        assert!(expansions.records);

        let source = r#"
feature billing
  enum InvoiceStatus
    draft
    issued
    paid

  record InvoiceSummary
    status: InvoiceStatus required discriminator
    amount: Integer required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"records\":["),
            "expected records projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"InvoiceSummary\""),
            "record name should surface: {json}"
        );
        assert!(
            json.contains("\"discriminator_field\":\"status\""),
            "discriminator_field should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — inspect projections (§7.3 snapshot tests)
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_tools_flag_parses() {
        let expansions = parse_expand_set("tools").unwrap();
        assert!(expansions.tools);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_expand_tenant_migrations_alias_projects_ir() {
        let source = r#"
feature customer
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let expansions = parse_expand_set("tenant_migrations").unwrap();
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let projected = report.features[0]
            .tenant_migrations
            .as_ref()
            .expect("tenant migrations projection");
        assert_eq!(projected[0].name, "backfill_lifecycle_stage");
        assert_eq!(projected[0].target.axis, "org");
        assert!(projected[0].target.operation.is_some());
    }

    #[test]
    fn inspect_summary_includes_agent_tools_evals_output_kind() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: ID required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 1
    prompt "./p.md"
    tools
      customer.query.lookup.by_id
      @tool.web_search
    evals
      case mentions_status
        requires output contains "active"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Agents are emitted regardless of expansion (always-on field).
        assert!(json.contains("\"name\":\"summarize\""));
        // tools[] now picks up indent-6 entries (canonical block form).
        assert!(
            json.contains("\"tools\":[\"customer.query.lookup.by_id\",\"@tool.web_search\"]"),
            "expected tools list in agent: {json}"
        );
        // evals[] carries the case names.
        assert!(
            json.contains("\"evals\":[\"mentions_status\"]"),
            "expected evals list in agent: {json}"
        );
        // output_kind + output_discriminator surface the discriminator
        // form.
        assert!(
            json.contains("\"output_kind\":\"discriminated_enum\""),
            "expected output_kind discriminated_enum: {json}"
        );
        assert!(
            json.contains("\"output_discriminator\":\"Intent\""),
            "expected output_discriminator Intent: {json}"
        );
        // eval_determinism is `pinned` because temperature 0 + seed 1.
        assert!(
            json.contains("\"eval_determinism\":\"pinned\""),
            "expected eval_determinism pinned: {json}"
        );
    }

    #[test]
    fn inspect_summary_marks_nondeterministic_eval_block() {
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"eval_determinism\":\"nondeterministic\""),
            "expected eval_determinism nondeterministic: {json}"
        );
        assert!(
            json.contains("\"output_kind\":\"stream\""),
            "expected output_kind stream: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_emits_per_agent_dispatch_graph() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      query.lookup.by_id
      customer.command.archive
      @tool.web_search
"#;
        let mut expansions = ExpandSet::default();
        expansions.tools = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // The new --expand=tools projection populates `features[].tools`.
        assert!(
            json.contains("\"agent\":\"triage\""),
            "expected agent entry: {json}"
        );
        // Local query.lookup categorised correctly.
        assert!(
            json.contains("\"reference\":\"query.lookup.by_id\",\"kind\":\"query.lookup\",\"scope\":\"local\",\"derived_effect\":\"read\""),
            "expected local query.lookup binding: {json}"
        );
        // Cross-feature command writes.
        assert!(
            json.contains("\"reference\":\"customer.command.archive\",\"kind\":\"command\",\"scope\":\"cross_feature\",\"derived_effect\":\"write\""),
            "expected cross-feature command binding: {json}"
        );
        // Adapter tool with unknown effect (registry resolves in doctor).
        assert!(
            json.contains("\"reference\":\"@tool.web_search\",\"kind\":\"adapter\",\"scope\":\"adapter\",\"derived_effect\":\"unknown\""),
            "expected adapter binding: {json}"
        );
    }

    #[test]
    fn inspect_expand_events_includes_built_in_trace_events() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"built_in_trace_events\":[{\"name\":\"agent_run\""),
            "expected built_in_trace_events with agent_run: {json}"
        );
        assert!(
            json.contains("\"fires_per\":\"agent_dispatch\""),
            "expected fires_per agent_dispatch: {json}"
        );
        assert!(
            json.contains("\"name\":\"tokens_total\",\"type\":\"Integer\""),
            "expected canonical payload field tokens_total: {json}"
        );
    }

    #[test]
    fn inspect_built_in_trace_events_omitted_without_events_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("built_in_trace_events"),
            "built_in_trace_events must be omitted without --expand=events: {json}"
        );
    }

    #[test]
    fn inspect_expand_expose_flag_parses() {
        let expansions = parse_expand_set("expose").unwrap();
        assert!(expansions.expose);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_summary_includes_agent_expose_http() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"expose_http\":{\"method\":\"POST\""),
            "expected expose_http always-on summary: {json}"
        );
        assert!(json.contains("\"path\":\"/api/customers/:id/summary\""));
    }

    #[test]
    fn inspect_expose_projection_emits_unified_route_table() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID

  api list_customers
    method GET
    path "/api/customers"
    handler "./api/list.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.expose = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(
            json.contains("\"kind\":\"agent\",\"origin\":\"customer.agent.summarize\""),
            "expected agent expose entry: {json}"
        );
        assert!(
            json.contains("\"kind\":\"api\",\"origin\":\"customer.api.list_customers\""),
            "expected api expose entry: {json}"
        );
    }

    #[test]
    fn inspect_expose_projection_omitted_without_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"origin\":\"customer.agent.summarize\""),
            "expose projection must be omitted without --expand=expose: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_omitted_without_expand() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      query.lookup.by_id
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Without --expand=tools the new projection is omitted (skipped
        // by `Option::is_none`). The agent's plain tools list is still
        // emitted as part of the always-on agents block.
        assert!(
            !json.contains("\"reference\":\"query.lookup.by_id\""),
            "tools projection should not appear without --expand=tools: {json}"
        );
        assert!(
            json.contains("\"tools\":[\"query.lookup.by_id\"]"),
            "agent.tools list should still be present: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L — `--expand=auth` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_auth_projection_emits_full_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let auth = &json["features"][0]["auth"];
        assert!(!auth.is_null(), "auth projection should be present: {json}");
        assert_eq!(auth["origin"]["feature"], "customer_auth");
        assert_eq!(auth["identity"]["field"], "Customer.email");
        assert_eq!(auth["identity"]["resource"], "Customer");
        assert_eq!(auth["identity"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert_eq!(auth["password"]["hash"], "@fn.hash_customer_password");
        assert_eq!(auth["password"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["mfa"]["method"], "totp");
        assert_eq!(auth["mfa"]["enroll"], "@fn.enroll_customer_totp");
        assert_eq!(auth["mfa"]["verify"], "@validator.verify_customer_totp");
        assert_eq!(auth["mfa"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["sessions"]["ttl"], "7 days");
        assert_eq!(auth["sessions"]["refresh"], false);
        assert_eq!(auth["sessions"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["oauth"][0]["provider"], "google");
        assert_eq!(auth["oauth"][0]["origin"]["feature"], "customer_auth");
    }

    #[test]
    fn inspect_auth_projection_omitted_without_expand() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer_auth.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"auth\":{"),
            "auth projection must be absent without --expand=auth: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `--expand=storage` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_storage_projection_emits_resource_field_capability() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
      uploaded_by: User required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer_import.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let storage = &json["features"][0]["storage"];
        assert!(
            !storage.is_null(),
            "storage projection should be present: {json}"
        );
        let field = &storage["fields"][0];
        assert_eq!(field["resource"], "CustomerImportBatch");
        assert_eq!(field["field"], "file");
        assert_eq!(
            field["file_capability"]["max_size"]["bytes"],
            25 * 1024 * 1024
        );
        assert_eq!(field["file_capability"]["max_size"]["literal"], "25mb");
        assert_eq!(field["file_capability"]["accept"][0]["family"], "text");
        assert_eq!(field["file_capability"]["accept"][0]["subtype"], "csv");
    }

    #[test]
    fn inspect_storage_projection_emits_api_output_capability() {
        let source = r#"
feature customer
  api customer_export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/export.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let output = &json["features"][0]["storage"]["api_outputs"][0];
        assert_eq!(output["api"], "customer_export");
        assert_eq!(output["file_capability"]["max_size"]["literal"], "100mb");
        assert_eq!(output["file_capability"]["visibility"], "signed");
        assert_eq!(output["file_capability"]["signed_ttl"], "1h");
    }

    #[test]
    fn inspect_storage_projection_omitted_without_expand() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("customer_import.lzi"),
            ExpandSet::default(),
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"storage\":{"),
            "storage projection must be absent without --expand=storage: {json}"
        );
    }

    #[test]
    fn inspect_storage_projection_absent_when_feature_has_no_cap_file() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No @cap.File authored → field omitted entirely.
        assert!(json["features"][0]["storage"].is_null());
    }

    #[test]
    fn inspect_auth_projection_absent_when_feature_has_no_auth() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No auth block authored → field omitted (None serialises away).
        assert!(json["features"][0]["auth"].is_null());
    }

    // -------------------------------------------------------------------------
    // Roadmap §1.2 — `--expand=http` projection coverage. The unified
    // `http` slot at the report root surfaces cookie + proxy + limits
    // with `origin` metadata only when the flag is set. The typed
    // blocks still serialize on `app` either way.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_http_flag_parses() {
        let expansions = parse_expand_set("http").unwrap();
        assert!(expansions.http);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_http_projection_surfaces_cookie_proxy_limits_with_flag() {
        let source = r#"
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax

  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto

  limits
    body_size "10mb"
    header_size "16kb"
    timeout "30s"
"#;
        let mut expansions = ExpandSet::default();
        expansions.http = true;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let http = &json["http"];
        assert!(!http.is_null(), "http projection should be present: {json}");
        assert_eq!(http["origin"]["app"], "MyApp");
        // Cookie block.
        assert_eq!(http["cookie"]["profiles"][0]["name"], "default");
        assert_eq!(http["cookie"]["profiles"][0]["signed"], true);
        assert_eq!(http["cookie"]["profiles"][0]["same_site"], "strict");
        assert_eq!(http["cookie"]["profiles"][0]["max_age"], "7d");
        assert_eq!(http["cookie"]["profiles"][1]["name"], "session");
        assert_eq!(http["cookie"]["profiles"][1]["same_site"], "lax");
        // Proxy block.
        assert_eq!(http["proxy"]["trusted"][0], "10.0.0.0/8");
        assert_eq!(http["proxy"]["trusted"][1], "172.16.0.0/12");
        assert_eq!(http["proxy"]["real_ip_header"], "X-Forwarded-For");
        assert_eq!(http["proxy"]["forwarded_proto_header"], "X-Forwarded-Proto");
        // Limits block.
        assert_eq!(http["limits"]["body_size"], "10mb");
        assert_eq!(http["limits"]["header_size"], "16kb");
        assert_eq!(http["limits"]["timeout"], "30s");
        // Per-block origin envelope.
        assert_eq!(http["cookie"]["origin"]["app"], "MyApp");
        assert_eq!(http["proxy"]["origin"]["app"], "MyApp");
        assert_eq!(http["limits"]["origin"]["app"], "MyApp");
    }

    #[test]
    fn inspect_http_projection_omitted_without_expand() {
        let source = r#"
app MyApp
  cookie
    default
      same_site strict

  limits
    body_size "10mb"
"#;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        // The unified `http` slot at the report root is absent without
        // the flag — Option<Value>::None skips the serde key.
        assert!(
            !json.contains("\"http\":{"),
            "http projection must be absent without --expand=http: {json}"
        );
        // But the typed blocks still serialize on `app`.
        assert!(
            json.contains("\"cookie\":"),
            "cookie still surfaces on AppManifest: {json}"
        );
        assert!(
            json.contains("\"limits\":"),
            "limits still surfaces on AppManifest: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // `--format=lazuli` for `lazuli inspect <symbol>` (next-checklist
    // follow-up from lsp-symbol-origin v0.2; closes the deferred item).
    // -------------------------------------------------------------------------

    #[test]
    fn render_inspect_symbol_lazuli_found_emits_human_readable_one_liner() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "features/account/account.lzi",
                "line": 42,
                "column": 3,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": [],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("Customer"),
            "rendered should name the symbol:\n{rendered}"
        );
        assert!(
            rendered.contains("account"),
            "rendered should name the feature:\n{rendered}"
        );
        assert!(
            rendered.contains("features/account/account.lzi:42"),
            "rendered should anchor the source location:\n{rendered}"
        );
        assert!(
            rendered.contains("(resource)"),
            "rendered should name the symbol kind:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_with_previous_names() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "x.lzi",
                "line": 10,
                "column": 1,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": ["Client", "User"],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("previously:"),
            "rendered should announce previously: trailer:\n{rendered}"
        );
        assert!(
            rendered.contains("Client") && rendered.contains("User"),
            "rendered should list both previous names:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_not_found_emits_code_and_message() {
        let output = serde_json::json!({
            "error": {
                "code": "SYMBOL_NOT_FOUND",
                "message": "no declaration named `Foo` in any feature of this project",
            }
        });
        let rendered = render_inspect_symbol_lazuli("Foo", &output);
        assert!(
            rendered.starts_with("SYMBOL_NOT_FOUND:"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("Foo"),
            "rendered should echo the missing symbol:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_ambiguous_lists_candidates() {
        let output = serde_json::json!({
            "error": {
                "code": "AMBIGUOUS_SYMBOL",
                "message": "`Customer` is declared in multiple features",
                "candidates": ["account.Customer", "billing.Customer"],
            }
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("AMBIGUOUS_SYMBOL"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("- account.Customer"),
            "rendered should list candidate as bullet:\n{rendered}"
        );
        assert!(
            rendered.contains("- billing.Customer"),
            "rendered should list every candidate:\n{rendered}"
        );
    }
}
