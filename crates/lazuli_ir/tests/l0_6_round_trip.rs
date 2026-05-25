//! L0 #6 — terminal, search, sort, selection, setting, drawer, on-success,
//! auth-rotation, route-guard, and lifecycle-gate serde round-trips.
//!
//! Previously lived inline as `mod l0_6_ir_tests` in `lazuli_ir/src/lib.rs`;
//! promoted to an integration test by Wave R3-F Stage 3 to shrink the lib
//! root toward the ≤ 1000 LOC gold-standard target. These tests only touch
//! the public `lazuli_ir` ABI surface — they belong at the integration
//! layer.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use lazuli_ir::{
    AppRoute, AudienceSurface, AuthSessions, BindingRef, CellBinding, CommandRef,
    DrawerBindingSource, DrawerRouteBinding, DrawerSubView, DrawerTrigger, Experience,
    ExperienceModule, ExperienceView, Feature, FilterCardinality, FilterDecl, FlashSpec,
    InvalidatesSpec, ListRender, OnSuccessSpec, PlatformView, QualifiedName, QueryKind, QueryRef,
    RequiresLifecycle, ResolvedLifecycleGate, ResumeArm, ResumeArmKind, ResumeRouter,
    RoleMismatchArm, RotationConfig, RouteGuardDefaults, RouteRedirectTarget, SearchDecl,
    SearchField, SearchMode, SelectionDecl, SelectionMode, SettingDecl, SettingPersistence,
    SettingValueSpace, SortDecl, SortDir, SpanRef, TheftAction, TranslationKeyRef, ViewGuard,
    ViewTestAssertion, WhenDeniedRoute,
};

fn round_trip<T>(value: &T)
where
    T: Clone + PartialEq + std::fmt::Debug + Serialize + DeserializeOwned,
{
    let encoded = serde_json::to_string(value).expect("serialize IR value");
    let decoded: T = serde_json::from_str(&encoded).expect("deserialize IR value");
    assert_eq!(*value, decoded);
}

fn query_ref(name: &str) -> QueryRef {
    QueryRef {
        feature: "item".to_string(),
        kind: QueryKind::Lookup,
        name: name.to_string(),
    }
}

fn command_ref(name: &str) -> CommandRef {
    CommandRef {
        feature: "item".to_string(),
        name: name.to_string(),
    }
}

#[test]
fn terminal_render_drawer_and_filter_ir_round_trip() {
    round_trip(&ListRender::Table {
        columns: vec!["title".to_string(), "updated".to_string()],
    });
    round_trip(&ListRender::Cells {
        slot: "item_card".to_string(),
    });

    let route_binding = DrawerRouteBinding {
        target: "key".to_string(),
        source: DrawerBindingSource::Selection,
    };
    round_trip(&route_binding);
    round_trip(&DrawerTrigger::ManualOpen);
    round_trip(&DrawerBindingSource::Selection);
    round_trip(&DrawerSubView {
        name: "item_detail".to_string(),
        trigger: DrawerTrigger::Select,
        source: query_ref("by_id"),
        route_binding: Some(route_binding),
        sections: vec!["header".to_string(), "metadata".to_string()],
        cells: vec![CellBinding {
            field: "related".to_string(),
            slot: "related_items".to_string(),
        }],
        actions: vec![command_ref("update"), command_ref("delete")],
        span_ref: Some(SpanRef { start: 10, end: 42 }),
    });

    round_trip(&FilterCardinality::Multi);
    round_trip(&FilterDecl {
        name: "tags".to_string(),
        type_ref: "Text".to_string(),
        cardinality: FilterCardinality::Multi,
        url_sync: true,
        span_ref: Some(SpanRef { start: 44, end: 60 }),
    });
}

#[test]
fn terminal_search_sort_selection_and_setting_ir_round_trip() {
    let filter_binding = BindingRef::Filter {
        name: "slug".to_string(),
    };
    let source_binding = BindingRef::SourceInput {
        name: "q".to_string(),
    };
    round_trip(&filter_binding);
    round_trip(&source_binding);
    round_trip(&BindingRef::SelectionScalar);

    round_trip(&SearchMode::Columns {
        columns: vec!["title".to_string()],
    });
    round_trip(&SearchMode::Segmented);
    round_trip(&SearchField {
        key: "slug".to_string(),
        binds_to: filter_binding.clone(),
        span_ref: Some(SpanRef { start: 70, end: 88 }),
    });
    round_trip(&SearchDecl {
        mode: SearchMode::Segmented,
        fields: vec![SearchField {
            key: "slug".to_string(),
            binds_to: filter_binding,
            span_ref: None,
        }],
        free_text_target: Some(source_binding),
        span_ref: Some(SpanRef { start: 62, end: 96 }),
    });

    round_trip(&SortDir::Desc);
    round_trip(&SortDecl {
        allowed: vec!["title".to_string(), "updated".to_string()],
        default_field: "updated".to_string(),
        default_dir: SortDir::Desc,
        span_ref: Some(SpanRef {
            start: 100,
            end: 120,
        }),
    });

    round_trip(&SelectionMode::Multi);
    round_trip(&SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![command_ref("delete")],
        span_ref: Some(SpanRef {
            start: 130,
            end: 150,
        }),
    });

    round_trip(&SettingValueSpace::Enum {
        values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
    });
    round_trip(&SettingValueSpace::Bool);
    round_trip(&SettingValueSpace::Int { min: 1, max: 12 });
    round_trip(&SettingPersistence::Workspace);
    round_trip(&SettingDecl {
        name: "grid_size".to_string(),
        value_space: SettingValueSpace::Enum {
            values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
        },
        default: "sm".to_string(),
        persistence: SettingPersistence::Local,
        span_ref: Some(SpanRef {
            start: 160,
            end: 190,
        }),
    });
}

#[test]
fn terminal_tagged_enums_use_kind_discriminators() {
    assert_eq!(
        serde_json::to_value(ListRender::Cells {
            slot: "item_card".to_string()
        })
        .expect("serialize list render"),
        json!({ "kind": "cells", "slot": "item_card" })
    );
    assert_eq!(
        serde_json::to_value(SearchMode::Columns {
            columns: vec!["title".to_string()]
        })
        .expect("serialize search mode"),
        json!({ "kind": "columns", "columns": ["title"] })
    );
    assert_eq!(
        serde_json::to_value(BindingRef::SelectionScalar).expect("serialize binding ref"),
        json!({ "kind": "selection_scalar" })
    );
    assert_eq!(
        serde_json::to_value(SettingValueSpace::Int { min: 1, max: 9 })
            .expect("serialize setting value space"),
        json!({ "kind": "int", "min": 1, "max": 9 })
    );
}

#[test]
fn terminal_optional_and_vec_fields_default_and_skip() {
    let search_json = r#"{"mode":{"kind":"segmented"}}"#;
    let search: SearchDecl = serde_json::from_str(search_json).expect("deserialize search");
    assert_eq!(search.fields, Vec::<SearchField>::new());
    assert_eq!(search.free_text_target, None);
    assert_eq!(search.span_ref, None);
    assert_eq!(
        serde_json::to_value(search).expect("serialize search"),
        json!({ "mode": { "kind": "segmented" } })
    );

    let drawer_json = r#"{
        "name":"item_detail",
        "trigger":"select",
        "source":{"feature":"item","kind":"lookup","name":"by_id"}
    }"#;
    let drawer: DrawerSubView = serde_json::from_str(drawer_json).expect("deserialize drawer");
    assert_eq!(drawer.route_binding, None);
    assert!(drawer.sections.is_empty());
    assert!(drawer.cells.is_empty());
    assert!(drawer.actions.is_empty());
    assert_eq!(drawer.span_ref, None);
    assert_eq!(
        serde_json::to_value(drawer).expect("serialize drawer"),
        json!({
            "name": "item_detail",
            "trigger": "select",
            "source": { "feature": "item", "kind": "lookup", "name": "by_id" }
        })
    );
}

#[test]
fn on_success_spec_round_trips_and_skips_empty_slots() {
    let spec = OnSuccessSpec {
        back: true,
        redirect: Some("/host/property/{result.id}".to_owned()),
        flash: Some(FlashSpec {
            kind: "success".to_owned(),
            message_key: TranslationKeyRef {
                key: "saved".to_owned(),
                span_ref: None,
            },
        }),
        invalidates: vec![InvalidatesSpec {
            query: QualifiedName {
                feature: Some("host".to_owned()),
                name: "lookup_my_host".to_owned(),
            },
            args: vec![],
        }],
        replace: false,
    };
    round_trip(&spec);
    let value = serde_json::to_value(&spec).expect("serialize on_success");
    assert_eq!(value["back"], json!(true));
    assert_eq!(value["redirect"], json!("/host/property/{result.id}"));
    assert_eq!(value["flash"]["message_key"]["key"], json!("saved"));
    assert_eq!(
        value["invalidates"][0]["query"]["name"],
        json!("lookup_my_host")
    );
    assert!(value.get("replace").is_none());
}

// -------------------------------------------------------------------
// ir-auth-refresh-rotation §3 — TheftAction + RotationConfig + AuthSessions
// resolver methods. See docs/proposals/ir-auth-refresh-rotation.md.
// -------------------------------------------------------------------

fn user_session_qn() -> QualifiedName {
    QualifiedName {
        feature: Some("account".to_string()),
        name: "UserSession".to_string(),
    }
}

fn legacy_sessions() -> AuthSessions {
    AuthSessions {
        resource: user_session_qn(),
        ttl: "7 days".to_string(),
        refresh: false,
        extra_columns: Vec::new(),
        access_ttl: None,
        rotation: None,
    }
}

fn rotation_sessions(rotation: RotationConfig) -> AuthSessions {
    AuthSessions {
        resource: user_session_qn(),
        ttl: "7 days".to_string(),
        refresh: false,
        extra_columns: Vec::new(),
        access_ttl: None,
        rotation: Some(rotation),
    }
}

#[test]
fn theft_action_round_trip_and_default() {
    round_trip(&TheftAction::RevokeSessionFamily);
    round_trip(&TheftAction::RevokeUser);
    assert_eq!(TheftAction::default(), TheftAction::RevokeSessionFamily);

    // snake_case at the wire — required for parity with .lzi keyword.
    assert_eq!(
        serde_json::to_value(TheftAction::RevokeSessionFamily).unwrap(),
        json!("revoke_session_family")
    );
    assert_eq!(
        serde_json::to_value(TheftAction::RevokeUser).unwrap(),
        json!("revoke_user")
    );
}

#[test]
fn rotation_config_round_trips_with_all_slots() {
    round_trip(&RotationConfig {
        refresh_ttl: Some("30 days".to_string()),
        grace: Some("30 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeSessionFamily),
        span_ref: Some(SpanRef {
            start: 100,
            end: 200,
        }),
    });
}

#[test]
fn rotation_config_round_trips_with_all_slots_absent() {
    // Empty rotation block: presence = enabled, all defaults kick in.
    round_trip(&RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    });
}

#[test]
fn rotation_config_omits_none_fields_when_serialized() {
    let cfg = RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    };
    let v = serde_json::to_value(&cfg).unwrap();
    // Author wrote `rotation` block with no inner slots — the JSON
    // must be an empty object, not e.g. {"refresh_ttl": null, ...}.
    assert_eq!(v, json!({}));
}

#[test]
fn auth_sessions_legacy_back_compat_deserializes() {
    // Pre-this-cell fixtures lack access_ttl + rotation. Confirm they
    // still deserialize cleanly with the new fields defaulting to None.
    let legacy_json = json!({
        "resource": { "feature": "account", "name": "UserSession" },
        "ttl": "7 days",
        "refresh": false
    });
    let parsed: AuthSessions =
        serde_json::from_value(legacy_json).expect("legacy fixture must deserialize");
    assert_eq!(parsed.ttl, "7 days");
    assert!(!parsed.refresh);
    assert!(parsed.access_ttl.is_none());
    assert!(parsed.rotation.is_none());
    assert!(!parsed.is_rotation_enabled());
}

#[test]
fn auth_sessions_resolves_legacy_ttl_when_neither_set() {
    let s = legacy_sessions();
    assert_eq!(s.resolved_access_ttl(), "7 days");
    assert_eq!(s.resolved_refresh_ttl(), None);
    assert_eq!(s.resolved_rotation_grace(), None);
    assert_eq!(s.resolved_theft_action(), None);
}

#[test]
fn auth_sessions_resolves_framework_defaults_when_rotation_on() {
    let s = rotation_sessions(RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    });
    assert!(s.is_rotation_enabled());
    // access_ttl=None + rotation on => "15 minutes" framework default.
    assert_eq!(s.resolved_access_ttl(), "15 minutes");
    assert_eq!(s.resolved_refresh_ttl(), Some("30 days"));
    assert_eq!(s.resolved_rotation_grace(), Some("30 seconds"));
    assert_eq!(
        s.resolved_theft_action(),
        Some(TheftAction::RevokeSessionFamily)
    );
}

#[test]
fn auth_sessions_resolves_explicit_values_when_set() {
    let mut s = rotation_sessions(RotationConfig {
        refresh_ttl: Some("14 days".to_string()),
        grace: Some("60 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeUser),
        span_ref: None,
    });
    s.access_ttl = Some("10 minutes".to_string());

    assert_eq!(s.resolved_access_ttl(), "10 minutes");
    assert_eq!(s.resolved_refresh_ttl(), Some("14 days"));
    assert_eq!(s.resolved_rotation_grace(), Some("60 seconds"));
    assert_eq!(s.resolved_theft_action(), Some(TheftAction::RevokeUser));
}

#[test]
fn auth_sessions_resolves_access_ttl_falls_back_to_legacy_when_rotation_off() {
    let mut s = legacy_sessions();
    s.access_ttl = None;
    // Rotation off, access_ttl not set → legacy ttl.
    assert_eq!(s.resolved_access_ttl(), "7 days");

    s.access_ttl = Some("3 hours".to_string());
    // Rotation off, access_ttl explicit → explicit value wins.
    assert_eq!(s.resolved_access_ttl(), "3 hours");
}

#[test]
fn auth_sessions_round_trips_with_nested_rotation_block() {
    round_trip(&rotation_sessions(RotationConfig {
        refresh_ttl: Some("30 days".to_string()),
        grace: Some("30 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeSessionFamily),
        span_ref: None,
    }));
}

// -------------------------------------------------------------------
// ir-route-guards §3 — ViewGuard + RouteGuardDefaults + view.guard
// slots on the 4 view kinds. See docs/proposals/ir-route-guards.md.
// -------------------------------------------------------------------

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
