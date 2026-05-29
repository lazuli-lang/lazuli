    // AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 — dual-identity ctx.User slot collision.
    // Mirrors the auth_a / auth_b aggregator tests: build a real package via
    // `package_from_sources`, run the full dispatcher, assert on codes.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    // App manifest fragment whose `actor_query` resolves the authenticated
    // actor to `customer.query.by_id` (a `Customer`, NOT a `User`).
    const APP_ACTOR_IS_CUSTOMER: &str = r#"
app AcmeCRM
  actor_query "customer.query.by_id"
"#;

    // The dual-identity feature set: `customer` owns `Customer` (whose owner
    // is a `User`) and a command that writes `owner = ctx.user`; `account`
    // owns the staff `User` resource. Mirrors examples/full-capsule.
    const DUAL_IDENTITY_FEATURES: &str = r#"
feature customer
  domain
    resource Customer
      owner: User optional
      name: Text required
      email: @semantic.Email required

    query.lookup by_id by id: ID

  command create
    input
      name: Text required
      email: @semantic.Email required
    creates Customer
      name = input.name
      email = input.email
      owner = ctx.user

feature account
  domain
    resource User
      email: @semantic.Email required unique
      name: Text required
"#;

    #[test]
    fn fires_when_actor_is_customer_and_command_writes_ctx_user() {
        let package = package_from_sources(vec![
            ("app.lzi", APP_ACTOR_IS_CUSTOMER),
            ("features.lzi", DUAL_IDENTITY_FEATURES),
        ]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "AUTH-ACTOR-SUBJECT-AMBIGUOUS-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one AUTH-ACTOR-SUBJECT-AMBIGUOUS-001; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        // Warn-level by design — never error, even though AUTH-* maps to
        // the Security category (which escalates to error under Production).
        assert_eq!(
            hits[0].severity,
            DoctorSeverity::Warning,
            "AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 must be warn-level (never error)"
        );
        assert!(
            hits[0].message.contains("Customer") && hits[0].message.contains("ctx.User"),
            "diagnostic should name the resolved subject + the shared slot: {}",
            hits[0].message
        );
    }

    #[test]
    fn fires_on_same_org_scope_policy_against_user_app() {
        // Variant of (b): the owner/scope site is a `@scope.same_org`
        // policy on a query rather than a `ctx.user` assignment.
        let features = r#"
feature customer
  domain
    resource Customer
      name: Text required

    query.lookup by_id by id: ID

  policies
    read: @scope.same_org

  query.list everything
    policy @policy.read

feature account
  domain
    resource User
      email: @semantic.Email required unique
"#;
        let package = package_from_sources(vec![
            ("app.lzi", APP_ACTOR_IS_CUSTOMER),
            ("features.lzi", features),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("AUTH-ACTOR-SUBJECT-AMBIGUOUS-001"),
            "expected AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 for @scope.same_org against a User app; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn silent_for_single_identity_app() {
        // actor_query resolves to `account.query.me` -> the `User`
        // resource itself. Single-identity: the slot is never shared, so
        // the rule stays silent even though a command writes ctx.user.
        let app = r#"
app AcmeCRM
  actor_query "account.query.me"
"#;
        let features = r#"
feature account
  domain
    resource User
      email: @semantic.Email required unique
      name: Text required

    query.lookup me by id: ID

  command touch
    route id: ID
    input
      name: Text required
    updates User
      name = input.name
"#;
        let package = package_from_sources(vec![("app.lzi", app), ("features.lzi", features)]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("AUTH-ACTOR-SUBJECT-AMBIGUOUS-001"),
            "single-identity app (actor_query -> User) must not fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn silent_for_customer_only_app_with_no_user_resource() {
        // actor_query resolves to `Customer`, a command writes ctx.user,
        // but there is NO `User` resource anywhere: no second identity to
        // collapse into, so the slot is never ambiguous.
        let features = r#"
feature customer
  domain
    resource Customer
      owner_id: ID optional
      name: Text required

    query.lookup by_id by id: ID

  command create
    input
      name: Text required
    creates Customer
      name = input.name
      owner_id = ctx.user.id
"#;
        let package = package_from_sources(vec![
            ("app.lzi", APP_ACTOR_IS_CUSTOMER),
            ("features.lzi", features),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("AUTH-ACTOR-SUBJECT-AMBIGUOUS-001"),
            "Customer-only app with no User resource must not fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn inline_allow_silences_the_finding() {
        // The same dual-identity shape as the positive test, but the app
        // manifest carries the canonical opt-out comment — exercising the
        // `# doctor:allow` mechanism end to end.
        let app = r#"
app AcmeCRM
  # doctor:allow AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 -- reason "canonical dual-identity demonstration"
  actor_query "customer.query.by_id"
"#;
        let package = package_from_sources(vec![
            ("app.lzi", app),
            ("features.lzi", DUAL_IDENTITY_FEATURES),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("AUTH-ACTOR-SUBJECT-AMBIGUOUS-001"),
            "inline `# doctor:allow` must silence the finding; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
