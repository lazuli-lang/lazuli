#[test]
fn tabs_view_ref_dangling_errors() {
    let tabs = AudienceUx {
        tabs: vec![Tabs {
            entries: vec![TabEntry {
                label: "Ghost".to_owned(),
                view: "missing".to_owned(),
                audience: None,
                span_ref: Some(SpanRef { start: 5, end: 6 }),
            }],
            span_ref: None,
        }],
        wizards: vec![],
    };
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![detail_view("detail", ViewUx::default())],
        tabs,
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, TAB_VIEW_REF_CODE);
    assert!(f[0].message.contains("missing"));
}

#[test]
fn wizard_step_ref_dangling_errors() {
    let ux = AudienceUx {
        tabs: vec![],
        wizards: vec![Wizard {
            name: "flow".to_owned(),
            steps: vec![WizardStep {
                index: 1,
                ref_name: "ghost_form".to_owned(),
                span_ref: Some(SpanRef { start: 7, end: 8 }),
            }],
            span_ref: None,
        }],
    };
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view("v", ViewUx::default())],
        ux,
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, TAB_VIEW_REF_CODE);
    assert!(f[0].message.contains("ghost_form"));
}

// ── LZX-VIEW-MODE-001 ──────────────────────────────────────────────────────

fn inline_table_ux(cmd: &str) -> ViewUx {
    ViewUx {
        inline_table: Some(InlineTable {
            on_change: CommandRef {
                feature: "item".to_owned(),
                name: cmd.to_owned(),
            },
            span_ref: Some(SpanRef { start: 30, end: 31 }),
        }),
        ..Default::default()
    }
}

fn create_command(name: &str) -> lazuli_ir::Command {
    lazuli_ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: lazuli_ir::CommandKind::Create,
        route: Vec::new(),
        input: lazuli_ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: lazuli_ir::CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        derived_from: None,
    }
}

#[test]
fn inline_table_known_command_is_clean() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view("v", inline_table_ux("update_row"))],
        AudienceUx::default(),
        vec![create_command("update_row")],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn inline_table_unknown_command_errors() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view("v", inline_table_ux("update_row"))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, VIEW_MODE_CODE);
    assert!(f[0].message.contains("update_row"));
}

// ── LZX-BOARD-LANES-001 (GAP-UX-05) ────────────────────────────────────────

fn many_field(name: &str) -> Field {
    field(
        name,
        TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Id))),
    )
}

fn board_ux(lanes_source: &str) -> ViewUx {
    ViewUx {
        board: Some(Board {
            name: "activity_board".to_owned(),
            lanes_source: lanes_source.to_owned(),
            span_ref: Some(SpanRef { start: 40, end: 42 }),
        }),
        ..Default::default()
    }
}

#[test]
fn board_lanes_enum_field_is_clean() {
    let m = module(
        vec![enum_field("status", "Status")],
        vec![enum_decl("Status", &["OPEN", "DONE"])],
        vec![list_view("v", board_ux("status"))],
        AudienceUx::default(),
        vec![],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn board_lanes_has_many_relation_is_clean() {
    let m = module(
        vec![many_field("tasks")],
        vec![],
        vec![list_view("v", board_ux("tasks"))],
        AudienceUx::default(),
        vec![],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn board_lanes_non_enum_non_relation_field_errors() {
    let m = module(
        vec![text_field("status")],
        vec![],
        vec![list_view("v", board_ux("status"))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, BOARD_LANES_CODE);
    assert_eq!(f[0].severity, Severity::Error);
    assert!(f[0].message.contains("status"));
}

#[test]
fn board_lanes_unknown_field_errors() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view("v", board_ux("ghost"))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, BOARD_LANES_CODE);
}

// ── LZX-REPEATABLE-SUM-001 (GAP-UX-05) ─────────────────────────────────────

fn repeatable_ux(sum_field: &str, fields: &[(&str, &str)]) -> ViewUx {
    ViewUx {
        repeatable_groups: vec![RepeatableGroup {
            name: "installments".to_owned(),
            fields: fields
                .iter()
                .map(|(n, t)| RepeatableField {
                    name: (*n).to_owned(),
                    type_name: (*t).to_owned(),
                })
                .collect(),
            sum_field: sum_field.to_owned(),
            sum_target: "100".to_owned(),
            span_ref: Some(SpanRef { start: 50, end: 51 }),
        }],
        ..Default::default()
    }
}

#[test]
fn repeatable_sum_numeric_field_is_clean() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view(
            "v",
            repeatable_ux("percentage", &[("days", "Int"), ("percentage", "Decimal")]),
        )],
        AudienceUx::default(),
        vec![],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn repeatable_sum_unknown_field_errors() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view(
            "v",
            repeatable_ux("ghost", &[("percentage", "Decimal")]),
        )],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, REPEATABLE_SUM_CODE);
    assert!(f[0].message.contains("not a field declared in the group"));
}

#[test]
fn repeatable_sum_non_numeric_field_errors() {
    let m = module(
        vec![text_field("title")],
        vec![],
        vec![list_view("v", repeatable_ux("label", &[("label", "Text")]))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, REPEATABLE_SUM_CODE);
    assert_eq!(f[0].severity, Severity::Error);
    assert!(f[0].message.contains("non-numeric"));
}

// ── LZX-BOARD-LANES-001 — FP2: multi-resource source-query resolution ───────

/// Build a resource with an explicit name + fields.
fn named_resource(name: &str, fields: Vec<Field>) -> Resource {
    let mut r = resource(fields);
    r.name = name.to_owned();
    r
}

/// Build a `query.list <name>` (only its name matters for resolution).
fn named_list_query(name: &str) -> lazuli_ir::Query {
    let mut q = list_query();
    if let lazuli_ir::Query::List(ref mut lq) = q {
        lq.name = name.to_owned();
    }
    q
}

/// Build a list view whose `source` names a specific query on the `item`
/// feature.
fn list_view_sourcing(view_name: &str, query_name: &str, ux: ViewUx) -> View {
    let mut v = list_view(view_name, ux);
    if let View::List(ref mut vl) = v {
        vl.source = QueryRef {
            feature: "item".to_owned(),
            kind: QueryKind::List,
            name: query_name.to_owned(),
        };
    }
    v
}

/// A single-feature module with TWO resources, one list query, and a board
/// view — mirrors pauta `job_steps_activities` (JobStep + Activity, view
/// `activity_board` sourcing `list_job_steps`).
fn two_resource_module(
    resources: Vec<Resource>,
    queries: Vec<lazuli_ir::Query>,
    views: Vec<View>,
) -> Module {
    // Start from the canonical single-resource module then swap in the
    // multi-resource shape so every other field stays default.
    let mut m = module(vec![], vec![], views, AudienceUx::default(), vec![]);
    let feature = &mut m.features[0];
    feature.resources = resources;
    feature.queries = queries;
    m
}

/// FP2 — a board in a MULTI-resource feature whose `derived_from <relation>`
/// exists on the SOURCE-QUERY's resolved resource must NOT fire. The view
/// `activity_board` sources `list_job_steps` → `JobStep`; `JobStep` declares
/// `has_many activities` (`TypeRef::Many`); `lanes derived_from activities`
/// is valid. Before the fix `resolve_resource` bailed (returned `None`) on
/// any 2-resource feature, firing spuriously.
#[test]
fn board_lanes_multi_resource_has_many_relation_is_clean() {
    let job_step = named_resource("JobStep", vec![many_field("activities")]);
    let activity = named_resource("Activity", vec![text_field("title")]);
    let m = two_resource_module(
        vec![job_step, activity],
        vec![named_list_query("list_job_steps")],
        vec![list_view_sourcing(
            "activity_board",
            "list_job_steps",
            board_ux("activities"),
        )],
    );
    assert!(
        check(&m).is_empty(),
        "board derived_from a has_many on the correctly-resolved source resource must not fire: {:?}",
        check(&m)
    );
}

/// FP2 complement — a board in the SAME multi-resource feature whose lane
/// source is genuinely NOT a relation/field on the resolved resource STILL
/// fires. `list_job_steps` resolves to `JobStep`; `ghost` is not a field on
/// `JobStep` (it only exists, as a plain text field, on `Activity`), so the
/// rule must report it. Proves the resolution is precise, not a blanket
/// suppression.
#[test]
fn board_lanes_multi_resource_bad_lane_source_still_fires() {
    let job_step = named_resource("JobStep", vec![many_field("activities")]);
    let activity = named_resource("Activity", vec![text_field("ghost")]);
    let m = two_resource_module(
        vec![job_step, activity],
        vec![named_list_query("list_job_steps")],
        vec![list_view_sourcing(
            "activity_board",
            "list_job_steps",
            board_ux("ghost"),
        )],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1, "bad lane source must fire, got {:?}", f);
    assert_eq!(f[0].code, BOARD_LANES_CODE);
    assert!(f[0].message.contains("ghost"));
}

/// FP2 (verbatim pauta shape) — the experience→web projection lowers a board
/// view with a SOURCELESS `query.list` ref (empty `name`) into a MULTI-resource
/// feature. The rule cannot resolve which resource backs the board, so it must
/// SKIP rather than fire: "can't validate" is not "invalid". This is the exact
/// pauta `activity_board` over-block (2 of the 10 blockers).
#[test]
fn board_lanes_multi_resource_sourceless_ref_is_skipped() {
    let job_step = named_resource("JobStep", vec![many_field("activities")]);
    let activity = named_resource("Activity", vec![text_field("title")]);
    let m = two_resource_module(
        vec![job_step, activity],
        vec![named_list_query("list_job_steps")],
        // Empty query name = the synthetic sourceless experience→web ref.
        vec![list_view_sourcing("activity_board", "", board_ux("activities"))],
    );
    assert!(
        check(&m).is_empty(),
        "a board on a sourceless multi-resource view must be skipped, not flagged: {:?}",
        check(&m)
    );
}

/// FP2 guard — a SINGLE-resource feature with a sourceless ref STILL resolves
/// (one resource is unambiguous), so a genuinely-bad lane source there still
/// fires. Proves the skip is scoped to the unresolvable multi-resource case.
#[test]
fn board_lanes_single_resource_sourceless_bad_lane_still_fires() {
    let m = two_resource_module(
        vec![named_resource("Item", vec![text_field("title")])],
        vec![named_list_query("list_items")],
        vec![list_view_sourcing("board_view", "", board_ux("ghost"))],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1, "single-resource bad lane must fire, got {:?}", f);
    assert_eq!(f[0].code, BOARD_LANES_CODE);
}
