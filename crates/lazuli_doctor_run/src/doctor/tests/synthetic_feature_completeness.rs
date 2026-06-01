// Guard B — synthetic-feature-completeness gate (the F4 drift class).
//
// Several doctor aggregators build a *synthetic* `lazuli_ir::Feature`
// from a `Tier3FeatureFacts` row via `make_synthetic_feature_for_reports`
// rather than re-lowering the source. That builder deliberately ZEROES a
// number of slots (it only populates the report-family slots it directly
// needs), and each consuming aggregator is responsible for RE-ATTACHING
// the slots its rules read. The drift hazard: a rule reads a slot the
// builder zeroed and the aggregator forgot to re-attach → the rule sees an
// empty slot and either goes dormant or, worse, fires a false positive on
// a validly-declared construct.
//
// That is exactly the F4 bug: the `domain` aggregator's
// `SCHEDULE-RULE-001` rule (`schedule_rule_invalid::check`) resolves a
// `schedule_rule from @fn.<name>` base against the feature's `fn`
// (`Function`) `extensions`. The report-shaped synthetic feature zeroes
// `extensions`, so a validly-declared `fn` was invisible → false-positive
// "unresolved fn". F4's fix re-attaches `feature.extensions =
// fact.extensions.clone()` inside `aggregators::domain::diagnostics`.
//
// These gates pin the contract so the hole cannot silently reopen:
//
// 1. `builder_zeroes_the_slots_domain_must_reattach` pins the builder's
//    precondition (it zeroes `extensions` / `policies` / `commands`) and
//    that it preserves the slots it does carry (`resources`, `aggregates`,
//    `apis`, `records`, `queries`, `agents`, `reports`, `uses`). If the
//    builder regresses to dropping a carried slot, this goes RED.
//
// 2. `domain_reattaches_extensions_so_schedule_rule_resolves` runs the
//    REAL `aggregators::domain::diagnostics` end-to-end over a fact whose
//    only domain-relevant content is a `schedule_rule from @fn.<name>`
//    field plus a matching `fn` extension. The rule MUST resolve the fn
//    and emit NOTHING. If the `extensions` re-attach is dropped (the exact
//    F4 regression), `SCHEDULE-RULE-001` false-fires and this goes RED.
//    The sibling `policies` / `commands` re-attaches are exercised the
//    same way (POLICY-PREDICATE-001 reads both), pinned here as the
//    known-consumed slots the task calls out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lazuli_ir::{
    BuiltinType, ComputedDate, ComputedDateBase, ComputedDateOffset, Extension, ExtensionContract,
    Field, FieldConstraints, PathRef, Resource, TypeRef,
};

use crate::doctor::aggregators;
use crate::doctor::{Tier3FeatureFacts, make_synthetic_feature_for_reports};

/// Build a `Tier3FeatureFacts` with every slot empty/default. Callers
/// populate the one or two slots the test under exercise needs. Mirrors
/// the construction the live loader performs in `tier3_harvest`.
fn empty_fact() -> Tier3FeatureFacts {
    Tier3FeatureFacts {
        feature: "activity".into(),
        path: PathBuf::from("activity.lzi"),
        feature_line: 1,
        tenancy_axis: None,
        defaults_policy: None,
        defaults_timestamps: false,
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        resource_previous_names: Vec::new(),
        field_previous_names: Vec::new(),
        all_resource_names_in_feature: BTreeSet::new(),
        all_field_names_in_feature: BTreeMap::new(),
        job_lines: BTreeMap::new(),
        webhook_lines: BTreeMap::new(),
        notification_lines: BTreeMap::new(),
        tenant_migration_lines: BTreeMap::new(),
        event_group_lines: BTreeMap::new(),
        commands: Vec::new(),
        command_lines: BTreeMap::new(),
        queries: Vec::new(),
        query_lines: BTreeMap::new(),
        caches: Vec::new(),
        cache_lines: BTreeMap::new(),
        api_names_text_pattern: Vec::new(),
        apis: Vec::new(),
        api_lines: BTreeMap::new(),
        agents: Vec::new(),
        translation: None,
        translation_line: 1,
        records: Vec::new(),
        enums: Vec::new(),
        events: Vec::new(),
        policies_declared: false,
        policies: lazuli_ir::Policies::default(),
        extensions: Vec::new(),
        reports: Vec::new(),
        report_lines: BTreeMap::new(),
        resources: Vec::new(),
        report_decls: Vec::new(),
        aggregates: Vec::new(),
        aggregate_lines: BTreeMap::new(),
        errors: None,
        uses: Vec::new(),
        channels: Vec::new(),
    }
}

fn mk_field(name: &str, builtin: BuiltinType) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Builtin(builtin),
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

/// A `<name>: Date computed_date from @fn.<fn_ref>(<rule>) offset <offset>`
/// field — the W4 `schedule_rule` form SCHEDULE-RULE-001 resolves against
/// the feature's `fn` extensions.
fn mk_schedule_rule_field(
    name: &str,
    rule: &str,
    fn_ref: &str,
    offset: ComputedDateOffset,
) -> Field {
    let mut f = mk_field(name, BuiltinType::Date);
    f.computed_date = Some(ComputedDate {
        base: ComputedDateBase::Rule {
            rule: rule.into(),
            fn_ref: fn_ref.into(),
        },
        offset,
    });
    f
}

fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.into(),
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
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    }
}

/// A `fn <name>` (Function) extension — the declaration SCHEDULE-RULE-001
/// resolves a `@fn.<name>` base against.
fn mk_fn_extension(name: &str) -> Extension {
    Extension {
        name: name.into(),
        contract: ExtensionContract::Function {
            input: TypeRef::Builtin(BuiltinType::Json),
            output: TypeRef::Builtin(BuiltinType::Date),
        },
        resolved_path: PathRef::authored("./handlers/x.go"),
        previous_names: Vec::new(),
        span_ref: None,
    }
}

#[test]
fn builder_zeroes_the_slots_domain_must_reattach() {
    // Populate every slot the builder is supposed to CARRY plus the three
    // it is supposed to ZERO (so a regression in either direction shows).
    let mut fact = empty_fact();
    let resource = mk_resource("Activity", vec![mk_field("name", BuiltinType::Text)]);
    fact.resources = vec![resource];
    fact.extensions = vec![mk_fn_extension("activity_date_rule")];

    let feature = make_synthetic_feature_for_reports(&fact);

    // Carried-through slots: the builder populates these directly. If a
    // future edit drops one, the report/domain rules that walk it go blind.
    assert!(
        !feature.resources.is_empty(),
        "make_synthetic_feature_for_reports must carry `resources`"
    );

    // F4 precondition: the builder ZEROES `extensions` / `policies` /
    // `commands`. This is exactly why the consuming aggregator MUST
    // re-attach them — `domain_reattaches_extensions_so_schedule_rule_resolves`
    // verifies the re-attach actually happens. If the builder ever starts
    // carrying these itself, the re-attach becomes redundant (not wrong),
    // but until then this asserts the slot the F4 bug left empty.
    assert!(
        feature.extensions.is_empty(),
        "builder is expected to zero `extensions` (the slot F4 forgot to re-attach); \
         if this changed, confirm the domain aggregator still works without the re-attach"
    );
    assert!(
        feature.policies.categories.is_empty(),
        "builder is expected to zero `policies` (re-attached by the domain aggregator)"
    );
    assert!(
        feature.commands.is_empty(),
        "builder is expected to zero `commands` (re-attached by the domain aggregator)"
    );
}

#[test]
fn domain_reattaches_extensions_so_schedule_rule_resolves() {
    // A feature whose only domain-relevant content is a `schedule_rule`
    // field bound to a VALIDLY-DECLARED `fn` extension. SCHEDULE-RULE-001
    // (`schedule_rule_invalid`) reads `feature.extensions`; if the domain
    // aggregator drops the F4 `feature.extensions = fact.extensions.clone()`
    // re-attach, the synthetic feature's `extensions` stays empty, the fn
    // is invisible, and the rule false-fires.
    let mut fact = empty_fact();
    let resource = mk_resource(
        "Activity",
        vec![
            mk_field("offset_days", BuiltinType::Integer),
            mk_schedule_rule_field(
                "due_date",
                "input.rule",
                "activity_date_rule",
                ComputedDateOffset::Field("offset_days".into()),
            ),
        ],
    );
    fact.resources = vec![resource];
    fact.extensions = vec![mk_fn_extension("activity_date_rule")];

    let diagnostics = aggregators::domain::diagnostics(std::slice::from_ref(&fact));

    let schedule_findings: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "SCHEDULE-RULE-001")
        .map(|d| d.message.as_str())
        .collect();

    assert!(
        schedule_findings.is_empty(),
        "SCHEDULE-RULE-001 false-fired on a validly-declared `fn` — the domain aggregator \
         dropped the `feature.extensions = fact.extensions.clone()` re-attach (the F4 \
         regression). Synthetic feature is missing a slot a registered rule consumes. \
         Findings: {schedule_findings:?}"
    );

    // Sanity: the rule is genuinely wired and CAN fire — drop the extension
    // and the same fact must now produce exactly one SCHEDULE-RULE-001. This
    // proves the green above is "fn resolved", not "rule dormant".
    let mut unbound = fact.clone();
    unbound.extensions = Vec::new();
    let unbound_diagnostics = aggregators::domain::diagnostics(std::slice::from_ref(&unbound));
    assert_eq!(
        unbound_diagnostics
            .iter()
            .filter(|d| d.code == "SCHEDULE-RULE-001")
            .count(),
        1,
        "control case: with no `fn` extension, SCHEDULE-RULE-001 must fire exactly once \
         (proves the resolved-green case is not a dormant rule)"
    );
}
