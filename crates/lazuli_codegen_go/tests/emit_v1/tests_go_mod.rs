//! `go.mod` plugin require lines + geo/postgis require + workspace
//! dev_replace tests. Walks the manifest-aware emit paths through the
//! `LazuriteGenerateGo` toggle space.

use lazuli_codegen_go::{
    GoEmitOptions, LazuriteGenerateGo, generate_v1, generate_v1_with_manifest,
};
use lazuli_ir::{BuiltinType, Field, FieldConstraints, Resource, TypeRef};

use super::builders::{lazurite_manifest, minimal_module};

#[test]
fn emit_go_mod_with_plugins_emits_require_lines() {
    let module = minimal_module("test_app", "customer");
    let manifest = lazurite_manifest(
        vec![(
            "@lazuli/plugin-foo",
            "github.com/lazuli-lang/lazuli-plugin-foo",
        )],
        Some(LazuriteGenerateGo {
            emit_main: true,
            submodule: true,
            dev_replace: None,
            dev_work_replace: None,
        }),
        Vec::new(),
    );
    let files = generate_v1_with_manifest(&module, &GoEmitOptions::default(), Some(&manifest));
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("expected generated go.mod");

    assert!(
        go_mod
            .contents
            .contains("github.com/lazuli-lang/lazuli-plugin-foo v0.1.0")
    );
}

#[test]
fn emit_go_mod_with_geopoint_resource_adds_postgis_require() {
    let mut module = minimal_module("marketplace", "listing");
    module.features[0].resources.push(Resource {
        name: "Listing".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "location".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticGeoPoint),
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
        append_only: false,
    });

    let files = generate_v1(&module, &GoEmitOptions::default());
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("expected generated go.mod");

    assert!(
        go_mod
            .contents
            .contains("github.com/cridenour/go-postgis v1.0.2")
    );
}

#[test]
fn emit_go_mod_with_dev_replace_requires_runtime_zero_in_workspace_mode() {
    // Workspace mode (manifest + submodule) writes a sibling `go.work`
    // or keeps an existing replace directive that points
    // `lazuli.dev/runtime` at local source. `dist/go/go.mod` still
    // needs a require line so Go puts the runtime module on the build
    // list; the zero version makes the local override explicit.
    let module = minimal_module("test_app", "customer");
    let manifest = lazurite_manifest(
        Vec::new(),
        Some(LazuriteGenerateGo {
            emit_main: true,
            submodule: true,
            dev_replace: Some("../../runtime/go".to_owned()),
            dev_work_replace: None,
        }),
        Vec::new(),
    );
    let files = generate_v1_with_manifest(&module, &GoEmitOptions::default(), Some(&manifest));
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("expected generated go.mod");

    assert!(
        go_mod.contents.contains("lazuli.dev/runtime v0.0.0"),
        "workspace mode must require `lazuli.dev/runtime v0.0.0` in dist/go/go.mod:\n{}",
        go_mod.contents
    );
    // module.rs:876-890 — `replace lazuli.dev/runtime => <path>` is now
    // emitted unconditionally whenever `dev_replace_runtime` is set,
    // including workspace mode. Empirically the go.work `use` line is
    // not enough by itself; freshly-scaffolded projects fail with
    // `lazuli.dev/runtime@v0.0.0: unrecognized import path` until the
    // replace lands in go.mod too. Both forms point at the same path,
    // so the duplication is harmless.
    assert!(
        go_mod.contents.contains("replace lazuli.dev/runtime"),
        "dev_replace must surface as `replace lazuli.dev/runtime` in dist/go/go.mod even under workspace mode:\n{}",
        go_mod.contents
    );
}

#[test]
fn emit_go_work_with_dev_replace_includes_runtime_path() {
    let module = minimal_module("test_app", "customer");
    let manifest = lazurite_manifest(
        Vec::new(),
        Some(LazuriteGenerateGo {
            emit_main: true,
            submodule: true,
            dev_replace: Some("../../runtime/go".to_owned()),
            dev_work_replace: None,
        }),
        Vec::new(),
    );
    let files = generate_v1_with_manifest(&module, &GoEmitOptions::default(), Some(&manifest));
    let go_work = files
        .iter()
        .find(|f| f.path == "go.work")
        .expect("expected generated go.work");

    assert!(go_work.contents.contains("../../runtime/go"));
}

#[test]
fn emit_go_mod_without_manifest_falls_back_to_legacy_behavior() {
    let module = minimal_module("test_app", "customer");
    let files = generate_v1(&module, &GoEmitOptions::default());
    let go_mod = files
        .iter()
        .find(|f| f.path == "go.mod")
        .expect("expected generated go.mod");

    assert!(go_mod.contents.contains("lazuli.dev/runtime"));
    assert!(!go_mod.contents.contains("replace lazuli.dev/runtime"));
    assert!(
        !go_mod
            .contents
            .contains("github.com/lazuli-lang/lazuli-plugin")
    );
    assert!(!go_mod.contents.contains("github.com/cridenour/go-postgis"));
}
