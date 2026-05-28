//! `module_name` override, deterministic output, Lazuli Go version
//! pin, and cross-feature user-defined ref tests.

use lazuli_codegen_go::{GoEmitOptions, LAZULI_GO_VERSION, generate_v1};
use lazuli_ir::{BuiltinType, Field, FieldConstraints, Resource, TypeRef};

use super::builders::{empty_feature, minimal_module};

#[test]
fn module_name_override_wins_over_app_name() {
    let module = minimal_module("test_app", "customer");
    let options = GoEmitOptions {
        module_name: Some("github.com/acme/custom-name".to_owned()),
        lazuli_go_version: LAZULI_GO_VERSION.to_owned(),
        check: false,
        plan_gate: None,
    };
    let files = generate_v1(&module, &options);
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("go.mod missing");
    assert!(
        go_mod
            .contents
            .contains("module github.com/acme/custom-name")
    );
}

#[test]
fn deterministic_output_across_runs() {
    let module = minimal_module("marketplace", "customer");
    let a = generate_v1(&module, &GoEmitOptions::default());
    let b = generate_v1(&module, &GoEmitOptions::default());
    assert_eq!(a.len(), b.len());
    for (left, right) in a.iter().zip(b.iter()) {
        assert_eq!(left.path, right.path);
        assert_eq!(left.contents, right.contents);
    }
}

#[test]
fn lazuli_go_version_override_lands_in_go_mod() {
    let module = minimal_module("marketplace", "customer");
    let options = GoEmitOptions {
        module_name: None,
        lazuli_go_version: "v9.9.9".to_owned(),
        check: false,
        plan_gate: None,
    };
    let files = generate_v1(&module, &options);
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("go.mod missing");
    // Require names the Lazuli Go module (`lazuli.dev/runtime`); the
    // generated code imports the per-bucket subpackages
    // (`lazuli.dev/runtime/lazuli`, `lazuli.dev/runtime/lazuli/storage`,
    // ...) against it.
    assert!(go_mod.contents.contains("lazuli.dev/runtime v9.9.9"));
}

#[test]
fn cross_feature_user_defined_ref_emits_qualified_type_and_import() {
    // Phase Prep §1.1 — two features. `customer` declares
    // `Customer`; `org` declares `User`. A `Customer.owner: User`
    // field should emit `*org.User` plus a `lazuli/test-app/org`
    // import in `customer/resource.gen.go`. The analyzer leaves
    // `qname.feature = None` for these refs; codegen resolves them
    // via the cross-feature index.
    let mut module = minimal_module("test_app", "customer");
    module.features.push(empty_feature("org"));

    // Add Customer{owner: User} on `customer` feature.
    module.features[0].resources.push(Resource {
        name: "Customer".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "owner".to_owned(),
            type_ref: TypeRef::UserDefined(lazuli_ir::QualifiedName {
                feature: None,
                name: "User".to_owned(),
            }),
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
        }],
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
        append_only: false,
    });

    // Add User on `org` feature.
    module.features[1].resources.push(Resource {
        name: "User".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "name".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
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
        }],
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
        append_only: false,
    });

    let files = generate_v1(&module, &GoEmitOptions::default());
    let customer_resource = files
        .iter()
        .find(|f| f.path == "customer/resource.gen.go")
        .expect("expected customer/resource.gen.go");

    // Resource-typed cross-feature FK collapses to `lazuli.ID` —
    // the DB column holds the FK as BIGINT, and the prior struct
    // ref shape would fault `pgx.RowToStructByName`. The
    // cross-feature import is no longer required for this field
    // alone; resources with mixed (record + FK) refs may still
    // pull the import.
    assert!(
        customer_resource.contents.contains("Owner lazuli.ID"),
        "expected `Owner lazuli.ID` FK shape, got:\n{}",
        customer_resource.contents
    );

    // Sanity: the `org` feature's resource file still emits `User`
    // bare (same-package).
    let org_resource = files
        .iter()
        .find(|f| f.path == "org/resource.gen.go")
        .expect("expected org/resource.gen.go");
    assert!(
        org_resource.contents.contains("type User struct"),
        "expected `type User struct` in org/resource.gen.go, got:\n{}",
        org_resource.contents
    );
}
