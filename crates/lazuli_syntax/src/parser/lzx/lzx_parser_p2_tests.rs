// =============================================================================
// ir-route-guard-escape-hatch-2026-05-28 §4.1 — escape-hatch surface tests.
// Cell A: parser must accept `requires_lifecycle_in`, the composed
// `forbid_when ... only_when lifecycle ...` shape, and the
// `requires <feature>.lookup_my.<field> == <literal> on_unmet redirect "..."`
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
    requires user.lookup_my.is_phone_verified == true
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
    requires traveler_profile.lookup_my.preferred_language == "pt-BR"
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
    requires user.lookup_my.kyc_passed_at == null
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
    requires user.is_phone_verified == true
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
      repeatable input installments group days: Integer, pct: Decimal validates sum(pct) = 100
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
