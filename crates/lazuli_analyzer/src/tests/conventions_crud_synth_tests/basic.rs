use super::{
    author_list_customers_query, customer_resource, empty_feature, req_field, req_unique_field,
    user_qn,
};
use crate::{CrudSynthDiagnostic, synthesize_conventions};
use lazuli_ir as ir;

#[test]
fn synth_produces_five_entries_for_customer_resource() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());

    let diags = synthesize_conventions(&mut feature);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for clean Customer, got {:?}",
        diags
    );

    // 3 commands appended: create / update / delete.
    let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        cmd_names,
        vec!["create_customer", "update_customer", "delete_customer"]
    );

    // 2 queries appended: lookup / list.
    let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
    assert_eq!(q_names, vec!["lookup_customer", "list_customers"]);

    // create_customer §5.2 shape — input has [email, name, status]
    // (org + created_at are Tenant/Auto, dropped).
    let create = feature
        .commands
        .iter()
        .find(|c| c.name == "create_customer")
        .unwrap();
    assert!(matches!(create.kind, ir::CommandKind::Create));
    match &create.input {
        ir::CommandInput::Typed(slots) => {
            let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["email", "name", "status"]);
            // Required-on-resource fields stay required.
            assert!(slots.iter().all(|s| s.required));
        }
        other => panic!("expected Typed input, got {:?}", other),
    }
    match &create.effect {
        ir::CommandEffect::Creates(e) => assert_eq!(e.resource.name, "Customer"),
        other => panic!("expected Creates effect, got {:?}", other),
    }
    let create_rate_limit = create.rate_limit.as_ref().expect("rate_limit");
    assert_eq!(create_rate_limit.default, "100 per 10 minutes per ip");
    assert!(create_rate_limit.by_env.is_empty());
    assert!(matches!(&create.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
    assert!(create.audit.is_some());

    // update_customer §5.3 — every field becomes optional in input,
    // route id: ID present, effect Updates Customer.
    let update = feature
        .commands
        .iter()
        .find(|c| c.name == "update_customer")
        .unwrap();
    assert!(matches!(update.kind, ir::CommandKind::Update));
    assert_eq!(update.route.len(), 1);
    assert_eq!(update.route[0].name, "id");
    assert!(matches!(
        update.route[0].type_ref,
        ir::TypeRef::Builtin(ir::BuiltinType::Id)
    ));
    match &update.input {
        ir::CommandInput::Typed(slots) => {
            let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["email", "name", "status"]);
            // All slots optional per §5.3.
            assert!(slots.iter().all(|s| !s.required));
        }
        other => panic!("expected Typed input, got {:?}", other),
    }
    match &update.effect {
        ir::CommandEffect::Updates(e) => assert_eq!(e.resource.name, "Customer"),
        other => panic!("expected Updates effect, got {:?}", other),
    }

    // delete_customer §5.4 — no input, route id, Deletes effect.
    let delete = feature
        .commands
        .iter()
        .find(|c| c.name == "delete_customer")
        .unwrap();
    assert!(matches!(delete.kind, ir::CommandKind::Delete));
    assert_eq!(delete.route.len(), 1);
    assert_eq!(delete.route[0].name, "id");
    assert!(matches!(delete.input, ir::CommandInput::Empty));
    match &delete.effect {
        ir::CommandEffect::Deletes(e) => assert_eq!(e.resource.name, "Customer"),
        other => panic!("expected Deletes effect, got {:?}", other),
    }

    // lookup_customer §5.5 — Lookup with key id, policy authenticated.
    let lookup = feature
        .queries
        .iter()
        .find(|q| q.name() == "lookup_customer")
        .unwrap();
    match lookup {
        ir::Query::Lookup(lq) => {
            assert_eq!(lq.keys.len(), 1);
            assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
            assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
        }
        other => panic!("expected Lookup query, got {:?}", other),
    }

    // list_customers §5.6 — List with limit+offset params, paginate 50.
    let list = feature
        .queries
        .iter()
        .find(|q| q.name() == "list_customers")
        .unwrap();
    match list {
        ir::Query::List(lq) => {
            let pnames: Vec<&str> = lq.params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(pnames, vec!["limit", "offset"]);
            assert!(lq.params.iter().all(|p| !p.required));
            assert_eq!(lq.paginate, Some(50));
            assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
        }
        other => panic!("expected List query, got {:?}", other),
    }
}

/// §5.2 / §5.3 binding axis — both the synthesized create_<R> and
/// update_<R> commands must carry one `<field> = input.<field>`
/// assignment per input slot, mirroring what the author would have
/// written by hand. Without these the Go codegen emits an empty
/// `lazuli.Bindings{}` body and every dispatch tripped the runtime
/// guard "updates effect requires Bind bindings" (PG 500 at first
/// call). Regression for the 2026-05-22 the canonical pilot /settings save
/// outage; pairs with `create_<R>` having the same gap.
#[test]
fn synth_create_and_update_populate_assignments_from_input() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "unexpected synth diagnostics: {diags:?}");

    let create = feature
        .commands
        .iter()
        .find(|c| c.name == "create_customer")
        .expect("create_customer must synth");
    let create_assignments = match &create.effect {
        ir::CommandEffect::Creates(e) => &e.assignments,
        other => panic!("expected Creates effect, got {:?}", other),
    };
    let create_fields: Vec<&str> = create_assignments
        .iter()
        .map(|a| a.field.as_str())
        .collect();
    assert_eq!(
        create_fields,
        vec!["email", "name", "status"],
        "create assignments must mirror input slots in order"
    );
    for a in create_assignments {
        match &a.value {
            ir::Expr::Path(p) => assert_eq!(
                p.segments,
                vec!["input".to_owned(), a.field.clone()],
                "create assignment value must be `input.<field>`"
            ),
            other => panic!("create assignment value not a Path: {:?}", other),
        }
    }

    let update = feature
        .commands
        .iter()
        .find(|c| c.name == "update_customer")
        .expect("update_customer must synth");
    let update_assignments = match &update.effect {
        ir::CommandEffect::Updates(e) => &e.assignments,
        other => panic!("expected Updates effect, got {:?}", other),
    };
    let update_fields: Vec<&str> = update_assignments
        .iter()
        .map(|a| a.field.as_str())
        .collect();
    assert_eq!(
        update_fields,
        vec!["email", "name", "status"],
        "update assignments must mirror input slots in order"
    );
    for a in update_assignments {
        match &a.value {
            ir::Expr::Path(p) => assert_eq!(
                p.segments,
                vec!["input".to_owned(), a.field.clone()],
                "update assignment value must be `input.<field>`"
            ),
            other => panic!("update assignment value not a Path: {:?}", other),
        }
    }
}

/// §9 worked override — author wrote `update_customer`; other 4
/// still synthesize; no warning emitted.
#[test]
fn author_override_skips_just_that_name() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());

    // Author's update_customer: matches canonical input + Updates
    // Customer (so no signature_mismatch diagnostic should fire).
    let author_update = ir::Command {
        name: "update_customer".to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: ir::CommandInput::Typed(vec![
            ir::TypedSlot {
                name: "email".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "name".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "status".to_owned(),
                type_ref: ir::TypeRef::UserDefined(ir::QualifiedName {
                    feature: None,
                    name: "CustomerStatus".to_owned(),
                }),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
        ]),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            },
            assignments: Vec::new(),
        }),
        policy: ir::PolicyRef::Local("customer_admin".to_owned()),
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
    };
    feature.commands.push(author_update);

    let diags = synthesize_conventions(&mut feature);
    assert!(
        diags.is_empty(),
        "matching-signature author override should not emit a diagnostic, got {:?}",
        diags
    );

    let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
    assert!(cmd_names.contains(&"create_customer"));
    assert!(cmd_names.contains(&"delete_customer"));
    // update_customer present, but appears exactly once (the author's).
    let update_count = cmd_names
        .iter()
        .filter(|n| **n == "update_customer")
        .count();
    assert_eq!(update_count, 1, "update_customer must not be duplicated");

    // The remaining update_customer is the author's — its policy is
    // `customer_admin`, not `authenticated`.
    let update = feature
        .commands
        .iter()
        .find(|c| c.name == "update_customer")
        .unwrap();
    assert!(matches!(&update.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));

    let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
    assert!(q_names.contains(&"lookup_customer"));
    assert!(q_names.contains(&"list_customers"));
}
