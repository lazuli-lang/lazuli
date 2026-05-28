//! Tests for the Wave-W6 surface UX doctor rules (`ux_rules.rs`).

use lazuli_ir::{
    AudienceUx, Board, BuiltinType, CommandRef, Defaults, EnumDecl, EnumVariant, FieldConstraints,
    InlineTable, ListQuery, ListRender, Policies, PolicyRef, QualifiedName, QueryKind, QueryRef,
    RepeatableField, RepeatableGroup, Resource, SpanRef, Surface, SurfaceTarget, TabEntry,
    TabGroup, TabGroupCase, Tabs, TypeRef, ViewDetail, ViewList, ViewUx, Wizard, WizardStep,
    WizardSteps,
};

use super::*;

fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

fn enum_variant(name: &str) -> EnumVariant {
    EnumVariant {
        name: name.to_owned(),
        storage_value: None,
        label_key: None,
        hint_key: None,
        icon_key: None,
        previous_names: Vec::new(),
    }
}

fn enum_decl(name: &str, variants: &[&str]) -> EnumDecl {
    EnumDecl {
        name: name.to_owned(),
        public_contract: None,
        variants: variants.iter().map(|v| enum_variant(v)).collect(),
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn enum_field(name: &str, enum_name: &str) -> Field {
    field(name, TypeRef::EnumRef(qname(enum_name)))
}

fn text_field(name: &str) -> Field {
    field(name, TypeRef::Builtin(BuiltinType::Text))
}

fn field(name: &str, type_ref: TypeRef) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

fn resource(fields: Vec<Field>) -> Resource {
    Resource {
        name: "Item".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        append_only: false,
        invariants: vec![],
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
    }
}

fn list_query() -> lazuli_ir::Query {
    lazuli_ir::Query::List(ListQuery {
        name: "search".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

fn source() -> QueryRef {
    QueryRef {
        feature: "item".to_owned(),
        kind: QueryKind::List,
        name: "search".to_owned(),
    }
}

fn list_view(name: &str, ux: ViewUx) -> View {
    View::List(ViewList {
        name: name.to_owned(),
        route: Some("/".to_owned()),
        source: source(),
        render: ListRender::Table {
            columns: vec!["title".to_owned()],
        },
        search: None,
        filter: vec![],
        cells: Vec::new(),
        actions: Vec::new(),
        drawer: None,
        sort: None,
        selection: None,
        settings: Vec::new(),
        redacted_fields: Vec::new(),
        ux,
        span_ref: None,
    })
}

fn detail_view(name: &str, ux: ViewUx) -> View {
    View::Detail(ViewDetail {
        name: name.to_owned(),
        route: Some("/d".to_owned()),
        source: source(),
        route_params: Vec::new(),
        sections: Vec::new(),
        cells: Vec::new(),
        actions: Vec::new(),
        redacted_fields: Vec::new(),
        ux,
        span_ref: None,
    })
}

/// Build a single-feature module: one resource, one list query, one audience
/// holding `views`, plus the audience-level `audience_ux` and feature enums.
fn module(
    fields: Vec<Field>,
    enums: Vec<EnumDecl>,
    views: Vec<View>,
    audience_ux: AudienceUx,
    commands: Vec<lazuli_ir::Command>,
) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![Feature {
            name: "item".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums,
            resources: vec![resource(fields)],
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands,
            apis: Vec::new(),
            records: Vec::new(),
            queries: vec![list_query()],
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![Surface {
                feature: "item".to_owned(),
                target: SurfaceTarget::Web,
                audiences: vec![Audience {
                    name: "admin".to_owned(),
                    requires: Vec::new(),
                    views,
                    ux: audience_ux,
                    span_ref: None,
                }],
                span_ref: None,
            }],
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }],
    }
}

fn wizard_steps_ux(total: u32, field: &str) -> ViewUx {
    ViewUx {
        wizard_steps: Some(WizardSteps {
            total,
            current_field: field.to_owned(),
            span_ref: Some(SpanRef { start: 10, end: 12 }),
        }),
        ..Default::default()
    }
}

// ── LZX-WIZARD-STEPS-EXPR-001 ──────────────────────────────────────────────

#[test]
fn wizard_steps_enum_field_match_is_clean() {
    let m = module(
        vec![enum_field("step", "Step")],
        vec![enum_decl("Step", &["A", "B", "C"])],
        vec![detail_view("v", wizard_steps_ux(3, "step"))],
        AudienceUx::default(),
        vec![],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn wizard_steps_non_enum_field_errors() {
    let m = module(
        vec![text_field("step")],
        vec![],
        vec![detail_view("v", wizard_steps_ux(3, "step"))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].code, WIZARD_STEPS_CODE);
    assert_eq!(f[0].severity, Severity::Error);
}

#[test]
fn wizard_steps_variant_count_mismatch_warns() {
    let m = module(
        vec![enum_field("step", "Step")],
        vec![enum_decl("Step", &["A", "B"])],
        vec![detail_view("v", wizard_steps_ux(3, "step"))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warn);
    assert!(f[0].message.contains("variant count 2"));
}

// ── LZX-TAB-GROUP-CASE-001 ─────────────────────────────────────────────────

fn tab_group_ux(field: &str, cases: Vec<(&[&str], &str)>) -> ViewUx {
    ViewUx {
        tab_group: Some(TabGroup {
            derived_from: field.to_owned(),
            cases: cases
                .into_iter()
                .map(|(vs, label)| TabGroupCase {
                    variants: vs.iter().map(|s| s.to_string()).collect(),
                    label: label.to_owned(),
                    span_ref: Some(SpanRef { start: 20, end: 22 }),
                })
                .collect(),
            span_ref: Some(SpanRef { start: 18, end: 19 }),
        }),
        ..Default::default()
    }
}

#[test]
fn tab_group_exhaustive_is_clean() {
    let m = module(
        vec![enum_field("kind", "Kind")],
        vec![enum_decl("Kind", &["TV", "RADIO", "PRINT"])],
        vec![list_view(
            "v",
            tab_group_ux(
                "kind",
                vec![(&["TV", "RADIO"], "Broadcast"), (&["PRINT"], "Print")],
            ),
        )],
        AudienceUx::default(),
        vec![],
    );
    assert!(check(&m).is_empty());
}

#[test]
fn tab_group_unknown_variant_errors() {
    let m = module(
        vec![enum_field("kind", "Kind")],
        vec![enum_decl("Kind", &["TV", "RADIO"])],
        vec![list_view(
            "v",
            tab_group_ux("kind", vec![(&["GHOST"], "Nope")]),
        )],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert!(f.iter().any(|x| x.code == TAB_GROUP_CASE_CODE
        && x.severity == Severity::Error
        && x.message.contains("GHOST")));
}

#[test]
fn tab_group_non_exhaustive_warns() {
    let m = module(
        vec![enum_field("kind", "Kind")],
        vec![enum_decl("Kind", &["TV", "RADIO", "PRINT"])],
        vec![list_view(
            "v",
            tab_group_ux("kind", vec![(&["TV", "RADIO"], "Broadcast")]),
        )],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert!(f.iter().any(|x| x.code == TAB_GROUP_CASE_CODE
        && x.severity == Severity::Warn
        && x.message.contains("PRINT")));
}

#[test]
fn tab_group_non_enum_field_errors() {
    let m = module(
        vec![text_field("kind")],
        vec![],
        vec![list_view("v", tab_group_ux("kind", vec![(&["TV"], "B")]))],
        AudienceUx::default(),
        vec![],
    );
    let f = check(&m);
    assert!(f.iter().any(|x| x.code == TAB_GROUP_CASE_CODE
        && x.severity == Severity::Error
        && x.message.contains("derived_from")));
}

// ── LZX-TAB-VIEW-REF-001 ───────────────────────────────────────────────────

#[test]
fn tabs_view_ref_resolves() {
    let tabs = AudienceUx {
        tabs: vec![Tabs {
            entries: vec![TabEntry {
                label: "Details".to_owned(),
                view: "detail".to_owned(),
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
    assert!(check(&m).is_empty());
}

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
    field(name, TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Id))))
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
        vec![list_view("v", repeatable_ux("ghost", &[("percentage", "Decimal")]))],
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
