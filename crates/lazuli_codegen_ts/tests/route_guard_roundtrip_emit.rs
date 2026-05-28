//! Byte-equal snapshot for the round-trip canonical demo fixture at
//! `examples/full-capsule/route-guard-roundtrip/fixture.lzx`.
//!
//! Cell B-1 (`ir-route-guard-escape-hatch-2026-05-28` §5 / §4.5) needs
//! `expected.emit.ts` to be genuinely emit-equal to what the TanStack-
//! web codegen produces — the file is the load-bearing AI-first
//! authoring proof, not a stub.
//!
//! The fixture exercises all 3 new policy slots in one route:
//!   * `requires_lifecycle_in Host [basic_details_pending, address_pending]`
//!   * `requires user.lookup_my.is_phone_verified = true on_unmet redirect "/demo/phone-verify"`
//!   * `forbid_when @role.guest dispatch_to "/demo/welcome" only_when lifecycle Host = complete`
//!
//! The IR shape mirrors what Cell A's parser produces; the snapshot
//! shape mirrors what Cell B-1's `before_load.rs` + `mod.rs` emit. If
//! either drifts, `expected.emit.ts` is regenerated and the snapshot
//! update is reviewed as part of the codegen-emit contract.

use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::routes::{RoutesTarget, emit_routes_artifacts};
use lazuli_ir::{
    AppRoute, Experience, Feature, LifecycleRouteArm, LifecycleRoutes, Platform, PlatformSurface,
    Resource,
};

fn host_feature() -> Feature {
    let source = r#"
feature host
  policies
    authenticated: @scope.authenticated
  query.lookup lookup_my_host
"#;
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(source).expect("host skeleton parses");
    let mut feature = lazuli_analyzer::lower_feature_skeleton(&skeletons[0])
        .expect("host feature lowers");
    feature.resources.push(Resource {
        name: "Host".to_string(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: Vec::new(),
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
        lifecycle_routes: Some(LifecycleRoutes {
            arms: vec![
                LifecycleRouteArm {
                    state: "basic_details_pending".to_string(),
                    url: "/onboarding/host/basic-details".to_string(),
                },
                LifecycleRouteArm {
                    state: "address_pending".to_string(),
                    url: "/onboarding/host/address".to_string(),
                },
                LifecycleRouteArm {
                    state: "complete".to_string(),
                    url: "/host".to_string(),
                },
                LifecycleRouteArm {
                    state: "*".to_string(),
                    url: "/onboarding/host".to_string(),
                },
            ],
            span_ref: None,
        }),
    });
    feature
}

fn user_feature() -> Feature {
    let source = r#"
feature user
  query.lookup lookup_my_user
"#;
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(source).expect("user skeleton parses");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("user feature lowers")
}

fn roundtrip_route() -> AppRoute {
    serde_json::from_value(serde_json::json!({
        "name": "roundtrip_canonical_demo",
        "path": "/demo/roundtrip",
        "to": "host.view.roundtrip_canonical_demo",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "requires_lifecycle_in": {
                "resource": "Host",
                "allowed_states": ["basic_details_pending", "address_pending"]
            },
            "requires_field": [
                {
                    "feature": "user",
                    "field": "is_phone_verified",
                    "expected": { "kind": "Boolean", "value": true },
                    "on_unmet_redirect": "/demo/phone-verify"
                }
            ],
            "forbid_when": [
                {
                    "atom_ref": "@role.guest",
                    "atom": { "namespace": "role", "name": "guest" },
                    "dispatch_to": "/demo/welcome",
                    "only_when_lifecycle": {
                        "resource": "Host",
                        "state": "complete"
                    }
                }
            ]
        }
    }))
    .expect("route fixture parses")
}

#[test]
fn roundtrip_canonical_demo_matches_expected_emit_ts() {
    let surfaces = vec![PlatformSurface {
        experience: "host".to_string(),
        platform: Platform::Web,
        uses_experience: None,
        audiences: Vec::new(),
        span_ref: None,
    }];
    let experiences = vec![Experience {
        name: "host".to_string(),
        imports: Vec::new(),
        views: Vec::new(),
        resume_routers: Vec::new(),
        extensions: Vec::new(),
        span_ref: None,
    }];
    let features = vec![host_feature(), user_feature()];

    let files: Vec<GeneratedFile> = emit_routes_artifacts(
        None,
        &[roundtrip_route()],
        &surfaces,
        &experiences,
        &features,
        RoutesTarget::Web,
    );
    let actual = files
        .iter()
        .find(|f| f.path == "dist/ts-web/host/routes.gen.tsx")
        .expect("routes.gen.tsx emitted")
        .contents
        .as_str();

    let expected = include_str!(
        "../../../examples/full-capsule/route-guard-roundtrip/expected.emit.ts"
    );

    assert_eq!(
        actual, expected,
        "round-trip emit drifted from expected.emit.ts; \
         if intentional, re-snapshot the file"
    );
}
