    // Doctor i18n locale + message-ref + cldr-plural tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;


    const I18N_DEFAULT_NOT_SUPPORTED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/i18n/default_not_supported.lzi");
    const I18N_TRANSLATION_LOCALE_UNSUPPORTED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/i18n/translation_locale_unsupported.lzi");
    const I18N_TRANSLATION_KEY_UNRESOLVED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/i18n/translation_key_unresolved.lzi");
    const I18N_CLDR_PLURAL_ARM_INVALID_FIXTURE: &str =
        include_str!("../../../tests/fixtures/i18n/cldr_plural_arm_invalid.lzi");
    const I18N_LOCALE_NEGOTIATE_SOURCE_INVALID_FIXTURE: &str =
        include_str!("../../../tests/fixtures/i18n/locale_negotiate_source_invalid.lzi");

    #[test]
    fn app_locale_default_unsupported_fires() {
        let package = package_from_sources(vec![("app.lzi", I18N_DEFAULT_NOT_SUPPORTED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_locale_default_unsupported"),
            "expected app_locale_default_unsupported in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn translation_locale_unsupported_fires() {
        let package = package_from_sources(vec![(
            "app.lzi",
            I18N_TRANSLATION_LOCALE_UNSUPPORTED_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("translation_locale_unsupported"),
            "expected translation_locale_unsupported in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rule_message_ref_unresolved_fires() {
        let package =
            package_from_sources(vec![("app.lzi", I18N_TRANSLATION_KEY_UNRESOLVED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("rule_message_ref_unresolved"),
            "expected rule_message_ref_unresolved in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cldr_plural_arm_invalid_fires() {
        let package = package_from_sources(vec![("app.lzi", I18N_CLDR_PLURAL_ARM_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cldr_plural_arm_invalid"),
            "expected cldr_plural_arm_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn locale_negotiate_source_invalid_fires() {
        let package = package_from_sources(vec![(
            "app.lzi",
            I18N_LOCALE_NEGOTIATE_SOURCE_INVALID_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("locale_negotiate_source_invalid"),
            "expected locale_negotiate_source_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // MISSING-POLICY-ON-QUERY-001 - query public fallback visibility.
    // =========================================================================
