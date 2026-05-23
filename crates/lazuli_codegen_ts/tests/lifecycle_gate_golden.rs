use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::lzx_audience_slot::{
    LifecycleGateIntegration, LifecycleGateTarget, emit_lifecycle_gate_artifacts_from_json,
};

fn fixture_module() -> serde_json::Value {
    serde_json::json!({
        "features": [
            {
                "name": "host",
                "resources": [
                    {
                        "name": "Host",
                        "lifecycle": {
                            "discriminator_field": "lifecycle_state",
                            "generated_enum": "HostLifecycleState",
                            "states": [
                                { "name": "intermediation_terms_pending", "kind": "initial" },
                                { "name": "basic_details_pending", "kind": "intermediate" },
                                { "name": "complete", "kind": "terminal" }
                            ]
                        }
                    }
                ],
                "queries": [
                    { "kind": "Lookup", "name": "my_host", "resource": "Host" }
                ],
                "resume_routers": [
                    {
                        "name": "host_onboarding",
                        "source": { "feature": "host", "name": "my_host" },
                        "resource": "Host",
                        "arms": {
                            "none": "host_onboarding_intermediation",
                            "intermediation_terms_pending": "host_onboarding_intermediation",
                            "basic_details_pending": "host_onboarding_basic_details",
                            "complete": "host_home"
                        }
                    }
                ],
                "surfaces": [
                    surface("web"),
                    surface("mobile")
                ]
            }
        ]
    })
}

fn surface(target: &str) -> serde_json::Value {
    serde_json::json!({
        "feature": "host",
        "target": target,
        "audiences": [
            {
                "name": "host",
                "views": [
                    gated_view("host_home", "/host", "complete"),
                    gated_view(
                        "host_onboarding_basic_details",
                        "/onboarding/host/basic-details",
                        "basic_details_pending"
                    ),
                    gated_view(
                        "host_onboarding_intermediation",
                        "/onboarding/host/intermediation",
                        "intermediation_terms_pending"
                    )
                ]
            }
        ]
    })
}

fn gated_view(name: &str, route: &str, state: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "detail",
        "name": name,
        "route": route,
        "guard": {
            "policy": {
                "name": "@policy.host_only",
                "atoms": [
                    { "namespace": "scope", "name": "authenticated" },
                    { "namespace": "role", "name": "host" }
                ]
            },
            "on_unauthenticated": "/sign-in",
            "on_unauthorized": "/explore",
            "requires_lifecycle": { "resource": "Host", "state": state },
            "on_lifecycle_pending": "@resume host_onboarding"
        }
    })
}

fn gated_substep_view(name: &str, route: &str, state: &str, substep: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "detail",
        "name": name,
        "route": route,
        "guard": {
            "policy": {
                "name": "@policy.host_only",
                "atoms": [
                    { "namespace": "scope", "name": "authenticated" },
                    { "namespace": "role", "name": "host" }
                ]
            },
            "requires_lifecycle": {
                "resource": "Host",
                "state": state,
                "substep": substep
            },
            "on_lifecycle_pending": "@resume host_onboarding"
        }
    })
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}; got {files:#?}"))
        .contents
        .as_str()
}

#[test]
fn tanstack_lifecycle_gate_emit_matches_golden() {
    let files = emit_lifecycle_gate_artifacts_from_json(
        &fixture_module(),
        LifecycleGateTarget::Web,
        LifecycleGateIntegration::TanStack,
    );

    assert_eq!(files.len(), 2, "{files:#?}");
    assert_eq!(
        file(&files, "dist/ts-web/host/host.web.host.gen.ts"),
        include_str!("golden/lifecycle-gate/host.web.host.gen.ts")
    );
    assert_eq!(
        file(&files, "dist/ts-web/app/lifecycle_gates.gen.ts"),
        include_str!("golden/lifecycle-gate/lifecycle_gates.gen.ts")
    );
}

#[test]
fn hoc_lifecycle_gate_emit_matches_golden() {
    let files = emit_lifecycle_gate_artifacts_from_json(
        &fixture_module(),
        LifecycleGateTarget::Mobile,
        LifecycleGateIntegration::Hoc,
    );

    assert_eq!(files.len(), 2, "{files:#?}");
    assert_eq!(
        file(&files, "dist/ts-mobile/host/host.mobile.host.gen.ts"),
        include_str!("golden/lifecycle-gate/host.mobile.host.gen.ts")
    );
    assert_eq!(
        file(&files, "dist/ts-mobile/app/lifecycle_gates.gen.ts"),
        include_str!("golden/lifecycle-gate/lifecycle_gates.gen.ts")
    );
}

#[test]
fn lifecycle_gate_emit_preserves_substep_slot() {
    let mut fixture = fixture_module();
    fixture["features"][0]["surfaces"][0]["audiences"][0]["views"][1] = gated_substep_view(
        "host_onboarding_basic_details",
        "/onboarding/host/basic-details/phone",
        "basic_details_pending",
        "phone_verification",
    );

    let files = emit_lifecycle_gate_artifacts_from_json(
        &fixture,
        LifecycleGateTarget::Web,
        LifecycleGateIntegration::TanStack,
    );
    let generated = file(&files, "dist/ts-web/host/host.web.host.gen.ts");
    let registry = file(&files, "dist/ts-web/app/lifecycle_gates.gen.ts");

    assert!(generated.contains("substep: \"phone_verification\""));
    assert!(generated.contains("\"basic_details_pending\",\n      \"phone_verification\","));
    assert!(registry.contains("substep: \"phone_verification\""));
}
