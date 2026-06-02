use super::{
    author_list_customers_query, customer_resource, empty_feature, req_field, req_unique_field,
    user_qn,
};
use crate::{CrudSynthDiagnostic, synthesize_conventions};
use lazuli_ir as ir;

#[test]
fn fx1_crud_without_author_query_emits_catalog_queries() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

    let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
    assert_eq!(q_names, vec!["lookup_customer", "list_customers"]);
}

#[test]
fn fx1_crud_author_list_query_silences_synth() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());
    feature
        .queries
        .push(author_list_customers_query(ir::PolicyRef::Local(
            "authenticated".to_owned(),
        )));

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

    let list_count = feature
        .queries
        .iter()
        .filter(|q| q.name() == "list_customers")
        .count();
    assert_eq!(
        list_count, 1,
        "author list_customers must not be duplicated"
    );
    assert_eq!(
        feature.synth_origins.get("list_customers"),
        Some(&ir::ConventionOrigin::AuthorOverride(
            ir::ConventionRef::Crud
        ))
    );
}

#[test]
fn fx1_crud_author_list_query_policy_mismatch_warns_and_silences() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());
    feature
        .queries
        .push(author_list_customers_query(ir::PolicyRef::Local(
            "customer_admin".to_owned(),
        )));

    let diags = synthesize_conventions(&mut feature);
    let mismatch = diags
        .iter()
        .find(|d| {
            matches!(
                d,
                CrudSynthDiagnostic::SignatureMismatch { resource, synth_name, .. }
                    if resource == "Customer" && synth_name == "list_customers"
            )
        })
        .expect("expected SignatureMismatch for list_customers policy divergence");
    assert_eq!(
        mismatch.diagnostic_code(),
        "@correctness.crud_synth_author_signature_mismatch"
    );
    assert_eq!(mismatch.severity(), "warning");

    let lists: Vec<&ir::Query> = feature
        .queries
        .iter()
        .filter(|q| q.name() == "list_customers")
        .collect();
    assert_eq!(
        lists.len(),
        1,
        "author list_customers must not be duplicated"
    );
    match lists[0] {
        ir::Query::List(lq) => {
            assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));
        }
        other => panic!("expected List query, got {other:?}"),
    }
}

#[test]
fn fx1_without_crud_author_list_query_has_no_synth_collision() {
    let mut feature = empty_feature("customer", true);
    let mut resource = customer_resource();
    resource.conventions = Vec::new();
    feature.resources.push(resource);
    feature
        .queries
        .push(author_list_customers_query(ir::PolicyRef::Local(
            "authenticated".to_owned(),
        )));

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    assert!(feature.commands.is_empty());
    assert_eq!(feature.queries.len(), 1);
    assert_eq!(feature.queries[0].name(), "list_customers");
}

/// §5.7 edge — resource with `user: User required unique` places
/// both `org` and `user` in the Tenant group (neither lands in
/// input).
#[test]
fn user_unique_resource_drops_user_from_inputs() {
    let mut feature = empty_feature("photoshare", true);
    feature.resources.push(ir::Resource {
        name: "PhotoShare".to_owned(),
        public_contract: None,
        tenancy: Some(ir::Tenancy::Org),
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![
            req_field("org", user_qn("Org")),
            req_unique_field("user", user_qn("User")),
            req_field("caption", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
        ],
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
        conventions: vec![ir::ConventionRef::Crud],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    });

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "no diagnostics expected, got {:?}", diags);

    let create = feature
        .commands
        .iter()
        .find(|c| c.name == "create_photo_share")
        .unwrap();
    match &create.input {
        ir::CommandInput::Typed(slots) => {
            let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
            // org + user are Tenant; only caption remains.
            assert_eq!(names, vec!["caption"]);
        }
        other => panic!("expected Typed input, got {:?}", other),
    }
}

/// §5.7 edge — resource without a lifecycle block has no discriminator
/// to drop. A field named like a discriminator on another resource
/// stays in input. Verifies the discriminator-skip is gated on
/// `resource.lifecycle` being `Some`.
#[test]
fn resource_without_lifecycle_keeps_status_field() {
    let mut feature = empty_feature("customer", true);
    // Customer above has `status` field; it has NO lifecycle block,
    // so `status` should land in create / update input.
    feature.resources.push(customer_resource());
    let _ = synthesize_conventions(&mut feature);

    let create = feature
        .commands
        .iter()
        .find(|c| c.name == "create_customer")
        .unwrap();
    let names: Vec<&str> = match &create.input {
        ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.as_str()).collect(),
        other => panic!("expected Typed input, got {:?}", other),
    };
    assert!(names.contains(&"status"));
}

/// §11 — `crud_synth_no_required_fields` fires when every required
/// field is Tenant or Auto. Build a resource with only `org`,
/// `id`, `created_at` (all Tenant/Auto).
#[test]
fn empty_required_emits_no_required_fields_diagnostic() {
    let mut feature = empty_feature("ledger", true);
    feature.resources.push(ir::Resource {
        name: "Ledger".to_owned(),
        public_contract: None,
        tenancy: Some(ir::Tenancy::Org),
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![
            req_field("id", ir::TypeRef::Builtin(ir::BuiltinType::Id)),
            req_field("org", user_qn("Org")),
            req_field(
                "created_at",
                ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
            ),
        ],
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
        conventions: vec![ir::ConventionRef::Crud],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    });

    let diags = synthesize_conventions(&mut feature);
    assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::NoRequiredFields { resource } if resource == "Ledger")),
            "expected NoRequiredFields for Ledger, got {:?}",
            diags
        );
}

/// §11 — `crud_synth_policy_not_found` fires when the feature has
/// no `authenticated` policy. Synth still produces entries with the
/// canonical PolicyRef; Cell C4 surfaces the diagnostic to the
/// author.
#[test]
fn missing_authenticated_policy_emits_diagnostic() {
    let mut feature = empty_feature("customer", false); // no authenticated
    feature.resources.push(customer_resource());

    let diags = synthesize_conventions(&mut feature);
    assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::PolicyNotFound { resource } if resource == "Customer")),
            "expected PolicyNotFound for Customer, got {:?}",
            diags
        );
}

/// §11 — `crud_synth_signature_mismatch` fires when author wrote
/// `update_customer` with a non-canonical input list (e.g., extra
/// field).
#[test]
fn diverging_author_signature_emits_mismatch_diagnostic() {
    let mut feature = empty_feature("customer", true);
    feature.resources.push(customer_resource());
    // Author wrote update_customer with extra `notes` field — diverges.
    feature.commands.push(ir::Command {
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
                name: "name".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "notes".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
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
            where_clause: Vec::new(),
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
    });

    let diags = synthesize_conventions(&mut feature);
    assert!(
        diags.iter().any(|d| matches!(
            d,
            CrudSynthDiagnostic::SignatureMismatch { resource, synth_name, .. }
                if resource == "Customer" && synth_name == "update_customer"
        )),
        "expected SignatureMismatch for update_customer, got {:?}",
        diags
    );
}

/// Resource without `conventions [crud]` is a no-op for the synth.
#[test]
fn resource_without_conventions_is_no_op() {
    let mut feature = empty_feature("customer", true);
    let mut r = customer_resource();
    r.conventions = Vec::new();
    feature.resources.push(r);

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty());
    assert!(feature.commands.is_empty());
    assert!(feature.queries.is_empty());
}

#[test]
fn crud_delete_soft_aware() {
    // Spec 0015 — `conventions [crud]` on a `soft_delete by` resource
    // still synthesizes a `delete_<r>` command with a `Deletes` effect;
    // the runtime turns that into a soft-delete (`UPDATE ... SET
    // deleted_at = now(), deleted_by = $actor`) because the resource
    // carries `soft_delete` / `soft_delete_actor`. The synth itself is
    // shape-stable (always a `Deletes` effect); soft-vs-hard is keyed on
    // the resource flags downstream (codegen `SoftDelete[Actor]` value).
    let mut feature = empty_feature("customer", true);
    let mut r = customer_resource();
    r.soft_delete = true;
    r.soft_delete_actor = true;
    feature.resources.push(r);

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

    let delete = feature
        .commands
        .iter()
        .find(|c| c.name == "delete_customer")
        .expect("delete_customer synthesized");
    assert!(matches!(delete.kind, ir::CommandKind::Delete));
    match &delete.effect {
        ir::CommandEffect::Deletes(e) => assert_eq!(e.resource.name, "Customer"),
        other => panic!("expected Deletes effect, got {:?}", other),
    }
    // The flags that drive the soft path survive synth on the resource.
    let res = feature
        .resources
        .iter()
        .find(|res| res.name == "Customer")
        .unwrap();
    assert!(res.soft_delete, "soft path keyed on Resource.soft_delete");
    assert!(
        res.soft_delete_actor,
        "actor stamp keyed on Resource.soft_delete_actor"
    );
}

#[test]
fn crud_delete_hard_when_no_soft_delete() {
    // Edge: `[crud]` WITHOUT `soft_delete` still produces a `Deletes`
    // effect, but the resource carries no soft-delete flags, so codegen
    // + runtime emit a hard `DELETE FROM` (back-compat, unchanged).
    let mut feature = empty_feature("customer", true);
    let r = customer_resource(); // soft_delete = false by default
    feature.resources.push(r);

    let diags = synthesize_conventions(&mut feature);
    assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

    let delete = feature
        .commands
        .iter()
        .find(|c| c.name == "delete_customer")
        .expect("delete_customer synthesized");
    assert!(matches!(delete.kind, ir::CommandKind::Delete));
    let res = feature
        .resources
        .iter()
        .find(|res| res.name == "Customer")
        .unwrap();
    assert!(!res.soft_delete, "hard delete: no soft_delete flag");
    assert!(!res.soft_delete_actor);
}
