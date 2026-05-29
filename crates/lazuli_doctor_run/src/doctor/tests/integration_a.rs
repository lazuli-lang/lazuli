    // Doctor integration bindings + packs + adapter provenance tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_validates_feature_integration_bindings() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    payments
  bindings
    payments.gateway = integrations.mercadopago
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    mercadopago: PaymentGateway
      adapter @adapter.mercadopago
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            package
                .diagnostics()
                .into_iter()
                .filter(|d| !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT")
                .collect::<Vec<_>>()
                .is_empty()
        );
    }

    #[test]
    fn doctor_resolves_features_and_requirements_from_enabled_packs() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    payments
  packs
    payments from registry.packs.payments
  bindings
    payments.gateway = registry.integrations.mercadopago
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    mercadopago: PaymentGateway
      adapter @adapter.mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            package.diagnostics().is_empty(),
            "expected enabled pack to satisfy uses and binding contracts"
        );
    }

    #[test]
    fn doctor_reports_unknown_enabled_pack() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    payments
  packs
    payments from registry.packs.payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
        )]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-PACK-002"));
        assert!(codes.contains("APP-USES-002"));
    }

    #[test]
    fn doctor_reports_unknown_adapter_provenance() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    customer
  targets
    backend go
  environments
    local
  integrations
    crm: CRMProvider
      adapter @unknown.crm
  runtime
    unit api
      serves commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @unknown.serasa
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  integrations
    crm adapter @unknown.fake_crm
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ADAPTER-001"));
        assert!(codes.contains("REG-ADAPTER-001"));
        assert!(codes.contains("PROFILE-ADAPTER-001"));
    }

    #[test]
    fn doctor_reports_missing_and_mismatched_feature_integration_bindings() {
        let missing = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            missing
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-001")
        );

        let mismatched = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    payments
  bindings
    payments.gateway = integrations.serasa
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @adapter.serasa
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            mismatched
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-004")
        );
    }

