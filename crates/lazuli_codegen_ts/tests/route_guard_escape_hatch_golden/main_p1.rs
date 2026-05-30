/// Build a minimal `host` feature carrying `Host` resource +
/// `lookup_my_host` query + `lifecycle_routes` table — the surface
/// `resolve_lifecycle_in_emit` + `resolve_field_gate_emit` need.
///
/// Built via parse-then-lower (mirrors `route_guard_golden.rs`) so the
/// fixture survives Feature struct evolution.
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
    feature.resources.push(host_resource());
    feature
}

/// Construct a minimal `Host` resource via JSON — the only IR-side
/// shape that has to be sufficient for `resolve_lifecycle_in_emit` is
/// `name` + `lifecycle_routes` (the resolver only inspects those two).
/// Field-level slots stay empty.
fn host_resource() -> lazuli_ir::Resource {
    lazuli_ir::Resource {
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
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    }
}

/// Build a minimal `user` feature with `lookup_my_user`. Used by
/// field-gate tests for `requires user.lookup_my.is_phone_verified`.
fn user_feature() -> Feature {
    let source = r#"
feature user
  query.lookup lookup_my_user
"#;
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(source).expect("user skeleton parses");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("user feature lowers")
}

fn host_surface() -> PlatformSurface {
    PlatformSurface {
        experience: "host".to_string(),
        platform: Platform::Web,
        uses_experience: None,
        audiences: Vec::new(),
        span_ref: None,
    }
}

fn host_experience() -> Experience {
    Experience {
        name: "host".to_string(),
        imports: Vec::new(),
        views: Vec::new(),
        resume_routers: Vec::new(),
        extensions: Vec::new(),
        span_ref: None,
    }
}

/// Build an [`AppRoute`] from a JSON literal so each test case stays
/// readable as a single block.
fn route_from_json(value: serde_json::Value) -> AppRoute {
    serde_json::from_value(value).expect("route fixture parses")
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}; got {files:#?}"))
        .contents
        .as_str()
}

/// Render the routes artifact for the host audience with the given
/// route slice + feature catalog.
fn render(routes: &[AppRoute], features: &[Feature]) -> String {
    let surfaces = vec![host_surface()];
    let experiences = vec![host_experience()];
    let files = emit_routes_artifacts(
        None,
        routes,
        &surfaces,
        &experiences,
        features,
        RoutesTarget::Web,
    );
    file(&files, "dist/ts-web/host/routes.gen.tsx").to_owned()
}

// ---------------------------------------------------------------------
// 1. Allow-list lifecycle — `requires_lifecycle_in Host [s1, s2]`
// ---------------------------------------------------------------------

#[test]
fn requires_lifecycle_in_emits_allow_list_membership_check() {
    let route = route_from_json(serde_json::json!({
        "name": "host_basic_details",
        "path": "/onboarding/host/basic-details",
        "to": "host.view.host_basic_details",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "requires_lifecycle_in": {
                "resource": "Host",
                "allowed_states": ["basic_details_pending", "address_pending"]
            }
        }
    }));

    let out = render(&[route], &[host_feature()]);

    // Allow-list literal is preserved in authored order.
    assert!(
        out.contains("const __allowedStates = [\"basic_details_pending\", \"address_pending\"] as const;"),
        "allow-list array literal missing or out of order\n---\n{out}\n---",
    );
    // Membership check via Array.includes (with null short-circuit).
    assert!(
        out.contains("if (__state === null || !__allowedStates.includes(__state as typeof __allowedStates[number]))"),
        "Array.includes membership check missing\n---\n{out}\n---",
    );
    // Redirect routes through the lifecycle helper (same as exact-match).
    assert!(
        out.contains("throw redirect({ to: hostLifecycleRoute(__state) });"),
        "lifecycle helper redirect missing\n---\n{out}\n---",
    );
    // Exact-match equality check MUST NOT appear when allow-list is used.
    assert!(
        !out.contains("if (__state !== "),
        "equality fallback leaked into allow-list emit\n---\n{out}\n---",
    );
}

#[test]
fn requires_lifecycle_exact_match_emit_unchanged_back_compat() {
    let route = route_from_json(serde_json::json!({
        "name": "host_basic_details",
        "path": "/onboarding/host/basic-details",
        "to": "host.view.host_basic_details",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "requires_lifecycle": {
                "resource": "Host",
                "state": "basic_details_pending"
            }
        }
    }));

    let out = render(&[route], &[host_feature()]);

    // Pre-Cell-B-1 equality-check shape, byte-identical.
    assert!(
        out.contains("if (__state !== \"basic_details_pending\") {"),
        "exact-match equality check changed shape\n---\n{out}\n---",
    );
    assert!(
        out.contains("throw redirect({ to: hostLifecycleRoute(__state) });"),
        "exact-match helper redirect missing\n---\n{out}\n---",
    );
    // Cell B-1 markers MUST NOT appear in legacy emit.
    assert!(
        !out.contains("__allowedStates"),
        "allow-list machinery leaked into exact-match emit\n---\n{out}\n---",
    );
    assert!(
        !out.contains("__fieldRow"),
        "field-gate machinery leaked into exact-match emit\n---\n{out}\n---",
    );
}

// ---------------------------------------------------------------------
// 2. Field gate — `requires user.lookup_my.is_phone_verified = true`
// ---------------------------------------------------------------------

#[test]
fn requires_field_emits_lookup_fetch_and_mismatch_redirect() {
    let route = route_from_json(serde_json::json!({
        "name": "host_address",
        "path": "/onboarding/host/address",
        "to": "host.view.host_address",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "requires_field": [
                {
                    "feature": "user",
                    "field": "is_phone_verified",
                    "expected": { "kind": "Boolean", "value": true },
                    "on_unmet_redirect": "/onboarding/host/phone-verification"
                }
            ]
        }
    }));

    let out = render(&[route], &[host_feature(), user_feature()]);

    // SDK import for the user feature carrying the lookup query.
    assert!(
        out.contains("import { lookupMyUser } from \"../user/user.gen.js\";"),
        "user SDK import missing for field-gate lookup\n---\n{out}\n---",
    );
    // Field-gate fetch + mismatch redirect.
    assert!(
        out.contains("const __fieldRow0 = await params.context.queryClient.fetchQuery({"),
        "field-gate fetchQuery missing\n---\n{out}\n---",
    );
    assert!(
        out.contains("queryKey: queryKeyFor(lookupMyUser, {})"),
        "field-gate queryKey missing\n---\n{out}\n---",
    );
    // The IR's snake_case `field` is camelCased before emit so the
    // runtime access matches the SDK row shape (`isPhoneVerified`, not
    // `is_phone_verified`). See `resolve.rs::snake_to_lower_camel`.
    assert!(
        out.contains("if (__fieldRow0.isPhoneVerified !== true) {"),
        "field-gate mismatch check missing or not camelCased\n---\n{out}\n---",
    );
    assert!(
        !out.contains("is_phone_verified"),
        "snake_case field name leaked into emit (would read undefined on SDK rows)\n---\n{out}\n---",
    );
    assert!(
        out.contains("}) as { isPhoneVerified?: unknown };"),
        "row-shape narrowing cast not camelCased\n---\n{out}\n---",
    );
    assert!(
        out.contains("throw redirect({ to: \"/onboarding/host/phone-verification\" });"),
        "field-gate redirect missing\n---\n{out}\n---",
    );
}
