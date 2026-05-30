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
