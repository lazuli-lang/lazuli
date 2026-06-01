    // Doctor policy/audit hints + unresolved bare type refs + cross-feature type refs
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    const SEMANTIC_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/semantic_unknown.lzi");

    const DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required
      name: Text required

  command create
    input
      name: Text required
    creates Customer from input
"#;

    const DOCTOR_HINTS_GUARDED_WRITE_FIXTURE: &str = r#"
feature customer
  policies
    create: @role.admin

  domain
    resource Customer
      id: ID required
      name: Text required

  command create
    policy @policy.create
    audit default
    input
      name: Text required
    creates Customer from input
"#;

    const DOCTOR_HINTS_UNWRITTEN_RESOURCE_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required

  command preview
    returns Customer
"#;

    #[test]
    fn doctor_hints_resource_without_policy_for_written_resource() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "resource_without_policy_hint")
            .collect();

        assert_eq!(
            hits.len(),
            1,
            "expected one resource hint, got {diagnostics:?}"
        );
        let diagnostic = hits[0];
        assert_eq!(diagnostic.severity, DoctorSeverity::Hint);
        assert_eq!(diagnostic.line, 4);
        assert_eq!(
            diagnostic.message,
            "feature `customer` declares resource `Customer` with no `policies` block ÔÇö every write command implicitly gets the default policy. Add an explicit `policies` block to make access control auditable."
        );
    }

    #[test]
    fn doctor_hints_command_without_audit_for_write_command() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "command_without_audit_hint")
            .collect();

        assert_eq!(
            hits.len(),
            1,
            "expected one command hint, got {diagnostics:?}"
        );
        let diagnostic = hits[0];
        assert_eq!(diagnostic.severity, DoctorSeverity::Hint);
        assert_eq!(diagnostic.line, 8);
        assert_eq!(
            diagnostic.message,
            "command `customer.create` is write-effect but has no `audit default` declared ÔÇö write actions without audit are invisible to compliance. Add `audit default` on the command or `audit_default` in feature defaults."
        );
    }

    #[test]
    fn doctor_hints_suppressed_when_policy_block_and_audit_declared() {
        let package =
            package_from_sources(vec![("customer.lzi", DOCTOR_HINTS_GUARDED_WRITE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);

        assert!(!codes.contains("resource_without_policy_hint"));
        assert!(!codes.contains("command_without_audit_hint"));
    }

    #[test]
    fn doctor_hints_skip_unwritten_resource_and_returns_command() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_UNWRITTEN_RESOURCE_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);

        assert!(!codes.contains("resource_without_policy_hint"));
        assert!(!codes.contains("command_without_audit_hint"));
    }

    #[test]
    fn doctor_emits_semantic_type_unknown_for_unknown_semantic_fields() {
        let package =
            package_from_sources(vec![("semantic_unknown.lzi", SEMANTIC_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == SEMANTIC_TYPE_UNKNOWN_CODE)
            .collect();

        assert!(
            hits.len() >= 2,
            "expected at least two semantic_type_unknown diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits.iter().any(|diagnostic| {
            diagnostic.line == 8
                && diagnostic.message
                    == "unknown @semantic type \"@semantic.Distance\"; the closed catalog is {EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT, HEXCOLOR, PERCENTAGE}."
        }));
        assert!(hits.iter().any(|diagnostic| {
            diagnostic.line == 15
                && diagnostic.message
                    == "unknown @semantic type \"@semantic.Range\"; the closed catalog is {EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT, HEXCOLOR, PERCENTAGE}."
        }));
    }

    const CROSS_FEATURE_TYPE_UNRESOLVED_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required
      owner: User required

    record InviteDraft
      reviewer: Reviewer required
"#;

    #[test]
    fn doctor_reports_unresolved_bare_type_refs_on_resource_and_record_fields() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            CROSS_FEATURE_TYPE_UNRESOLVED_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let messages: BTreeSet<&str> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "cross_feature_type_unresolved")
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.contains(
            "type `User` referenced by `customer.Customer.owner` is not declared in any feature. Add a `resource`/`record`/`enum User` block, or check for a typo."
        ));
        assert!(messages.contains(
            "type `Reviewer` referenced by `customer.InviteDraft.reviewer` is not declared in any feature. Add a `resource`/`record`/`enum Reviewer` block, or check for a typo."
        ));
    }

    #[test]
    fn doctor_reports_unresolved_bare_type_refs_on_command_input_slots() {
        let mut package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command create
    input
      email: Text required
    returns Text
"#,
        )]);
        let command = package
            .tier3_facts
            .iter_mut()
            .flat_map(|fact| fact.commands.iter_mut())
            .find(|command| command.name == "create")
            .expect("expected create command fact");
        let lazuli_ir::CommandInput::Typed(slots) = &mut command.input else {
            panic!("expected typed command input");
        };
        slots[0].type_ref = lazuli_ir::TypeRef::UserDefined(lazuli_ir::QualifiedName {
            feature: None,
            name: "EmailAddress".to_owned(),
        });

        let diagnostics = package.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "cross_feature_type_unresolved"
                && diagnostic.message
                    == "type `EmailAddress` referenced by `customer.create.input.email` is not declared in any feature. Add a `resource`/`record`/`enum EmailAddress` block, or check for a typo."
        }));
    }

    #[test]
    fn doctor_allows_bare_type_refs_declared_in_any_feature() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    resource Customer
      id: ID required
      owner: User required

feature identity
  domain
    resource User
      id: ID required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cross_feature_type_unresolved"),
            "declared cross-feature type should resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    const FEATURE_USES_MISSING_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required

    record CustomerView
      id: ID required

feature identity
  domain
    record UserProfile
      id: ID required

feature catalog
  domain
    record ProductFilter
      sku: Text required

feature orders
  domain
    resource Order
      customer: Customer required

    query.list by_product
      params
        filter: ProductFilter required

  command assign_user
    input
      assignee: UserProfile required
    returns CustomerView
"#;

    #[test]
    fn doctor_warns_when_cross_feature_type_refs_omit_uses() {
        let package = package_from_sources(vec![("orders.lzi", FEATURE_USES_MISSING_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "feature_uses_missing")
            .collect();
        let messages: BTreeSet<&str> = hits
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert_eq!(
            hits.len(),
            3,
            "expected one missing-uses warning per referenced feature, got {hits:#?}; all diagnostics: {diagnostics:#?}"
        );
        assert!(
            hits.iter()
                .all(|diagnostic| diagnostic.severity == DoctorSeverity::Warning)
        );
        assert!(messages.contains(
            "feature `orders` references types declared in feature `customer` but does not declare `uses customer` in its header. Add `uses customer` to make the dependency explicit."
        ));
        assert!(messages.contains(
            "feature `orders` references types declared in feature `identity` but does not declare `uses identity` in its header. Add `uses identity` to make the dependency explicit."
        ));
        assert!(messages.contains(
            "feature `orders` references types declared in feature `catalog` but does not declare `uses catalog` in its header. Add `uses catalog` to make the dependency explicit."
        ));
    }

    #[test]
    fn doctor_allows_cross_feature_type_refs_with_declared_uses() {
        let fixture = FEATURE_USES_MISSING_FIXTURE.replace(
            "feature orders\n",
            "feature orders\n  uses customer, identity, catalog\n",
        );
        let package = package_from_sources(vec![("orders.lzi", fixture.as_str())]);
        let diagnostics = package.diagnostics();

        assert!(
            !codes(&diagnostics).contains("feature_uses_missing"),
            "declared uses should satisfy cross-feature refs; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // ── spec 0004 — `defaults rate_limit` / `defaults audit` hoist hints ──

    // Three commands repeat the same `rate_limit` + `audit default` and the
    // feature does NOT hoist either → both hints fire (≥3 threshold).
    const DEFAULTS_HOIST_REPEAT_FIXTURE: &str = r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      amount: Integer required
    creates Invoice from input

  command void_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      id: ID required
    updates Invoice from input

  command delete_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      id: ID required
    deletes Invoice
"#;

    #[test]
    fn doctor_fires_defaults_hoist_hints_at_three_identical() {
        let package =
            package_from_sources(vec![("billing.lzi", DEFAULTS_HOIST_REPEAT_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let rate = diagnostics
            .iter()
            .filter(|d| d.code == "defaults_hoist_rate_limit_hint")
            .count();
        let audit = diagnostics
            .iter()
            .filter(|d| d.code == "defaults_hoist_audit_hint")
            .count();
        assert_eq!(
            rate, 1,
            "expected one rate_limit hoist hint, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(
            audit, 1,
            "expected one audit hoist hint, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Only two identical commands → below the ≥3 threshold → silent.
    const DEFAULTS_HOIST_TWO_FIXTURE: &str = r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      amount: Integer required
    creates Invoice from input

  command void_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      id: ID required
    updates Invoice from input
"#;

    #[test]
    fn doctor_silent_defaults_hoist_below_threshold() {
        let package = package_from_sources(vec![("billing.lzi", DEFAULTS_HOIST_TWO_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(!codes.contains("defaults_hoist_rate_limit_hint"));
        assert!(!codes.contains("defaults_hoist_audit_hint"));
    }

    // Already hoisted into `defaults` → the inheritance pass bakes the value
    // onto every command, but the `defaults_*` guard keeps the hint silent.
    const DEFAULTS_HOIST_ALREADY_FIXTURE: &str = r#"
feature billing
  defaults
    rate_limit "60 per minute per actor"
    audit default

  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    input
      amount: Integer required
    creates Invoice from input

  command void_invoice
    input
      id: ID required
    updates Invoice from input

  command delete_invoice
    input
      id: ID required
    deletes Invoice
"#;

    #[test]
    fn doctor_silent_when_already_hoisted() {
        let package =
            package_from_sources(vec![("billing.lzi", DEFAULTS_HOIST_ALREADY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("defaults_hoist_rate_limit_hint"),
            "a feature that already hoists rate_limit must not get the hint"
        );
        assert!(
            !codes.contains("defaults_hoist_audit_hint"),
            "a feature that already hoists audit must not get the hint"
        );
    }

