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
