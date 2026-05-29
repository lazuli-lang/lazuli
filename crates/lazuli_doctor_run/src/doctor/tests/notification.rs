    // Doctor notification digest + throttle diagnostics tests
    // Split from crates/lazuli_cli/src/doctor/tests.rs.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    // =========================================================================

    fn notification_package(extra_children: &str) -> DoctorPackage {
        let source = format!(
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
{extra_children}
"#
        );
        package_from_sources(vec![("package.lzi", source.as_str())])
    }

    fn assert_notification_diag(code: &str, extra_children: &str) {
        let package = notification_package(extra_children);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&code), "expected {code}, got {codes:?}");
    }

    /// `NOTIF-DIGEST-001` fires when `digest every "<duration>"` does
    /// not match the closed shape `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_digest_001_every_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 month"
      group_by customer_id
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-001"),
            "expected NOTIF-DIGEST-001, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-002` fires when `digest max_size` is 0 or above
    /// the 10_000 ceiling. Both extremes are authoring smells: 0 is
    /// dead; > 10k blows up the in-window buffer.
    #[test]
    fn notif_digest_002_max_size_out_of_range() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      max_size 99999
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-002"),
            "expected NOTIF-DIGEST-002, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-003` fires when `digest template_strategy` is not
    /// in the closed catalog.
    #[test]
    fn notif_digest_003_template_strategy_unknown() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      template_strategy squash
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-003"),
            "expected NOTIF-DIGEST-003, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-001` fires when neither `per_recipient` nor
    /// `per_channel` is present.
    #[test]
    fn notif_throttle_001_axis_missing() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 hour"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-001"),
            "expected NOTIF-THROTTLE-001, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-002` fires when `burst` is larger than the
    /// parsed `max_per` window.
    #[test]
    fn notif_throttle_002_burst_exceeds_max_per() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 second"
      per_recipient
      burst 2
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-002"),
            "expected NOTIF-THROTTLE-002, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-003` fires when `throttle max_per` does not
    /// match `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_throttle_003_max_per_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "forever"
      per_recipient
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-003"),
            "expected NOTIF-THROTTLE-003, got {codes:?}"
        );
    }

    /// Two extra cases per new diagnostic, paired with the focused
    /// tests above, give each code three covered variants without
    /// repeating a full package fixture 18 times.
    #[test]
    fn notif_digest_throttle_diagnostics_cover_three_cases_each() {
        for extra in [
            "    digest\n      every forever\n",
            "    digest\n      every \"\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-001", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      max_size 0\n",
            "    digest\n      every 1h\n      max_size 10001\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-002", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      template_strategy replace\n",
            "    digest\n      every 1h\n      template_strategy \"merge\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-003", extra);
        }
        for extra in [
            "    throttle\n      max_per 1h\n",
            "    throttle\n      max_per 1h\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-001", extra);
        }
        for extra in [
            "    throttle\n      max_per 1s\n      per_channel\n      burst 2\n",
            "    throttle\n      max_per 0s\n      per_recipient\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-002", extra);
        }
        for extra in [
            "    throttle\n      max_per later\n      per_channel\n",
            "    throttle\n      max_per \"1 month\"\n      per_recipient\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-003", extra);
        }
    }

