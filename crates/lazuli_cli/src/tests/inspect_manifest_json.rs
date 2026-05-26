    // Inspect-CLI manifest/profile/registry/webhook/flag tests — split
    // from `crates/lazuli_cli/src/tests.rs`.

    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{ExpandSet, inspect_canonical_source, inspect_json_value, parse_expand_set};

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
