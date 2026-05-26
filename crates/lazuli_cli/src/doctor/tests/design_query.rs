    // Doctor design custom lints + query.view SQL tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_design_custom_lints_fire_for_collisions_reserved_and_bad_hex() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        // `design.lzi` with: a custom token that collides with a color group
        // entry, a reserved Shadcn-semantic name, and an invalid hex value.
        // We need the allowlist file present so `design_token_diagnostics`
        // doesn't bail out early — emit a minimal stub.
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["brand-blue"],"text":[],"font":[]}"#,
        );
        write_file(
            &root.join("design.lzi"),
            r##"design hostpoint
  color
    brand-blue "#28bbdd"
  custom
    brand-blue "#28bbdd"
    primary "#7c3aed"
    oops "not-a-color"
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("design-custom-duplicate"),
            "expected duplicate diagnostic; got {:?}",
            surfaced,
        );
        assert!(
            surfaced.contains("design-custom-reserved-name"),
            "expected reserved-name diagnostic; got {:?}",
            surfaced,
        );
        assert!(
            surfaced.contains("design-custom-invalid-value"),
            "expected invalid-value diagnostic; got {:?}",
            surfaced,
        );
    }

    #[test]
    fn doctor_design_custom_lints_silent_on_clean_design() {
        // Regression: a `design.lzi` with a well-formed custom group should
        // NOT fire any `design-custom-*` diagnostic.
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["primary","chat-bubble-mine"],"text":[],"font":[]}"#,
        );
        write_file(
            &root.join("design.lzi"),
            r##"design hostpoint
  color
    primary "#28bbdd"
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    map-marker-active "#ff5722"
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);
        assert!(
            !surfaced.contains("design-custom-duplicate"),
            "unexpected duplicate diagnostic on clean design; got {:?}",
            surfaced,
        );
        assert!(
            !surfaced.contains("design-custom-reserved-name"),
            "unexpected reserved-name diagnostic on clean design; got {:?}",
            surfaced,
        );
        assert!(
            !surfaced.contains("design-custom-invalid-value"),
            "unexpected invalid-value diagnostic on clean design; got {:?}",
            surfaced,
        );
    }

    #[test]
    fn doctor_pipeline_invokes_folder_and_design_rules() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("features/slug/web/views/admin/list.tsx"),
            "export function List() { return null; }\n",
        );
        // Orphan must live in a Lazuli-owned root (app/ | features/ | frontends/)
        // for the feature-orphan-component rule to see it; commit f4185a9
        // narrowed the rule's scope so `src/components/` is no longer walked.
        write_file(
            &root.join("app/components/Foo.tsx"),
            "export function Foo() { return null; }\n",
        );
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["primary"],"text":["foreground"],"font":["sans"]}"#,
        );
        write_file(
            &root.join("features/slug/web/views/admin/styled.tsx"),
            r##"export function Styled() {
  return <div style={{ color: "#7c3aed" }} />;
}
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("feature-orphan-component"),
            "expected folder rule to fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            surfaced.contains("design-token-hex-leak"),
            "expected design rule to fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_query_view_reports_missing_sql_file() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("app/features/host/host.lzi"),
            r#"
feature host
  record HostHomeRow
    id: ID required
  query.view host_home_view
    returns list of HostHomeRow
    source @file.host_home_view.sql
    params
      user_id: ID required
"#,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("QUERY-VIEW-SQL-FILE-001"),
            "expected missing SQL file diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_query_view_reports_unsafe_sql_pattern() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("app/features/host/host.lzi"),
            r#"
feature host
  record HostHomeRow
    id: ID required
  query.view host_home_view
    returns list of HostHomeRow
    source @file.host_home_view.sql
    params
      user_id: ID required
"#,
        );
        write_file(
            &root.join("app/features/host/queries/host_home_view.sql"),
            "select id from host_rows where title like '%' + $1 + '%'\n",
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("QUERY-VIEW-SQL-UNSAFE-001"),
            "expected unsafe SQL diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

