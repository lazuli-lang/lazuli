    // RUNTIME-REACHABLE-STUB-001 end-to-end doctor tests (W2-2).
    //
    // Drives a real `.lzi` source through the full doctor package and
    // asserts the analyze-time diagnostic fires when a DSL construct
    // lowers to a KNOWN not-implemented 501 runtime arm — here a resource
    // `retention <dur> then archive` policy, whose runtime arm returns
    // `ErrRetentionArchiveNotImplemented`
    // (`runtime/go/lazuli/retention.go: applyRetentionAction`). The clean
    // sibling uses `then delete` (a live runtime arm) and must NOT fire —
    // no false-positive on the implemented retention actions the pilots
    // actually use.

    use super::test_support_packages::*;
    use crate::doctor::*;

    // A resource carrying `retention 90d then archive`. The archive arm is
    // a documented runtime stub (it 501s "retention archive action not yet
    // implemented"), so the resource compiles + `go build`s but the
    // retention worker fails on first sweep. The diagnostic must fire at
    // analyze time instead.
    const ARCHIVE_SRC: &str = r#"
feature ledgers
  uses org

  domain
    resource Entry
      tenancy org
      soft_delete
      amount: Integer required
      timestamps
      retention 90d then archive
"#;

    // The same shape but with `then delete` — a runtime arm that IS
    // implemented (`applyRetentionDelete`). Must NOT fire.
    const DELETE_SRC: &str = r#"
feature ledgers
  uses org

  domain
    resource Entry
      tenancy org
      soft_delete
      amount: Integer required
      timestamps
      retention 90d then delete
"#;

    #[test]
    fn doctor_fires_on_retention_archive() {
        let package = package_from_sources(vec![("ledgers.lzi", ARCHIVE_SRC)]);
        let diagnostics = package.diagnostics();

        let hit = diagnostics
            .iter()
            .find(|d| d.code == "RUNTIME-REACHABLE-STUB-001")
            .unwrap_or_else(|| {
                panic!(
                    "expected RUNTIME-REACHABLE-STUB-001 to fire on `retention ... then archive`, \
                     got: {diagnostics:#?}"
                )
            });
        assert_eq!(hit.severity, DoctorSeverity::Error);
        assert!(
            hit.message.contains("not yet implemented"),
            "message should explain the runtime stub: {}",
            hit.message
        );
        assert!(
            hit.message.contains("Entry"),
            "message should name the offending resource: {}",
            hit.message
        );
    }

    #[test]
    fn doctor_does_not_fire_on_retention_delete() {
        let package = package_from_sources(vec![("ledgers.lzi", DELETE_SRC)]);
        let diagnostics = package.diagnostics();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == "RUNTIME-REACHABLE-STUB-001"),
            "RUNTIME-REACHABLE-STUB-001 must not fire on the implemented `then delete` arm, \
             got: {:#?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "RUNTIME-REACHABLE-STUB-001")
                .collect::<Vec<_>>()
        );
    }
