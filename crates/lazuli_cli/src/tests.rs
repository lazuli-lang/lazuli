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

    use crate::cli_args::{DesignExportTarget, DesignImportFormat};
use crate::go_work_io::add_missing_go_work_use_entries;
use crate::{
        Cli, Commands, DesignCommand, ExpandSet,
        GenerateKind, MigrateCommand, REGISTRY_TEMPLATE,
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

    mod migrate {
        include!("tests/migrate.rs");
    }

    mod test_support {
        include!("tests/test_support.rs");
    }
    use test_support::*;

    mod codegen_ts_enums {
        include!("tests/codegen_ts_enums.rs");
    }

    mod codegen_ts_command_sdk {
        include!("tests/codegen_ts_command_sdk.rs");
    }

    mod codegen_ts_react_hooks {
        include!("tests/codegen_ts_react_hooks.rs");
    }

    mod codegen_ts_query_sdk {
        include!("tests/codegen_ts_query_sdk.rs");
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
