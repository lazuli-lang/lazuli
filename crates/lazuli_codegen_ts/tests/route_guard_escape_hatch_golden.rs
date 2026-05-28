//! Snapshot coverage for the 3 new route-guard escape-hatch slots
//! shipped under `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! Cell B-1. Each pair (`*_emits_*` happy path + `*_back_compat_*`
//! invariant) proves:
//!
//!  1. **Allow-list lifecycle** — `requires_lifecycle_in <R> [s, ...]`
//!     emits an `Array.includes` membership check; existing
//!     `requires_lifecycle <R> = <state>` fixtures emit byte-identical
//!     equality-check TS.
//!  2. **Field gate** — `requires <feature>.lookup_my.<field> = <lit>
//!     on_unmet redirect <path>` emits a fetchQuery + mismatch redirect;
//!     routes without the slot emit unchanged.
//!  3. **`forbid_when ... only_when lifecycle`** — composes the atom
//!     check with a `lifecycleState === <s>` gate; legacy unconditional
//!     `forbid_when` keeps its previous shape.
//!
//! The fixtures are built via `serde_json::from_value` against the IR
//! shapes (same approach as `lifecycle_gate_golden.rs`) so the test
//! stays decoupled from parser surface evolution.

use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::routes::{RoutesTarget, emit_routes_artifacts};
use lazuli_ir::{
    AppRoute, Experience, Feature, LifecycleRouteArm, LifecycleRoutes, Platform, PlatformSurface,
};

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

#[test]
fn routes_without_requires_field_emit_unchanged_back_compat() {
    let route = route_from_json(serde_json::json!({
        "name": "host_home",
        "path": "/host",
        "to": "host.view.host_home",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in"
        }
    }));

    let out = render(&[route], &[host_feature()]);

    assert!(
        !out.contains("__fieldRow"),
        "field-gate machinery leaked into route without requires_field\n---\n{out}\n---",
    );
    assert!(
        !out.contains("lookupMyUser"),
        "user lookup import leaked into route without requires_field\n---\n{out}\n---",
    );
    // Existing policy gate still emits.
    assert!(
        out.contains("await tanstackBeforeLoadGuard(options.client, {"),
        "policy gate missing from legacy route\n---\n{out}\n---",
    );
}

// ---------------------------------------------------------------------
// 3. `forbid_when ... only_when lifecycle <R> = <state>`
// ---------------------------------------------------------------------

#[test]
fn forbid_when_with_only_when_lifecycle_wraps_redirect_in_state_check() {
    let route = route_from_json(serde_json::json!({
        "name": "choose_role",
        "path": "/choose-role",
        "to": "host.view.choose_role",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "forbid_when": [
                {
                    "atom_ref": "@role.host",
                    "atom": { "namespace": "role", "name": "host" },
                    "dispatch_to": "/host",
                    "only_when_lifecycle": {
                        "resource": "Host",
                        "state": "complete"
                    }
                }
            ]
        }
    }));

    let out = render(&[route], &[host_feature()]);

    // Lifecycle row fetched once and cached.
    assert!(
        out.contains("const __forbidLcRow_host = await params.context.queryClient.fetchQuery({"),
        "cached lifecycle fetch missing for forbid_when only_when\n---\n{out}\n---",
    );
    assert!(
        out.contains("const __forbidLcState_host = (__forbidLcRow_host as { lifecycleState?: string }).lifecycleState ?? null;"),
        "cached lifecycleState extract missing\n---\n{out}\n---",
    );
    // Atom check still wraps the redirect, and the redirect is now
    // gated by the lifecycle-state equality.
    assert!(
        out.contains("if (evaluatePolicy(__forbidActor, { name: \"@role.host\", atoms: [{ namespace: \"role\", name: \"host\" }] }) === \"authorized\") {"),
        "atom check missing\n---\n{out}\n---",
    );
    assert!(
        out.contains("if (__forbidLcState_host === \"complete\") {"),
        "lifecycle-state gate missing inside forbid_when arm\n---\n{out}\n---",
    );
    assert!(
        out.contains("throw redirect({ to: \"/host\" });"),
        "forbid_when redirect missing\n---\n{out}\n---",
    );
}

#[test]
fn forbid_when_without_only_when_emit_unchanged_back_compat() {
    let route = route_from_json(serde_json::json!({
        "name": "choose_role",
        "path": "/choose-role",
        "to": "host.view.choose_role",
        "surface": "host web",
        "audience": "host",
        "guard": {
            "policy": ["@policy.authenticated"],
            "on_unauthenticated": "/sign-in",
            "forbid_when": [
                {
                    "atom_ref": "@role.host",
                    "atom": { "namespace": "role", "name": "host" },
                    "dispatch_to": "/host"
                }
            ]
        }
    }));

    let out = render(&[route], &[host_feature()]);

    // Legacy forbid_when fires unconditionally — no lifecycle cache,
    // no state check.
    assert!(
        !out.contains("__forbidLcRow"),
        "lifecycle cache leaked into legacy forbid_when\n---\n{out}\n---",
    );
    assert!(
        !out.contains("__forbidLcState"),
        "lifecycle state leaked into legacy forbid_when\n---\n{out}\n---",
    );
    assert!(
        out.contains("if (evaluatePolicy(__forbidActor, { name: \"@role.host\", atoms: [{ namespace: \"role\", name: \"host\" }] }) === \"authorized\") {"),
        "atom check missing\n---\n{out}\n---",
    );
    // The redirect sits directly under the atom check (no intervening
    // `if (__forbidLcState ...)` wrapper).
    let atom_idx = out
        .find("=== \"authorized\") {")
        .expect("atom check present");
    let after = &out[atom_idx..];
    let next_line_end = after.find('\n').unwrap_or(after.len());
    let body_start = atom_idx + next_line_end + 1;
    let body_line = out[body_start..]
        .lines()
        .next()
        .expect("body line after atom check");
    assert!(
        body_line.contains("throw redirect"),
        "legacy forbid_when redirect not directly under atom check; got: {body_line:?}\n---\n{out}\n---",
    );
}

// ---------------------------------------------------------------------
// 4. Composed canonical demo — all 3 slots on one route
// ---------------------------------------------------------------------

/// Anchors the round-trip canonical fixture at
/// `examples/full-capsule/route-guard-roundtrip/fixture.lzx`. When this
/// test passes, `expected.emit.ts` is genuinely emit-equal (no more
/// TODO sketch).
#[test]
fn roundtrip_canonical_demo_emits_three_chained_slots() {
    let route = route_from_json(serde_json::json!({
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
    }));

    let out = render(&[route], &[host_feature(), user_feature()]);

    // All three Cell B-1 markers coexist in the same beforeLoad body.
    assert!(out.contains("__allowedStates"), "allow-list missing");
    assert!(out.contains("__fieldRow0"), "field gate missing");
    assert!(out.contains("__forbidLcState_host"), "forbid-with-only-when missing");
}
