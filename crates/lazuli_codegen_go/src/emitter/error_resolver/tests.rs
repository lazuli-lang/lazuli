//! Tests for error_resolver emission. Split out of `mod.rs` to
//! keep production under the 500 LOC budget.

    use super::*;
    use lazuli_ir::{
        CommandEffect, CommandInput, CommandKind, Defaults, Feature, FeatureErrorMessage,
        FeatureErrors, Module, Policies, PolicyCategory, PolicyRef, TranslationKeyRef,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn empty_command(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Create,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: Vec::new(),
            span_ref: None,
            derived_from: None,
        }
    }

    fn module_with(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    #[test]
    fn command_without_override_emits_nothing() {
        let command = empty_command("create");
        assert!(!command_has_error_keys(&command, None));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", None);
        assert_eq!(p.finish(), String::new());
    }

    #[test]
    fn command_with_override_emits_error_keys_var() {
        let mut command = empty_command("choose_role");
        command.policy_when_denied = Some(TranslationKeyRef {
            key: "choose_role_signin_required".to_owned(),
            span_ref: None,
        });
        assert!(command_has_error_keys(&command, None));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", None);
        let out = p.finish();
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        // `command_var_name` interleaves the effect-pinned resource pascal
        // (`Result` when `CommandEffect::None`) between the verb and the
        // modifier words — `choose_role` + `Result` → `chooseResultRole`.
        // Reusing that helper keeps the ErrorKeys var name in lock-step
        // with the `lazuli.Command[I, O]` var name in command.gen.go
        // (single source of truth, no drift).
        let expected_var = command_error_keys_var(&command);
        assert_eq!(expected_var, "chooseResultRoleErrorKeys");
        assert!(out.contains(&format!("var {expected_var} = lazuli.ErrorKeys{{")));
        assert!(out.contains(
            "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"choose_role_signin_required\"},"
        ));
    }

    #[test]
    fn command_with_policy_category_when_denied_emits_error_keys_var() {
        let mut command = empty_command("account_me");
        command.policy = PolicyRef::Local("authenticated".to_owned());
        let policies = Policies {
            categories: vec![PolicyCategory {
                name: "authenticated".to_owned(),
                atoms: vec!["@scope.authenticated".to_owned()],
                conditional_atoms: Vec::new(),
                previous_names: Vec::new(),
                when_denied: Some(TranslationKeyRef {
                    key: "account_signin".to_owned(),
                    span_ref: None,
                }),
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        assert!(command_has_error_keys(&command, Some(&policies)));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", Some(&policies));
        let out = p.finish();
        assert!(out.contains(
            "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"account_signin\"},"
        ));
    }

    #[test]
    fn feature_without_errors_block_skips_file() {
        let feature = empty_feature("account");
        assert!(emit_feature_errors_file("lazuli/test-app", &feature).is_none());
    }

    #[test]
    fn feature_with_errors_block_emits_contract_value() {
        let mut feature = empty_feature("account");
        feature.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
            exposure_5xx: vec!["code".to_owned()],
            messages: vec![
                FeatureErrorMessage {
                    code: "policy_denied".to_owned(),
                    message: TranslationKeyRef {
                        key: "account_signin_required".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
                FeatureErrorMessage {
                    code: "tenant_mismatch".to_owned(),
                    message: TranslationKeyRef {
                        key: "account_wrong_workspace".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
            ],
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let out = emit_feature_errors_file("lazuli/test-app", &feature)
            .expect("feature with errors block must emit");

        assert!(out.contains("\npackage accountgen\n"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        assert!(out.contains("var FeatureErrors = lazuli.FeatureErrorContract{"));
        assert!(out.contains("Default:         lazuli.ExposureHide,"));
        assert!(out.contains("ExposeClient4xx: []string{\"message\", \"code\"},"));
        assert!(out.contains("ExposeClient5xx: []string{\"code\"},"));
        // BTreeMap sorts codes: `policy_denied` < `tenant_mismatch`.
        let policy = out
            .find("\"policy_denied\":")
            .expect("policy_denied present");
        let tenant = out
            .find("\"tenant_mismatch\":")
            .expect("tenant_mismatch present");
        assert!(policy < tenant, "messages must be sorted by code:\n{out}");
        assert!(out.contains(
            "\"policy_denied\": i18n.MessageRef{Feature: \"account\", Key: \"account_signin_required\"},"
        ));
        assert!(out.contains(
            "\"tenant_mismatch\": i18n.MessageRef{Feature: \"account\", Key: \"account_wrong_workspace\"},"
        ));
    }

    #[test]
    fn app_resolver_skipped_when_no_feature_has_errors() {
        let module = module_with(vec![empty_feature("account")]);
        assert!(emit_app_error_resolution("lazuli/test-app", &module, "lazuli/test-app").is_none());
    }

    #[test]
    fn app_resolver_registers_each_feature_with_errors() {
        let mut account = empty_feature("account");
        account.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: Vec::new(),
            exposure_5xx: Vec::new(),
            messages: Vec::new(),
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let mut billing = empty_feature("billing");
        billing.errors = Some(FeatureErrors::default());
        // Insert in reverse alphabetical order so we can prove sorting.
        let module = module_with(vec![billing, account]);
        let out = emit_app_error_resolution("lazuli/test-app", &module, "lazuli/test-app")
            .expect("must emit when at least one feature has errors");

        assert!(out.contains("\npackage app\n"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli/test-app/account\""));
        assert!(out.contains("\"lazuli/test-app/billing\""));
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        assert!(out.contains("func init() {"));
        assert!(
            out.contains("lazuli.RegisterFeatureErrors(\"account\", accountgen.FeatureErrors)")
        );
        assert!(
            out.contains("lazuli.RegisterFeatureErrors(\"billing\", billinggen.FeatureErrors)")
        );
        // Alphabetical: account before billing.
        let account_idx = out
            .find("\"account\", accountgen")
            .expect("account register present");
        let billing_idx = out
            .find("\"billing\", billinggen")
            .expect("billing register present");
        assert!(
            account_idx < billing_idx,
            "features must register in sorted order:\n{out}"
        );
    }

    #[test]
    fn emit_error_vocab_files_collects_features_and_app_registry() {
        let mut account = empty_feature("account");
        account.errors = Some(FeatureErrors::default());
        let module = module_with(vec![account]);
        let files = emit_error_vocab_files("lazuli/test-app", &module, "lazuli/test-app");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "account/errors.gen.go");
        assert_eq!(files[1].path, "app/error_resolution.gen.go");
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = empty_feature("account");
        feature.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
            exposure_5xx: Vec::new(),
            messages: vec![
                FeatureErrorMessage {
                    code: "tenant_mismatch".to_owned(),
                    message: TranslationKeyRef {
                        key: "k1".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
                FeatureErrorMessage {
                    code: "policy_denied".to_owned(),
                    message: TranslationKeyRef {
                        key: "k2".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
            ],
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let a = emit_feature_errors_file("lazuli/test-app", &feature).unwrap();
        let b = emit_feature_errors_file("lazuli/test-app", &feature).unwrap();
        assert_eq!(a, b);
    }
