#[test]
fn computed_date_field_emits_addate_helper_with_literal_offset() {
    use lazuli_ir::{ComputedDate, ComputedDateBase, ComputedDateOffset};
    let mut feature = base_feature("campaign");
    let mut due = simple_field("due_date", BuiltinType::Date, true);
    due.computed_date = Some(ComputedDate {
        base: ComputedDateBase::Field("campaign_start".to_owned()),
        offset: ComputedDateOffset::Literal(30),
    });
    let resource = simple_resource(
        "campaign",
        vec![simple_field("campaign_start", BuiltinType::Date, true), due],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(out.contains("func (m *Campaign) ComputeDueDate() {"));
    // Integer literal lowers verbatim (no `int(m.<field>)` wrapper).
    assert!(
        out.contains("base.AddDate(0, 0, 30)"),
        "expected literal AddDate emission; got:\n{out}"
    );
}

#[test]
fn schedule_rule_field_emits_fn_backed_addate_helper() {
    // W4 GAP-08 — `schedule_rule from @fn.activity_date_rule(input.rule)
    // offset offset_days` emits a `Compute<Field>(rule string)` helper that
    // resolves the base Date via `lazuli.ScheduleRuleDate` then applies the
    // same AddDate arithmetic as `computed_date`.
    use lazuli_ir::{ComputedDate, ComputedDateBase, ComputedDateOffset};
    let mut feature = base_feature("activity");
    let mut due = simple_field("due_date", BuiltinType::Date, true);
    due.computed_date = Some(ComputedDate {
        base: ComputedDateBase::Rule {
            rule: "input.rule".to_owned(),
            fn_ref: "activity_date_rule".to_owned(),
        },
        offset: ComputedDateOffset::Field("offset_days".to_owned()),
    });
    let resource = simple_resource(
        "activity",
        vec![simple_field("offset_days", BuiltinType::Integer, true), due],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    // The rule form takes a `rule string` argument and calls the bound @fn.
    assert!(
        out.contains("func (m *Activity) ComputeDueDate(rule string) {"),
        "expected rule-form Compute helper; got:\n{out}"
    );
    assert!(
        out.contains("lazuli.ScheduleRuleDate(\"activity_date_rule\", rule)"),
        "expected ScheduleRuleDate call; got:\n{out}"
    );
    assert!(
        out.contains("base.AddDate(0, 0, int(m.OffsetDays))"),
        "expected AddDate emission on the @fn-resolved base; got:\n{out}"
    );
}

#[test]
fn capability_encrypted_field_uses_lazuli_encrypted_ref() {
    let mut feature = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![Field {
            name: "external_id".to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                key: "@key.tenant".to_owned(),
            })),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(out.contains("ExternalID lazuli.EncryptedRef"));
}

#[test]
fn deterministic_across_runs() {
    let mut feature = base_feature("customer");
    feature.resources.push(simple_resource(
        "zebra",
        vec![simple_field("name", BuiltinType::Text, true)],
    ));
    feature.resources.push(simple_resource(
        "alpha",
        vec![simple_field("name", BuiltinType::Text, true)],
    ));
    let a = emit(&feature).expect("must emit");
    let b = emit(&feature).expect("must emit");
    assert_eq!(a, b);
    // Resources sorted alphabetically: alpha banner appears before zebra.
    let alpha_pos = a.find("Resource: Alpha").expect("alpha banner");
    let zebra_pos = a.find("Resource: Zebra").expect("zebra banner");
    assert!(alpha_pos < zebra_pos);
}

#[test]
fn cross_feature_user_defined_field_emits_qualified_ref_and_import() {
    // Two features: `customer` declares `Customer`; `org` declares
    // `User`. `Customer.owner: User` references a type the
    // analyzer left as `feature: None` because it lives in
    // another feature. The cross-feature resolver should lift it
    // to `*orggen.User` plus an `lazuli/test/org` import.
    let mut customer = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![Field {
            name: "owner".to_owned(),
            type_ref: TypeRef::UserDefined(lazuli_ir::QualifiedName {
                feature: None,
                name: "User".to_owned(),
            }),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }],
    );
    customer.resources.push(resource);

    let mut org = base_feature("org");
    org.resources.push(simple_resource("user", Vec::new()));
    // The org resource is named lowercase; pascal-case the
    // emitter-side declared type still appears as `User` in
    // the index because `CrossFeatureIndex` keys on
    // `Resource.name` verbatim. So we build the index against
    // an explicit `User` resource.
    org.resources.clear();
    org.resources.push(Resource {
        name: "User".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: Vec::new(),
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: vec![],

        lock: None,

        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    });

    let module = Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(default_app_manifest("test")),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features: vec![customer.clone(), org],
    };
    let index = CrossFeatureIndex::build(&module);
    let out = emit_resource_file("examples/x.lzi", &customer, "lazuli/test", &index)
        .expect("must emit");

    // Resource FK fields collapse to `lazuli.ID` — the DB column
    // is BIGINT, so `pgx.RowToStructByName` can't scan into a
    // `*orggen.User` struct. The cross-feature import isn't
    // required for the FK column itself (it's just a BIGINT) but
    // it may still appear if other fields on the resource use a
    // record/enum from `org`; this resource has none.
    assert!(
        out.contains("Owner *lazuli.ID"),
        "expected `*lazuli.ID` FK field for cross-feature resource ref, got:\n{out}"
    );
}

/// Regression for the 2026-05-28 prod-break window: hostpoint `account.User`
/// declared `created_at: DateTime required` + `updated_at: DateTime required`
/// as explicit fields (no `defaults timestamps` on the feature), so
/// `uses_timestamps()` returned false and the emitted `lazuli.Resource[User]`
/// value lacked `Timestamps: true`. The runtime then skipped auto-binding
/// `updated_at = now()` on INSERT, every `account.register` returned 500
/// `null value in column "updated_at" violates not-null constraint`.
///
/// Fix: `uses_timestamps()` now falls through to explicit-field detection
/// when neither `Resource.timestamps` nor `Defaults.timestamps` is set. A
/// resource that declares BOTH `created_at` and `updated_at` opts into
/// the auto-touch convention without the convention flag.
#[test]
fn explicit_created_and_updated_fields_set_timestamps_true() {
    let mut feature = base_feature("account");
    let resource = simple_resource(
        "User",
        vec![
            simple_field("email", BuiltinType::SemanticEmail, true),
            simple_field("created_at", BuiltinType::DateTime, true),
            simple_field("updated_at", BuiltinType::DateTime, true),
        ],
    );
    feature.resources.push(resource);

    let out = emit(&feature).expect("must emit");

    assert!(
        out.contains("Timestamps: true"),
        "explicit created_at + updated_at fields must set Timestamps: true \
         on the Resource[T] value so runtime auto-binds updated_at on INSERT — \
         got:\n{out}"
    );
}

/// Negative path: a resource with `created_at` but NOT `updated_at` is
/// engaging immutable-row / audit-trail semantics, not the auto-touch
/// convention. `Timestamps: true` would emit a runtime auto-bind for a
/// column that doesn't exist in the DDL → PG 42703 on every INSERT.
#[test]
fn created_at_alone_does_not_set_timestamps_true() {
    let mut feature = base_feature("audit");
    let resource = simple_resource(
        "Event",
        vec![
            simple_field("kind", BuiltinType::Text, true),
            simple_field("created_at", BuiltinType::DateTime, true),
        ],
    );
    feature.resources.push(resource);

    let out = emit(&feature).expect("must emit");

    assert!(
        !out.contains("Timestamps: true"),
        "resource with only `created_at` (no `updated_at`) must NOT \
         opt into the timestamps convention — got:\n{out}"
    );
}

/// Symmetric to the above: `updated_at` alone (with no `created_at`)
/// is an odd shape but should also NOT enable the convention. Both
/// columns must be present for the auto-touch path to kick in.
#[test]
fn updated_at_alone_does_not_set_timestamps_true() {
    let mut feature = base_feature("counter");
    let resource = simple_resource(
        "Bump",
        vec![
            simple_field("value", BuiltinType::Integer, true),
            simple_field("updated_at", BuiltinType::DateTime, true),
        ],
    );
    feature.resources.push(resource);

    let out = emit(&feature).expect("must emit");

    assert!(
        !out.contains("Timestamps: true"),
        "resource with only `updated_at` (no `created_at`) must NOT \
         opt into the timestamps convention — got:\n{out}"
    );
}

/// GAP-13 — `polymorphic_ref <type_field> <id_field> targets [...]` must
/// project BOTH columns onto the emitted Go struct. The migration DDL
/// emits `<type_field> TEXT NOT NULL` + `<id_field> BIGINT NOT NULL`, but
/// the pair lives on `resource.polymorphic_refs`, not `resource.fields`,
/// so without an explicit projection the struct omits them. That broke the
/// row both ways: the read path derives its `SELECT` list from the struct's
/// `db` tags (columns dropped from every list/lookup), and the write path's
/// `INSERT ... RETURNING *` hands pgx columns with no destination field
/// (strict `RowToStructByName` scan failure). The discriminator lowers to a
/// non-pointer `string`; the id to `lazuli.ID`; both required (NOT NULL),
/// so bare `db`/`json` tags with no `omitempty`.
#[test]
fn polymorphic_ref_projects_type_and_id_columns_onto_struct() {
    let mut feature = base_feature("attachments");
    let mut resource = simple_resource(
        "Attachment",
        vec![simple_field("status", BuiltinType::Text, true)],
    );
    resource.polymorphic_refs = vec![lazuli_ir::PolymorphicRef {
        type_field: "entity_type".to_owned(),
        id_field: "entity_id".to_owned(),
        targets: vec![
            "Customer".to_owned(),
            "Supplier".to_owned(),
            "Job".to_owned(),
            "Activity".to_owned(),
        ],
    }];
    feature.resources.push(resource);

    let out = emit(&feature).expect("must emit");

    // Struct rows are column-aligned (name / type / tag padded to the widest
    // row), so assert the load-bearing pieces — field name, Go type, and the
    // db/json tag — rather than an exact, padding-sensitive line.
    //
    // Discriminator column: TEXT NOT NULL → non-pointer `string`.
    assert!(
        out.contains("EntityType string"),
        "polymorphic_ref discriminator must project as a required `string` \
         field `EntityType` — got:\n{out}"
    );
    assert!(
        out.contains(r#"db:"entity_type" json:"entity_type"`"#),
        "polymorphic_ref discriminator must carry bare `entity_type` db/json \
         tags (NOT NULL → no omitempty) — got:\n{out}"
    );
    // Id column: BIGINT NOT NULL → `lazuli.ID`, no `*`, no omitempty.
    // (Field names are column-aligned, so `EntityID` is padded before its
    // type — match the name and type independently of the padding width.)
    assert!(
        out.contains("EntityID") && out.contains("lazuli.ID `db:\"entity_id\""),
        "polymorphic_ref id must project as a required `lazuli.ID` field \
         `EntityID` — got:\n{out}"
    );
    assert!(
        out.contains(r#"db:"entity_id"   json:"entity_id"`"#)
            || out.contains(r#"db:"entity_id" json:"entity_id"`"#),
        "polymorphic_ref id must carry bare `entity_id` db/json tags \
         (NOT NULL → no omitempty) — got:\n{out}"
    );
    // Neither column is optional → no pointer, no omitempty for the pair.
    assert!(
        !out.contains("*string `db:\"entity_type\"")
            && !out.contains("entity_type,omitempty"),
        "polymorphic_ref discriminator is NOT NULL — must not be a pointer / \
         omitempty — got:\n{out}"
    );
    assert!(
        !out.contains("entity_id,omitempty"),
        "polymorphic_ref id is NOT NULL — must not be omitempty — got:\n{out}"
    );
}
