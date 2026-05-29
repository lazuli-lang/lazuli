    // VOCAB-KNOWLEDGE-SINGLE-FEATURE-001 — package-level (cross-feature)
    // dispatch tests. The rule is meaningless per-file (a lone feature can
    // never satisfy the shared 1:N invariant), so these exercise it through
    // the full `DoctorPackage::diagnostics()` path over a two-feature package.

    use super::test_support_core::*;
    use super::test_support_packages::*;
    use crate::doctor::*;

    const SOLO_SECTOR_FIXTURE: &str =
        include_str!("../../../tests/fixtures/knowledge-single-feature/solo_sector.lzi");
    const SHARED_SECTOR_FIXTURE: &str =
        include_str!("../../../tests/fixtures/knowledge-single-feature/shared_sector.lzi");

    const CODE: &str = "VOCAB-KNOWLEDGE-SINGLE-FEATURE-001";

    #[test]
    fn fires_for_solo_sector() {
        let package = package_from_sources(vec![("app.lzi", SOLO_SECTOR_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, CODE),
            1,
            "expected SINGLE-FEATURE to fire once for the solo sector; got: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == CODE)
                .map(|d| (&d.feature_name, &d.message))
                .collect::<Vec<_>>()
        );
        let finding = diagnostics
            .iter()
            .find(|d| d.code == CODE)
            .expect("the solo-sector finding");
        assert_eq!(finding.feature_name.as_deref(), Some("alpha"));
        assert!(
            finding.message.contains("solo-sector"),
            "message names the sector: {}",
            finding.message
        );
    }

    #[test]
    fn silent_for_shared_sector() {
        let package = package_from_sources(vec![("app.lzi", SHARED_SECTOR_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, CODE),
            0,
            "shared sector (declared by 2 features) must not fire; got: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == CODE)
                .map(|d| (&d.feature_name, &d.message))
                .collect::<Vec<_>>()
        );
    }
