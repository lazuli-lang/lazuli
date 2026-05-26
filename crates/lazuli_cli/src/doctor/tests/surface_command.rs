    // Doctor public-surface + command-route binding tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use crate::doctor::*;

    #[test]
    fn doctor_reports_public_surface_reaching_staff_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin, @role.sales

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.create
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-POL-001"
                && diagnostic.message.contains("audience `public`")
                && diagnostic.message.contains("customer.command.create")
        }));
    }

    #[test]
    fn doctor_allows_public_surface_reaching_public_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    capture_lead: @scope.public

  command capture_lead
    policy @policy.capture_lead
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.capture_lead
"#,
            ),
        ]);

        // Filter out the `ERR-VOCAB-*` family — those are the new Cell
        // ANALYZE-1 warnings about missing `when_denied` overrides, not
        // related to the surface-to-command resolution this test pins.
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(diagnostics.is_empty(), "got: {:#?}", diagnostics);
    }

    #[test]
    fn doctor_resolves_platform_action_through_abstract_experience() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.lzx",
                r#"
experience customer
  imports customer

  view list
    action create -> customer.command.create
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      actions create
"#,
            ),
        ]);

        // Assert no BLOCKING diagnostics (Error/Warning). Info-level
        // advisories (e.g. `RBAC-CATALOG-MISSING-001` suggesting
        // migration to the top-level RBAC catalog) are non-blocking
        // suggestions and not part of this test's contract — the
        // assertion is "the platform action resolves through the
        // abstract experience without breaking validation". The new
        // `ERR-VOCAB-*` family (Cell ANALYZE-1) is also filtered here:
        // those nudge authors toward customized `when_denied` text but
        // do not block the surface-to-command resolution pinned here.
        let diagnostics = package.diagnostics();
        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(d.severity, DoctorSeverity::Error | DoctorSeverity::Warning)
                    && !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            blocking.is_empty(),
            "expected no blocking diagnostics, got: {:#?}",
            blocking
        );
    }

    #[test]
    fn doctor_reports_command_route_not_bound_by_surface_target() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    update: @role.admin

  command reassign
    route id: ID
    policy @policy.update
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view detail Form
      submit customer.command.reassign
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-ROUTE-001"
                && diagnostic
                    .message
                    .contains("required command route slot(s) id")
        }));
    }

    #[test]
    fn doctor_allows_command_route_bound_from_context() {
        let package = package_from_sources(vec![
            (
                "customer_auth.lzi",
                r#"
feature customer_auth
  policies
    update: @scope.same_org

  command enable_mfa
    route customer_id: ID from ctx.customer.id
    policy @policy.update
"#,
            ),
            (
                "customer_auth.web.lzx",
                r#"
surface customer_auth web
  uses experience customer_auth

  audience account
    view enable_mfa Form
      submit customer_auth.command.enable_mfa
"#,
            ),
        ]);

        // Filter out the `ERR-VOCAB-*` family — Cell ANALYZE-1 warnings
        // about missing `when_denied` overrides are orthogonal to the
        // route-binding-from-context behavior this test pins.
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(diagnostics.is_empty(), "got: {:#?}", diagnostics);
    }

