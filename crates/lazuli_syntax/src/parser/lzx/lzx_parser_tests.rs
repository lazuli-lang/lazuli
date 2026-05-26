//! End-to-end `parse_lzx_document` tests covering the full `.lzx` surface.
//!
//! Lives as a sibling of `parser::lzx::mod` (rather than inline) so the
//! module file stays under the 500-LOC ceiling. Raw-string fixtures are
//! verbatim — de-indenting them corrupts the canonical-indent contract
//! the parser asserts. The enclosing module path is preserved
//! (`parser::lzx::lzx_parser_tests::...`) via the
//! `#[cfg(test)] #[path = "lzx_parser_tests.rs"] mod lzx_parser_tests;`
//! declaration in `mod.rs`.

use super::parse_lzx_document;
use crate::{LzxPlatform, LzxViewTestAssertion};

/// Wave 4 — parser must lift view `tests` into the typed
/// `LzxViewTestAssertion` enum. `accepted by` / `rejected by` are
/// the only admissible shapes; anything else is a hard parse error.
#[test]
fn lzx_view_tests_lift_to_typed_assertions() {
    let source = "experience customer
  view detail
    anchor @anchor.customer_detail
    extensible_by customer_tags, customer_import
    source customer.query.by_id(id: route.id)

    tests
      accepted by customer_tags
      accepted by customer_import
      rejected by billing
";
    let document = parse_lzx_document(source).unwrap();
    let view = &document.experiences[0].views[0];
    assert_eq!(view.tests.len(), 3);

    match &view.tests[0] {
        LzxViewTestAssertion::AcceptedBy { feature, .. } => {
            assert_eq!(feature, "customer_tags")
        }
        other => panic!("expected AcceptedBy, got {other:?}"),
    }
    match &view.tests[1] {
        LzxViewTestAssertion::AcceptedBy { feature, .. } => {
            assert_eq!(feature, "customer_import")
        }
        other => panic!("expected AcceptedBy, got {other:?}"),
    }
    match &view.tests[2] {
        LzxViewTestAssertion::RejectedBy { feature, .. } => {
            assert_eq!(feature, "billing")
        }
        other => panic!("expected RejectedBy, got {other:?}"),
    }
}

/// Wave 4 — the parser must reject any view test assertion outside
/// the closed extensibility vocabulary (policy / predicate
/// vocabulary belongs to commands, rules, and transitions).
#[test]
fn lzx_view_tests_reject_non_extensibility_shapes() {
    let source = "experience customer
  view detail
    anchor @anchor.customer_detail

    tests
      allows when target.status = active
";
    let err = parse_lzx_document(source).unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("accepted by") && msg.contains("rejected by"),
        "expected guidance about closed catalog, got {msg}"
    );
}

/// Wave 4 — the live full-capsule fixture must still parse and its
/// `accepted by` / `rejected by` lines must lift to the new typed
/// shape (regression guard for the existing example).
#[test]
fn full_capsule_view_tests_round_trip() {
    let document = parse_lzx_document(include_str!(
        "../../../../../examples/full-capsule/full-capsule.lzx"
    ))
    .unwrap();
    let detail_view = document
        .experiences
        .iter()
        .flat_map(|e| e.views.iter())
        .find(|v| v.name == "detail")
        .expect("detail view present in fixture");
    // Sanity-check the assertion shape — the fixture has two
    // `accepted by` and one `rejected by` under the `detail` view.
    let accepted: Vec<&str> = detail_view
        .tests
        .iter()
        .filter_map(|t| match t {
            LzxViewTestAssertion::AcceptedBy { feature, .. } => Some(feature.as_str()),
            _ => None,
        })
        .collect();
    let rejected: Vec<&str> = detail_view
        .tests
        .iter()
        .filter_map(|t| match t {
            LzxViewTestAssertion::RejectedBy { feature, .. } => Some(feature.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(accepted, vec!["customer_tags", "customer_import"]);
    assert_eq!(rejected, vec!["billing"]);
}

#[test]
fn parses_lzx_experience_and_platform_surface() {
    let experience =
        parse_lzx_document(include_str!("../../../../../examples/customer-capsule.lzx"))
            .unwrap();
    assert_eq!(experience.experiences.len(), 1);
    assert_eq!(experience.experiences[0].name, "customer");
    assert_eq!(experience.experiences[0].imports, vec!["customer"]);
    assert_eq!(experience.experiences[0].views[0].name, "list");
    assert_eq!(
        experience.experiences[0].views[0].source.as_deref(),
        Some("customer.query.list")
    );
    assert_eq!(experience.experiences[0].views[1].anchor.as_deref(), None);

    let surface = parse_lzx_document(include_str!(
        "../../../../../examples/customer-capsule.web.lzx"
    ))
    .unwrap();
    assert_eq!(surface.surfaces.len(), 1);
    assert_eq!(surface.surfaces[0].experience, "customer");
    assert_eq!(surface.surfaces[0].platform, LzxPlatform::Web);
    assert_eq!(
        surface.surfaces[0].uses_experience.as_deref(),
        Some("customer")
    );
    assert_eq!(surface.surfaces[0].audiences[0].name, "admin");
    assert_eq!(
        surface.surfaces[0].audiences[0].views[0].columns,
        vec!["name", "email", "status", "created_at"]
    );
    assert_eq!(
        surface.surfaces[0].audiences[0].views[0].filter,
        vec!["status"]
    );
    assert_eq!(
        surface.surfaces[0].audiences[0].views[0].cells,
        vec!["status @client.status_cell"]
    );
}

#[test]
fn parses_lzx_app_manifest_and_routes() {
    let source = r#"
app AcmeCRM
  title "Acme CRM"
  version "0.1.0"
  targets
    backend go
    web react
    mobile expo
  default_locale "pt-BR"
  default_timezone "America/Sao_Paulo"
  auth_failed_redirect public.login
  not_found public.not_found
  error_page 404
    template "./views/404.tmpl"
    audience public
  error_page 500
    template "./views/500.tmpl"
  uses customer, customer_auth

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
  lazy true
"#;

    let document = parse_lzx_document(source).unwrap();
    let app = document.app.as_ref().unwrap();

    assert_eq!(app.name, "AcmeCRM");
    assert_eq!(app.title.as_deref(), Some("Acme CRM"));
    assert_eq!(app.targets, vec!["backend go", "web react", "mobile expo"]);
    assert_eq!(app.error_pages.len(), 2);
    assert_eq!(app.error_pages[0].status, 404);
    assert_eq!(app.error_pages[0].template, "./views/404.tmpl");
    assert_eq!(app.error_pages[0].audience.as_deref(), Some("public"));
    assert_eq!(app.error_pages[1].status, 500);
    assert_eq!(app.error_pages[1].template, "./views/500.tmpl");
    assert_eq!(app.error_pages[1].audience, None);
    assert_eq!(app.uses, vec!["customer", "customer_auth"]);
    assert_eq!(document.routes.len(), 1);
    assert_eq!(document.routes[0].path.as_deref(), Some("/customers/:id"));
    // ir+codegen(ts) §2.1 typed route_params landed (commit fe4d3a1c):
    // `route id: Customer.ID` now lifts to `route_params`, not `routes`.
    assert_eq!(document.routes[0].routes, Vec::<String>::new());
    assert_eq!(document.routes[0].route_params.len(), 1);
    assert_eq!(document.routes[0].route_params[0].name, "id");
    assert_eq!(
        document.routes[0].to.as_deref(),
        Some("customer.view.detail(id: route.id)")
    );
    assert_eq!(document.routes[0].lazy, Some(true));
}

#[test]
fn parses_lzx_route_guard_clauses() {
    let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton

route admin_home
  path "/admin"
  to customer.view.list
  surface customer web
  audience admin
  policy @policy.admin_only
    on_unauthenticated redirect "/sign-in"

experience customer
  view list
    policy @policy.admin_only
      on_unauthorized redirect "/"
    source customer.query.list

surface customer web
  uses experience customer

  audience admin
    policy @policy.admin_only
      on_unauthenticated redirect "/sign-in"
    view list Table
      policy @policy.admin_only
        on_unauthorized redirect "/"
      columns name
"#;

    let document = parse_lzx_document(source).unwrap();
    let app = document.app.as_ref().unwrap();
    let defaults = app.route_guard.as_ref().unwrap();
    assert_eq!(app.actor_query.as_deref(), Some("account.query.me"));
    assert_eq!(
        defaults.default_policy.as_deref(),
        Some("@scope.authenticated")
    );
    assert_eq!(defaults.on_unauthenticated.as_deref(), Some("/sign-in"));
    assert_eq!(defaults.on_unauthorized.as_deref(), Some("/403"));
    assert_eq!(
        defaults.skeleton.as_deref(),
        Some("@client.route_guard_skeleton")
    );

    let route_guard = document.routes[0].guard.as_ref().unwrap();
    assert_eq!(route_guard.policy, vec!["@policy.admin_only"]);
    assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));

    let experience_guard = document.experiences[0].views[0].guard.as_ref().unwrap();
    assert_eq!(experience_guard.policy, vec!["@policy.admin_only"]);
    assert_eq!(experience_guard.on_unauthorized.as_deref(), Some("/"));

    let audience = &document.surfaces[0].audiences[0];
    assert_eq!(
        audience
            .guard
            .as_ref()
            .and_then(|guard| guard.on_unauthenticated.as_deref()),
        Some("/sign-in")
    );
    assert_eq!(
        audience.views[0]
            .guard
            .as_ref()
            .and_then(|guard| guard.on_unauthorized.as_deref()),
        Some("/")
    );
}

#[test]
fn parses_lzx_lifecycle_substep_on_view_and_resume_arm() {
    let source = r#"
experience host
  imports host

  view phone_verification
    policy @policy.host_only
    requires_lifecycle Host = basic_details_pending substep phone_verification
    on_lifecycle_pending @resume host_onboarding
    source host.query.lookup.my_host

  resume host_onboarding
    source query.lookup my_host
    none -> view phone_verification
    basic_details_pending substep phone_verification -> view phone_verification
    * -> view phone_verification
"#;

    let document = parse_lzx_document(source).expect("parses");
    let view = &document.experiences[0].views[0];
    let requires = view
        .guard
        .as_ref()
        .and_then(|guard| guard.requires_lifecycle.as_ref())
        .expect("requires_lifecycle");
    assert_eq!(requires.state, "basic_details_pending");
    assert_eq!(requires.substep.as_deref(), Some("phone_verification"));

    let arm = &document.experiences[0].resume_routers[0].arms[1];
    assert_eq!(
        arm.kind,
        crate::LzxResumeArmKind::State("basic_details_pending".to_string())
    );
    assert_eq!(arm.substep.as_deref(), Some("phone_verification"));
}

#[test]
fn parses_lzx_audience_policy_single_and_list_rejects_trailing_comma() {
    let source = r#"
surface booking web
  audience host
    policy @policy.role.host
  audience signed_in
    policy [@policy.role.host, @policy.role.traveler]
"#;
    let document = parse_lzx_document(source).expect("parses");

    assert_eq!(
        document.surfaces[0].audiences[0]
            .guard
            .as_ref()
            .unwrap()
            .policy,
        vec!["@policy.role.host"]
    );
    assert_eq!(
        document.surfaces[0].audiences[1]
            .guard
            .as_ref()
            .unwrap()
            .policy,
        vec!["@policy.role.host", "@policy.role.traveler"]
    );

    let trailing = r#"
surface booking web
  audience signed_in
    policy [@policy.role.host,]
"#;
    let err = parse_lzx_document(trailing).unwrap_err();
    assert!(err.to_string().contains("empty entry"));
}

#[test]
fn parses_lzx_error_page_maintenance_status() {
    let source = r#"
app AcmeCRM
  error_page 503
    template "./views/maintenance.tmpl"
    audience public
"#;

    let document = parse_lzx_document(source).unwrap();
    let page = &document.app.as_ref().unwrap().error_pages[0];
    assert_eq!(page.status, 503);
    assert_eq!(page.template, "./views/maintenance.tmpl");
    assert_eq!(page.audience.as_deref(), Some("public"));
}

#[test]
fn rejects_lzx_error_page_without_template() {
    let source = r#"
app AcmeCRM
  error_page 404
    audience public
"#;

    let error = parse_lzx_document(source).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires a `template \"./...\"` declaration"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_lzx_partial_overrides() {
    let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      columns += score
"#;

    let error = parse_lzx_document(source).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("partial overrides are not valid in `.lzx`")
    );
}

#[test]
fn parses_lzx_view_anchor_child() {
    let source = r#"
experience customer
  imports customer

  view detail
    route id: Customer.ID
    anchor @anchor.customer_detail
    source customer.query.by_id(id: route.id)
"#;

    let document = parse_lzx_document(source).unwrap();

    assert_eq!(
        document.experiences[0].views[0].anchor.as_deref(),
        Some("@anchor.customer_detail")
    );
}

#[test]
fn parses_lzx_extension_slots_with_order() {
    let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    slot aside
      block @client.tag_editor
      platforms web, mobile
      audience admin, sales
    slot timeline after activity_timeline
      block @client.import_history
"#;

    let document = parse_lzx_document(source).unwrap();
    let extension = &document.experiences[0].extensions[0];

    assert_eq!(extension.anchor, "@anchor.customer_detail");
    assert!(extension.blocks.is_empty());
    assert_eq!(extension.slots.len(), 2);
    assert_eq!(extension.slots[0].name, "aside");
    assert_eq!(extension.slots[0].blocks, vec!["@client.tag_editor"]);
    assert_eq!(extension.slots[0].platforms, vec!["web", "mobile"]);
    assert_eq!(extension.slots[0].audiences, vec!["admin", "sales"]);
    assert_eq!(extension.slots[1].name, "timeline");
    assert_eq!(
        extension.slots[1]
            .order
            .as_ref()
            .map(|order| (order.relation.as_str(), order.target.as_str())),
            Some(("after", "activity_timeline"))
        );
}
