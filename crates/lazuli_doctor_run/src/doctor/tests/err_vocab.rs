    // Doctor error-vocab rule tests
    // Split from crates/lazuli_cli/src/doctor/tests.rs.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    const ERR_VOCAB_NO_WHEN_DENIED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/no_when_denied.lzi");
    const ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/key_unknown_from_policy.lzi");
    const ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/builtin_fallback.lzi");
    const ERR_VOCAB_CODE_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/code_unknown.lzi");
    const ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/expose_unknown.lzi");
    const ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/when_denied_no_policy.lzi");
    const ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/expose_5xx_message.lzi");
    const ERR_VOCAB_HAPPY_FIXTURE: &str =
        include_str!("../../../tests/fixtures/error-vocab/happy.lzi");

    fn err_vocab_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ERR-VOCAB-"))
            .collect()
    }

    #[test]
    fn err_vocab_001_fires_for_no_when_denied_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_NO_WHEN_DENIED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-001"),
            1,
            "expected ERR-VOCAB-001 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_002_fires_for_key_unknown_from_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-002"),
            1,
            "expected ERR-VOCAB-002 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_003_fires_for_builtin_fallback_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-003"),
            1,
            "expected ERR-VOCAB-003 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_code_unknown_fires_for_code_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_CODE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-CODE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-CODE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_unknown_fires_for_expose_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-EXPOSE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_when_denied_no_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-WHEN-DENIED-NO-POLICY"),
            1,
            "expected ERR-VOCAB-WHEN-DENIED-NO-POLICY to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_5xx_message_fires_for_expose_5xx_message_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-5XX-MESSAGE"),
            1,
            "expected ERR-VOCAB-EXPOSE-5XX-MESSAGE to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_happy_fixture_fires_no_err_vocab_diagnostics() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_HAPPY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab: Vec<_> = err_vocab_diags(&diagnostics);
        assert!(
            err_vocab.is_empty(),
            "happy.lzi must emit zero ERR-VOCAB-* diagnostics; got: {:?}",
            err_vocab.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Cross-feature key resolution: `feature sales` declares
    // `policies create.when_denied @translation.shared_key` and that key
    // lives in `feature crm`'s translation block. `feature sales`
    // imports it via `uses crm`. ERR-VOCAB-002 must stay silent.
    #[test]
    fn err_vocab_002_silent_through_uses_two_features() {
        const CRM_FIXTURE: &str = r#"
app AcmeApp
  title "Acme"
  version "0.1.0"
  targets
    backend go
  environments
    local
  locale
    default "pt-BR"
    supported "pt-BR"

feature crm
  domain
    resource Customer
      id: ID required

  translation
    catalog "./i18n/crm.<locale>.json"

    key shared_key
      pt-BR "Apenas administradores podem realizar esta ação."
"#;
        const SALES_FIXTURE: &str = r#"
feature sales
  uses crm
  domain
    resource Lead
      id: ID required

  policies
    create: @role.sales
      when_denied @translation.shared_key

  command create
    policy @policy.create
    creates Lead
"#;
        let package =
            package_from_sources(vec![("crm.lzi", CRM_FIXTURE), ("sales.lzi", SALES_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab_002 = count_code(&diagnostics, "ERR-VOCAB-002");
        assert_eq!(
            err_vocab_002,
            0,
            "cross-feature `@translation.shared_key` (declared in `crm`, used by `sales`) must \
             resolve through `uses crm`; got ERR-VOCAB-002 diagnostics: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "ERR-VOCAB-002")
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }
