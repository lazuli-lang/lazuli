//! Resource + enum emit tests — feature kinds beyond plain commands.
//!
//! Cell E2 / E2.5 invariants: a feature carrying resources or enums
//! materialises `<feature>/resource.gen.go` / `<feature>/enum.gen.go`
//! alongside the stub, with skip rules when the feature is bare.

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{
    BuiltinType, EnumDecl, EnumVariant, Field, FieldConstraints, Resource, StorageValue, TypeRef,
};

use super::builders::minimal_module;

#[test]
fn resource_kind_emits_typed_struct_and_resource_value() {
    // Cell E2 integration smoke: a feature carrying one resource
    // surfaces an extra `<feature>/resource.gen.go` alongside the
    // stub. Typed struct + `lazuli.Resource[T]` value land per
    // proposal §3.1.
    let mut module = minimal_module("marketplace", "customer");
    let resource = Resource {
        name: "customer".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
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
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    };
    module.features[0].resources.push(resource);

    let files = generate_v1(&module, &GoEmitOptions::default());
    let resource_file = files
        .iter()
        .find(|f| f.path == "customer/resource.gen.go")
        .expect("expected customer/resource.gen.go alongside the stub");
    assert!(
        resource_file.contents.contains("type Customer struct"),
        "expected struct declaration in resource.gen.go:\n{}",
        resource_file.contents
    );
    assert!(
        resource_file
            .contents
            .contains("var customerResource = lazuli.Resource[Customer]"),
        "expected `var customerResource = lazuli.Resource[Customer]` in resource.gen.go:\n{}",
        resource_file.contents
    );
}

#[test]
fn resource_kind_skips_file_when_feature_declares_no_resources_or_records() {
    // A bare feature stays at the `<feature>.gen.go` stub only; we
    // don't materialise an empty `resource.gen.go` because a single
    // `package <feature>` line provides no signal and would clutter
    // the listing.
    let module = minimal_module("marketplace", "customer");
    let files = generate_v1(&module, &GoEmitOptions::default());
    assert!(
        files.iter().all(|f| f.path != "customer/resource.gen.go"),
        "expected no resource.gen.go for an empty feature, got files: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}

#[test]
fn enum_kind_emits_typed_alias_and_const_block() {
    // Cell E2.5 integration smoke: a feature carrying one int-typed
    // enum surfaces an extra `<feature>/enum.gen.go` alongside the
    // stub. Typed alias + aligned const block land per proposal §3.
    let mut module = minimal_module("marketplace", "customer");
    module.features[0].enums.push(EnumDecl {
        name: "CustomerStatus".to_owned(),
        public_contract: None,
        variants: vec![
            EnumVariant {
                name: "lead".to_owned(),
                storage_value: Some(StorageValue::Integer(10)),
                label_key: None,
                hint_key: None,
                icon_key: None,
                previous_names: Vec::new(),
            },
            EnumVariant {
                name: "active".to_owned(),
                storage_value: Some(StorageValue::Integer(20)),
                label_key: None,
                hint_key: None,
                icon_key: None,
                previous_names: Vec::new(),
            },
        ],
        previous_names: Vec::new(),
        span_ref: None,
    });

    let files = generate_v1(&module, &GoEmitOptions::default());
    let enum_file = files
        .iter()
        .find(|f| f.path == "customer/enum.gen.go")
        .expect("expected customer/enum.gen.go alongside the stub");
    assert!(
        enum_file.contents.contains("type CustomerStatus int64"),
        "expected typed alias in enum.gen.go:\n{}",
        enum_file.contents
    );
    assert!(
        enum_file.contents.contains("const ("),
        "expected const block in enum.gen.go:\n{}",
        enum_file.contents
    );
    assert!(
        enum_file
            .contents
            .contains("CustomerStatusLead   CustomerStatus = 10"),
        "expected aligned const row in enum.gen.go:\n{}",
        enum_file.contents
    );
}

#[test]
fn enum_kind_skips_file_when_feature_declares_no_enums() {
    // A bare feature should not materialise `enum.gen.go`. Mirrors
    // the resource-side skip rule so the output listing stays
    // signal-rich.
    let module = minimal_module("marketplace", "customer");
    let files = generate_v1(&module, &GoEmitOptions::default());
    assert!(
        files.iter().all(|f| f.path != "customer/enum.gen.go"),
        "expected no enum.gen.go for an empty feature, got files: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}
