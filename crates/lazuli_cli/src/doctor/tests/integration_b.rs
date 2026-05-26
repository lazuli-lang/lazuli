    // Doctor external-calls + profiles validation tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_validates_external_calls_against_feature_requirements() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit worker
      runs jobs *
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
    crm: CRMProvider
      adapter @adapter.crm
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider

  job process_import
    trigger event import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected external call contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  job process_import
    trigger event import_uploaded
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("INT-CALL-001"));
        assert!(codes.contains("INT-CALL-002"));
        assert!(codes.contains("INT-CALL-003"));
        assert!(codes.contains("INT-CALL-004"));
    }

    #[test]
    fn doctor_validates_profiles_against_app_and_registry_contracts() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
    web react
  environments
    local
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit worker
      runs jobs *
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
    crm: CRMProvider
      adapter @adapter.crm
      environments sandbox, production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    imports.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
"#,
            ),
        ]);

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected profile contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
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
      environments production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
  bindings
    imports.crm = integrations.serasa
  integrations
    crm environment sandbox
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-BIND-001"));
        assert!(codes.contains("PROFILE-001"));
        assert!(codes.contains("PROFILE-INT-001"));
        assert!(codes.contains("PROFILE-BIND-004"));
    }

