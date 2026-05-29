    // Codegen-TS plugin-semantic type alias test — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use super::test_support::*;
    use crate::emit_feature_sdk_ts;

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
            knowledge: None,
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
