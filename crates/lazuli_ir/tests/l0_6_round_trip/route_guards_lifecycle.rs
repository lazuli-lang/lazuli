//! ir-route-guards §3 — ViewGuard + RouteGuardDefaults + view.guard
//! slots on the 4 view kinds. See docs/proposals/ir-route-guards.md.

use serde_json::json;

use lazuli_ir::{
    AppRoute, AudienceSurface, ExperienceModule, ExperienceView, Feature, PlatformView,
    RequiresLifecycle, ResolvedLifecycleGate, ResumeArm, ResumeArmKind, ResumeRouter,
    RoleMismatchArm, RouteGuardDefaults, RouteRedirectTarget, SpanRef, ViewGuard,
    ViewTestAssertion, WhenDeniedRoute, Experience,
};

use super::round_trip;

#[test]
fn view_guard_round_trips_with_all_slots() {
    round_trip(&ViewGuard {
        policy: vec!["@policy.host_only".to_string()],
        on_unauthenticated: Some("/sign-in".to_string()),
        on_unauthorized: Some("/explore".to_string()),
        requires_lifecycle: None,
        on_lifecycle_pending: None,
        forbid_when: Vec::new(),
        span_ref: Some(SpanRef { start: 1, end: 50 }),
    });
}

#[test]
fn view_guard_round_trips_with_only_policy() {
    round_trip(&ViewGuard {
        policy: vec!["@policy.authenticated".to_string()],
        on_unauthenticated: None,
        on_unauthorized: None,
        requires_lifecycle: None,
        on_lifecycle_pending: None,
        forbid_when: Vec::new(),
        span_ref: None,
    });
}

#[test]
fn view_guard_round_trips_with_lifecycle_slots() {
    round_trip(&ViewGuard {
        policy: vec!["@policy.host_only".to_string()],
        on_unauthenticated: None,
        on_unauthorized: None,
        requires_lifecycle: Some(RequiresLifecycle {
            resource: "Host".to_string(),
            state: "complete".to_string(),
            substep: None,
            span_ref: Some(SpanRef { start: 10, end: 47 }),
        }),
        on_lifecycle_pending: Some("host_onboarding".to_string()),
        forbid_when: Vec::new(),
        span_ref: None,
    });
}

#[test]
fn view_guard_omits_none_redirects_in_serialized_form() {
    let g = ViewGuard {
        policy: vec!["@policy.public".to_string()],
        on_unauthenticated: None,
        on_unauthorized: None,
        requires_lifecycle: None,
        on_lifecycle_pending: None,
        forbid_when: Vec::new(),
        span_ref: None,
    };
    let v = serde_json::to_value(&g).unwrap();
    assert_eq!(v, json!({ "policy": ["@policy.public"] }));
}

#[test]
fn lifecycle_route_gate_ir_round_trips_with_all_slots() {
    let lifecycle_span = SpanRef { start: 10, end: 40 };
    let resume_span = SpanRef {
        start: 50,
        end: 140,
    };
    let requires = RequiresLifecycle {
        resource: "Host".to_string(),
        state: "complete".to_string(),
        substep: Some("phone_verification".to_string()),
        span_ref: Some(lifecycle_span),
    };
    let guard = ViewGuard {
        policy: vec!["@policy.host_only".to_string()],
        on_unauthenticated: Some("/sign-in".to_string()),
        on_unauthorized: Some("/explore".to_string()),
        requires_lifecycle: Some(requires.clone()),
        on_lifecycle_pending: Some("host_onboarding".to_string()),
        forbid_when: Vec::new(),
        span_ref: Some(SpanRef { start: 1, end: 60 }),
    };
    let resolved = ResolvedLifecycleGate {
        resource: "Host".to_string(),
        state: "complete".to_string(),
        substep: Some("phone_verification".to_string()),
        resume_router: "host_onboarding".to_string(),
        source_query_qualified: "host.query.my_host".to_string(),
    };
    let router = ResumeRouter {
        name: "host_onboarding".to_string(),
        source_query: "my_host".to_string(),
        arms: vec![
            ResumeArm {
                kind: ResumeArmKind::None,
                substep: None,
                target_view: "host_onboarding_intermediation".to_string(),
                span_ref: Some(resume_span),
            },
            ResumeArm {
                kind: ResumeArmKind::State("complete".to_string()),
                substep: Some("phone_verification".to_string()),
                target_view: "host_home".to_string(),
                span_ref: None,
            },
            ResumeArm {
                kind: ResumeArmKind::Wildcard,
                substep: None,
                target_view: "host_onboarding_intermediation".to_string(),
                span_ref: None,
            },
        ],
        span_ref: Some(resume_span),
    };

    let module = ExperienceModule {
        app: None,
        routes: vec![AppRoute {
            name: "host_index".to_string(),
            path: Some("/host".to_string()),
            routes: Vec::new(),
            route_params: Vec::new(),
            to: Some("host_home".to_string()),
            surface: Some("host web".to_string()),
            audience: Some("host".to_string()),
            lazy: None,
            prerender: None,
            guard: Some(guard.clone()),
            loaders: Vec::new(),
            pending_view: None,
            error_view: None,
            parent: None,
            span_ref: None,
        }],
        experiences: vec![Experience {
            name: "host".to_string(),
            imports: Vec::new(),
            views: vec![ExperienceView {
                name: "host_home".to_string(),
                anchor: Some("host_root".to_string()),
                routes: vec!["host_index".to_string()],
                extensible_by: Vec::new(),
                source: Some("host.query.my_host".to_string()),
                submit: None,
                blocks: vec!["host_home_shell".to_string()],
                actions: Vec::new(),
                opens: Vec::new(),
                tests: Vec::<ViewTestAssertion>::new(),
                guard: Some(guard.clone()),
                resolved_guard_policy: None,
                resolved_lifecycle_gate: Some(resolved.clone()),
                span_ref: None,
            }],
            extensions: Vec::new(),
            resume_routers: vec![router.clone()],
            span_ref: None,
        }],
        surfaces: Vec::new(),
    };

    let json = serde_json::to_string(&module).expect("serialize ExperienceModule");
    let back: ExperienceModule = serde_json::from_str(&json).expect("deserialize ExperienceModule");
    assert_eq!(module, back);
    assert!(json.contains("\"requires_lifecycle\""));
    assert!(json.contains("\"on_lifecycle_pending\""));
    assert!(json.contains("\"resolved_lifecycle_gate\""));

    round_trip(&router);
    assert_eq!(
        serde_json::to_value(ResumeArmKind::State("complete".to_string())).unwrap(),
        json!({ "kind": "state", "value": "complete" })
    );

    let feature_json = json!({
        "name": "host",
        "purpose": null,
        "defaults": {},
        "uses": [],
        "enums": [],
        "resources": [],
        "events": [],
        "rules": [],
        "policies": {
            "categories": [],
            "fields": []
        },
        "commands": [],
        "queries": [],
        "resume_routers": [router],
        "workflows": [],
        "jobs": [],
        "webhooks": [],
        "surfaces": [],
        "extensions": [],
        "escape_routes": []
    });
    let feature: Feature =
        serde_json::from_value(feature_json.clone()).expect("deserialize feature");
    assert_eq!(feature.resume_routers.len(), 1);
    assert_eq!(
        serde_json::to_value(feature)
            .expect("serialize feature")
            .get("resume_routers")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn route_guard_defaults_round_trips_with_all_slots() {
    round_trip(&RouteGuardDefaults {
        default_policy: Some("@policy.public".to_string()),
        on_unauthenticated: Some("/sign-in".to_string()),
        on_unauthorized: Some("/".to_string()),
        skeleton: Some("@client.app_skeleton".to_string()),
        span_ref: None,
    });
}

#[test]
fn route_guard_defaults_round_trips_empty_block() {
    round_trip(&RouteGuardDefaults {
        default_policy: None,
        on_unauthenticated: None,
        on_unauthorized: None,
        skeleton: None,
        span_ref: None,
    });
}

#[test]
fn route_guard_defaults_omits_none_in_serialized_form() {
    let d = RouteGuardDefaults {
        default_policy: None,
        on_unauthenticated: None,
        on_unauthorized: None,
        skeleton: None,
        span_ref: None,
    };
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(v, json!({}));
}

#[test]
fn when_denied_route_round_trips_with_all_arms() {
    round_trip(&WhenDeniedRoute {
        unauthenticated: Some(RouteRedirectTarget::View("sign_in".to_string())),
        role_mismatch: vec![RoleMismatchArm {
            role: "traveler".to_string(),
            target: RouteRedirectTarget::View("explore".to_string()),
            span_ref: None,
        }],
        default: Some(RouteRedirectTarget::Path("/welcome".to_string())),
        span_ref: Some(SpanRef { start: 20, end: 80 }),
    });
}

#[test]
fn experience_view_back_compat_without_guard() {
    // Pre-this-cell fixtures lack the `guard` field. Confirm they
    // still deserialize cleanly with `guard: None`.
    let legacy_json = json!({
        "name": "host_home",
        "anchor": "host_root"
    });
    let parsed: ExperienceView = serde_json::from_value(legacy_json).unwrap();
    assert_eq!(parsed.name, "host_home");
    assert!(parsed.guard.is_none());
}

#[test]
fn audience_surface_back_compat_without_guard() {
    let legacy_json = json!({ "name": "host" });
    let parsed: AudienceSurface = serde_json::from_value(legacy_json).unwrap();
    assert_eq!(parsed.name, "host");
    assert!(parsed.guard.is_none());
}

#[test]
fn platform_view_back_compat_without_guard() {
    let legacy_json = json!({
        "name": "list",
        "view_type": "list"
    });
    let parsed: PlatformView = serde_json::from_value(legacy_json).unwrap();
    assert_eq!(parsed.name, "list");
    assert!(parsed.guard.is_none());
}

#[test]
fn app_route_back_compat_without_guard() {
    let legacy_json = json!({ "name": "host_index" });
    let parsed: AppRoute = serde_json::from_value(legacy_json).unwrap();
    assert_eq!(parsed.name, "host_index");
    assert!(parsed.guard.is_none());
}
