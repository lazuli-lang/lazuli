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
use crate::{LzxPlatform, LzxScalarLiteral, LzxViewTestAssertion};

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
        parse_lzx_document(include_str!("../../../../../examples/customer-capsule.lzx")).unwrap();
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

// =============================================================================
// ir-route-guard-escape-hatch-2026-05-28 §4.1 — escape-hatch surface tests.
// Cell A: parser must accept `requires_lifecycle_in`, the composed
// `forbid_when ... only_when lifecycle ...` shape, and the
// `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect "..."`
// row-field predicate.
// =============================================================================

#[test]
fn parses_lzx_requires_lifecycle_in_allow_list() {
    let source = r#"
route host_basic_details
  path "/onboarding/host/basic-details"
  policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    requires_lifecycle_in Host [basic_details_pending, address_pending, languages_pending]
"#;

    let document = parse_lzx_document(source).expect("parse");
    let guard = document.routes[0].guard.as_ref().expect("guard");
    let allow_list = guard
        .requires_lifecycle_in
        .as_ref()
        .expect("requires_lifecycle_in");
    assert_eq!(allow_list.resource, "Host");
    assert_eq!(
        allow_list.allowed_states,
        vec![
            "basic_details_pending",
            "address_pending",
            "languages_pending"
        ]
    );
    // Existing requires_lifecycle slot stays None.
    assert!(guard.requires_lifecycle.is_none());
}

#[test]
fn parses_lzx_requires_lifecycle_in_with_single_element_list() {
    // Single-element list is admissible — doctor flags the
    // canonical-form preference but parser accepts it.
    let source = r#"
route phone_verification
  path "/onboarding/phone"
  policy @policy.authenticated
    requires_lifecycle_in Host [phone_verification_pending]
"#;

    let document = parse_lzx_document(source).expect("parse");
    let allow_list = document.routes[0]
        .guard
        .as_ref()
        .and_then(|g| g.requires_lifecycle_in.as_ref())
        .expect("requires_lifecycle_in");
    assert_eq!(
        allow_list.allowed_states,
        vec!["phone_verification_pending"]
    );
}

#[test]
fn rejects_lzx_requires_lifecycle_in_with_lowercase_resource() {
    let source = r#"
route bad_resource
  path "/x"
  policy @policy.authenticated
    requires_lifecycle_in host [pending]
"#;
    let err = parse_lzx_document(source).expect_err("lowercase resource must fail");
    assert!(err.to_string().contains("requires_lifecycle_in"));
}

#[test]
fn parses_lzx_forbid_when_with_only_when_lifecycle() {
    let source = r#"
route choose_role
  path "/choose-role"
  policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    forbid_when @role.host dispatch_to "/host"
      only_when lifecycle Host = complete
    forbid_when @role.traveler dispatch_to "/explore"
      only_when lifecycle Traveler = complete
"#;

    let document = parse_lzx_document(source).expect("parse");
    let guard = document.routes[0].guard.as_ref().expect("guard");
    assert_eq!(guard.forbid_when.len(), 2);

    let host_fw = &guard.forbid_when[0];
    assert_eq!(host_fw.atom_ref, "@role.host");
    assert_eq!(host_fw.dispatch_to, "/host");
    let owl = host_fw
        .only_when_lifecycle
        .as_ref()
        .expect("host only_when_lifecycle");
    assert_eq!(owl.resource, "Host");
    assert_eq!(owl.state, "complete");

    let traveler_fw = &guard.forbid_when[1];
    let owl = traveler_fw
        .only_when_lifecycle
        .as_ref()
        .expect("traveler only_when_lifecycle");
    assert_eq!(owl.resource, "Traveler");
}

#[test]
fn parses_lzx_forbid_when_without_only_when_stays_back_compat() {
    let source = r#"
route choose_role
  path "/choose-role"
  policy @policy.authenticated
    forbid_when @role.host dispatch_to "/host"
"#;

    let document = parse_lzx_document(source).expect("parse");
    let fw = &document.routes[0]
        .guard
        .as_ref()
        .expect("guard")
        .forbid_when[0];
    assert!(fw.only_when_lifecycle.is_none());
    assert_eq!(fw.atom_ref, "@role.host");
}

#[test]
fn parses_lzx_requires_field_boolean_predicate() {
    let source = r#"
route host_address
  path "/onboarding/host/address"
  policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    requires user.lookup_my.is_phone_verified = true
      on_unmet redirect "/onboarding/host/phone-verification"
"#;

    let document = parse_lzx_document(source).expect("parse");
    let guard = document.routes[0].guard.as_ref().expect("guard");
    assert_eq!(guard.requires_field.len(), 1);
    let rf = &guard.requires_field[0];
    assert_eq!(rf.feature, "user");
    assert_eq!(rf.field, "is_phone_verified");
    assert_eq!(rf.expected, LzxScalarLiteral::Boolean(true));
    assert_eq!(rf.on_unmet_redirect, "/onboarding/host/phone-verification");
}

#[test]
fn parses_lzx_requires_field_string_literal() {
    let source = r#"
route preferred_locale
  path "/locale"
  policy @policy.authenticated
    requires traveler_profile.lookup_my.preferred_language = "pt-BR"
      on_unmet redirect "/locale"
"#;

    let document = parse_lzx_document(source).expect("parse");
    let rf = &document.routes[0]
        .guard
        .as_ref()
        .expect("guard")
        .requires_field[0];
    assert_eq!(rf.feature, "traveler_profile");
    assert_eq!(rf.field, "preferred_language");
    assert_eq!(rf.expected, LzxScalarLiteral::String("pt-BR".to_owned()));
}

#[test]
fn parses_lzx_requires_field_null_literal() {
    let source = r#"
route kyc_gate
  path "/kyc"
  policy @policy.authenticated
    requires user.lookup_my.kyc_passed_at = null
      on_unmet redirect "/onboarding/kyc"
"#;

    let document = parse_lzx_document(source).expect("parse");
    let rf = &document.routes[0]
        .guard
        .as_ref()
        .expect("guard")
        .requires_field[0];
    assert_eq!(rf.expected, LzxScalarLiteral::Null);
}

#[test]
fn rejects_lzx_requires_field_with_missing_lookup_my_literal() {
    // Per §4.1.1 — `lookup_my` is a parser-LITERAL, NOT a free
    // IDENT_LOWER. The LLM failure mode of skipping the segment
    // (`user.is_phone_verified`) MUST be structurally impossible.
    let source = r#"
route bad
  path "/bad"
  policy @policy.authenticated
    requires user.is_phone_verified = true
      on_unmet redirect "/x"
"#;
    let err = parse_lzx_document(source).expect_err("missing lookup_my must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("3 dot-separated segments") || msg.contains("lookup_my"),
        "diagnostic should name the path-segment grammar; got: {msg}"
    );
}

#[test]
fn rejects_lzx_requires_field_with_wrong_middle_segment() {
    let source = r#"
route bad
  path "/bad"
  policy @policy.authenticated
    requires user.lookup.is_phone_verified = true
      on_unmet redirect "/x"
"#;
    let err = parse_lzx_document(source).expect_err("wrong middle segment must fail");
    assert!(err.to_string().contains("lookup_my"));
}

// ── F5 — §7a surface UX primitives in the experience-surface dialect ──────────
// The abstract-experience `surface` dialect (parsed by `parse_lzx_document`,
// the dialect doctor runs) accepts the §7a view-level and audience-level UX
// primitives. Before this, these children were rejected with `LZX-PARSE`
// because the platform-view / audience catalogs were closed. They now parse
// into the shared surface-dialect `ViewUxAst` / `AudienceUxAst`.

#[test]
fn lzx_platform_view_accepts_view_level_ux_primitives() {
    let source = r#"
surface widget web
  uses experience widget

  audience admin
    policy @policy.read

    view list Table
      columns name, status
      view_mode
        table
        kanban
      tab_group derived_from status
        case OPEN, PENDING -> tab "Active"
        case CLOSED -> tab "Archived"
      view.inline_table on_change @command.update_row
      view.board activity
        lanes derived_from status

    view edit_form Form
      fields a, b
      wizard_steps 3 current step
      repeatable input installments group { days: Int; pct: Decimal } validates sum(pct) = 100
"#;
    let doc = parse_lzx_document(source).expect("§7a view-level primitives parse");
    let audience = &doc.surfaces[0].audiences[0];

    let list = &audience.views[0];
    assert_eq!(list.ux.view_modes, vec!["table", "kanban"]);
    let group = list.ux.tab_group.as_ref().expect("tab_group");
    assert_eq!(group.derived_from, "status");
    assert_eq!(group.cases.len(), 2);
    assert_eq!(group.cases[0].variants, vec!["OPEN", "PENDING"]);
    assert!(list.ux.inline_table.is_some());
    let board = list.ux.board.as_ref().expect("board");
    assert_eq!(board.name, "activity");
    assert_eq!(board.lanes_source, "status");

    let form = &audience.views[1];
    let steps = form.ux.wizard_steps.as_ref().expect("wizard_steps");
    assert_eq!(steps.total, 3);
    assert_eq!(steps.current_field, "step");
    assert_eq!(form.ux.repeatable_groups.len(), 1);
    assert_eq!(form.ux.repeatable_groups[0].sum_field, "pct");
}

#[test]
fn lzx_audience_accepts_tabs_and_wizard_containers() {
    let source = r#"
surface widget web
  uses experience widget

  audience admin
    view list Table
      columns name

    tabs
      tab "Details" -> view list
    wizard onboarding steps
      step 1: list
"#;
    let doc = parse_lzx_document(source).expect("§7a audience containers parse");
    let audience = &doc.surfaces[0].audiences[0];
    assert_eq!(audience.ux.tabs.len(), 1);
    assert_eq!(audience.ux.tabs[0].entries[0].view, "list");
    assert_eq!(audience.ux.wizards.len(), 1);
    assert_eq!(audience.ux.wizards[0].name, "onboarding");
    assert_eq!(audience.ux.wizards[0].steps[0].ref_name, "list");
}

// G-A1 — the typed `filters { … }` block (incl. `date_range`) now has an
// end-to-end home in the experience dialect. Previously it parsed only in
// the orphaned surface dialect; an experience-dialect platform view that
// declared it was rejected with `LZX-PARSE`. The block reuses the
// surface-dialect parser (`parse_filters_block_into`) and lands on
// `LzxPlatformView.filters` (`FilterDeclAst`). Strictly additive — the
// single-line `filter <list>` form is unchanged.
#[test]
fn lzx_platform_view_accepts_typed_filters_block() {
    use crate::FilterCardinalityAst;
    let source = r#"
surface widget web
  uses experience widget

  audience admin
    policy @policy.read

    view list Table
      columns name, status, created_at
      filter status, owner
      filters
        created: date_range
        status: single
        tags: list of Text from query
"#;
    let doc = parse_lzx_document(source).expect("typed filters block parses");
    let list = &doc.surfaces[0].audiences[0].views[0];

    // The single-line `filter` form is untouched.
    assert_eq!(list.filter, vec!["status", "owner"]);

    // The typed `filters` block lands on the new slot.
    assert_eq!(list.filters.len(), 3);
    assert_eq!(list.filters[0].name, "created");
    assert_eq!(list.filters[0].type_ref, "Date");
    assert_eq!(list.filters[0].cardinality, FilterCardinalityAst::DateRange);
    assert!(!list.filters[0].url_sync);

    assert_eq!(list.filters[1].name, "status");
    assert_eq!(list.filters[1].cardinality, FilterCardinalityAst::Single);

    assert_eq!(list.filters[2].name, "tags");
    assert_eq!(list.filters[2].cardinality, FilterCardinalityAst::Multi);
    assert!(list.filters[2].url_sync);
}

#[test]
fn lzx_platform_view_rejects_malformed_filters_block() {
    // Inline content on the block keyword is a hard error, mirroring the
    // surface dialect — the catalog expanded but the grammar stayed tight.
    let inline = r#"
surface widget web
  uses experience widget

  audience admin
    view list Table
      columns name
      filters status, owner
"#;
    let err = parse_lzx_document(inline).expect_err("inline filters content must fail");
    assert!(err.to_string().contains("block keyword"), "got: {err}");

    // An empty `filters` block is rejected by the reused surface parser.
    let empty = r#"
surface widget web
  uses experience widget

  audience admin
    view list Table
      columns name
      filters
      actions update
"#;
    let err = parse_lzx_document(empty).expect_err("empty filters block must fail");
    assert!(
        err.to_string().contains("requires at least one"),
        "got: {err}"
    );
}

#[test]
fn lzx_platform_view_still_rejects_unknown_child() {
    // Behavior-preserving: the catalog only EXPANDED — a genuinely
    // unknown child still hard-errors (no silent acceptance).
    let source = r#"
surface widget web
  uses experience widget

  audience admin
    view list Table
      columns name
      hologram_mode
"#;
    let err = parse_lzx_document(source).expect_err("unknown view child must fail");
    assert!(err.to_string().contains("platform view children are"));
}
