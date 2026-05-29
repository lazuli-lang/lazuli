    // Doctor lazuli_version_001/002 rule tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_core::*;
    use super::test_support_packages::*;
    use crate::doctor::*;
    use crate::doctor::aggregators::runtime_version::{
        lazuli_version_001_diagnostics, lazuli_version_002_diagnostics,
    };

    #[test]
    fn lazuli_version_001_warns_when_missing_in_0_x() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "LAZULI-VERSION-001");
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Warning);
        assert!(
            diagnostics[0]
                .message
                .contains("Expected: lazuli_version \"0.12\""),
            "user-facing prose should advertise the expected pin: {}",
            diagnostics[0].message
        );
    }

    /// Regression for the R1.C real-world sweep — the user-facing message
    /// must not leak the internal debug suffix `expected_value = "..."`.
    #[test]
    fn lazuli_version_001_message_has_no_debug_leakage() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.14.0");
        assert_eq!(diagnostics.len(), 1);
        assert!(
            !diagnostics[0].message.contains("expected_value ="),
            "LAZULI-VERSION-001 message should not contain debug leakage: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn lazuli_version_001_errors_when_missing_in_1_0() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "1.0.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn lazuli_version_001_errors_when_mismatched_with_recipe_path() {
        let package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.11\"\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
        assert!(
            diagnostics[0]
                .message
                .contains("migrations/recipes/0.11-to-0.12")
        );
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn lazuli_version_001_no_diagnostic_when_pin_matches() {
        let package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.12\"\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn lazuli_version_002_errors_when_no_recipe_dir() {
        let mut package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.5\"\n")]);
        package.project_root = temp_project("version-no-recipe");
        let diagnostics =
            lazuli_version_002_diagnostics(package.app.as_ref(), "0.12.0", &package.project_root);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "LAZULI-VERSION-002");
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn lazuli_version_002_silent_when_recipe_exists() {
        let mut package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.11\"\n")]);
        package.project_root = temp_project("version-recipe");
        fs::create_dir_all(
            package
                .project_root
                .join("migrations/recipes/0.11-to-0.12/sample"),
        )
        .unwrap();
        let diagnostics =
            lazuli_version_002_diagnostics(package.app.as_ref(), "0.12.0", &package.project_root);
        assert!(diagnostics.is_empty());
    }

