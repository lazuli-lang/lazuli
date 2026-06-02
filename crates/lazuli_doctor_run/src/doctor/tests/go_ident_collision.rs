    // Doctor CODEGEN-GO-IDENT-COLLISION-008 dispatch tests.
    // Proves the emitted-identifier-uniqueness rule surfaces from the
    // package-level `DoctorPackage::diagnostics()` stream (i.e. from
    // `lazuli doctor` / `lazuli check`), not just from its unit tests.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    const COLLISION_FIXTURE: &str =
        include_str!("../../../tests/fixtures/go-ident-collision/collision.lzi");
    const CLEAN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/go-ident-collision/clean.lzi");

    #[test]
    fn collision_fixture_fires_go_ident_collision_008() {
        let package = package_from_sources(vec![("collision.lzi", COLLISION_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "CODEGEN-GO-IDENT-COLLISION-008"),
            1,
            "expected exactly one CODEGEN-GO-IDENT-COLLISION-008 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        // The diagnostic must name both colliding constructs + the shared ident.
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "CODEGEN-GO-IDENT-COLLISION-008")
            .expect("collision diagnostic present");
        assert!(
            diag.message.contains("Status"),
            "message must name the shared emitted identifier: {}",
            diag.message
        );
        assert!(
            diag.message.contains("enum") && diag.message.contains("query"),
            "message must name both colliding construct kinds: {}",
            diag.message
        );
    }

    #[test]
    fn clean_fixture_does_not_false_positive() {
        let package = package_from_sources(vec![("clean.lzi", CLEAN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "CODEGEN-GO-IDENT-COLLISION-008"),
            0,
            "clean fixture must not fire CODEGEN-GO-IDENT-COLLISION-008; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
