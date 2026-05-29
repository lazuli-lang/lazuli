    // Doctor previously / tenant_migration / deploy tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_core::*;
    use super::test_support_packages::*;
    use crate::doctor::*;

    const MIGRATIONS_PREVIOUSLY_FWD_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/previously_forward_unresolved.lzi");
    const MIGRATIONS_PREVIOUSLY_CYCLE_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/previously_cycle.lzi");
    const MIGRATIONS_PREVIOUSLY_DUP_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/previously_duplicate_claim.lzi");
    const MIGRATIONS_TM_AXIS_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/tenant_migration_axis_unknown.lzi");
    const MIGRATIONS_TM_IDEMP_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/tenant_migration_no_idempotency.lzi");
    const MIGRATIONS_CHECKPOINT_INVALID_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/deploy_checkpoint_path_invalid.lzi");
    const MIGRATIONS_STRATEGY_INVALID_FIXTURE: &str =
        include_str!("../../../tests/fixtures/migrations/deploy_strategy_invalid.lzi");
    const MIGRATIONS_TM_TARGET_UNKNOWN_FIXTURE: &str = r#"
feature x
  defaults
    tenancy org

  tenant_migration backfill_x
    target query.missing
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_x.go"
"#;
    const MIGRATIONS_TM_HANDLER_MISSING_FIXTURE: &str = r#"
feature x
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_x
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_x.go"
"#;

    #[test]
    fn previously_forward_unresolved_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_FWD_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-FWD-001"),
            "expected PREVIOUSLY-FWD-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn previously_cycle_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_CYCLE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-CYCLE-001"),
            "expected PREVIOUSLY-CYCLE-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn previously_duplicate_claim_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_DUP_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-DUP-001"),
            "expected PREVIOUSLY-DUP-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_axis_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_AXIS_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-axis-mismatch"),
            "expected tenant-migration-axis-mismatch in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_no_idempotency_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_IDEMP_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-idempotency-required"),
            "expected tenant-migration-idempotency-required in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_target_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_TARGET_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-target-unknown"),
            "expected tenant-migration-target-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_handler_missing_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_HANDLER_MISSING_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-handler-missing"),
            "expected tenant-migration-handler-missing in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deploy_checkpoint_path_invalid_fires() {
        let package =
            package_from_sources(vec![("app.lzi", MIGRATIONS_CHECKPOINT_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-CHECKPOINT-001"),
            "expected DEPLOY-CHECKPOINT-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deploy_strategy_invalid_fires() {
        let package = package_from_sources(vec![("app.lzi", MIGRATIONS_STRATEGY_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-STRATEGY-001"),
            "expected DEPLOY-STRATEGY-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // DEPLOY-CHECKPOINT-002 (stale snapshot) requires an on-disk
    // snapshot file. The fixture lives in
    // `tests/fixtures/migrations/snapshot_stale/` so the doctor rule
    // can resolve the path relative to the manifest's location.
    #[test]
    fn deploy_checkpoint_stale_fires() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/migrations/snapshot_stale/app.lzi");
        let source = std::fs::read_to_string(&manifest_path).expect("read app");
        let mut package = package_from_sources(vec![]);
        if let Some(manifest) = parse_app_manifest(&source) {
            package.app = Some(DoctorAppManifest {
                path: manifest_path,
                source,
                manifest,
            });
        }
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-CHECKPOINT-002"),
            "expected DEPLOY-CHECKPOINT-002 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

