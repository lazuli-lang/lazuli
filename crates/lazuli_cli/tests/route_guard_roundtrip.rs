//! Round-trip authoring fixture for the route-guard escape-hatch
//! surface added by
//! `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md` §4.5.
//!
//! Cell A acceptance: the canonical `.lzx` fixture under
//! `examples/full-capsule/route-guard-roundtrip/` must parse, the
//! resulting IR JSON must match the committed `expected.ir.json`
//! snapshot, AND a re-parse of the snapshot (proving the IR shape
//! is closed under reconstruction) must produce the same IR JSON.
//!
//! Because `lazuli emit lzx` does not exist at HEAD, this test
//! implements the degraded variant described in §4.5: parse → IR
//! → re-deserialize from IR JSON → byte-equal IR JSON. The full
//! `.lzx → IR → .lzx → IR → byte-equal` byte-equal variant is a
//! follow-up cell tracked alongside the emit-lzx work.

use std::path::PathBuf;
use std::process::Command;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `crates/lazuli_cli`. The repo root
    // is the grandparent.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("workspace root")
}

fn fixture_dir() -> PathBuf {
    workspace_root()
        .join("examples")
        .join("full-capsule")
        .join("route-guard-roundtrip")
}

/// Strip the `ir.source` path from the inspect envelope so the
/// snapshot stays cwd-stable. The path is the raw `.lzx` location
/// passed to `lazuli inspect`; both call sites here pass the same
/// relative path so they match in practice, but the helper is
/// retained for clarity and future cells.
fn canonicalise(json: serde_json::Value) -> serde_json::Value {
    let mut v = json;
    if let Some(ir) = v.get_mut("ir").and_then(|x| x.as_object_mut()) {
        ir.remove("source");
    }
    v
}

#[test]
fn route_guard_roundtrip_matches_committed_snapshot() {
    let fixture = fixture_dir().join("fixture.lzx");
    let expected_path = fixture_dir().join("expected.ir.json");
    let expected_raw = std::fs::read_to_string(&expected_path)
        .expect("read expected.ir.json (commit the snapshot at examples/full-capsule/route-guard-roundtrip/expected.ir.json)");
    let expected: serde_json::Value =
        serde_json::from_str(&expected_raw).expect("expected.ir.json is valid JSON");

    let output = Command::new(cli_bin())
        .args([
            "inspect",
            "--format=json",
            fixture.to_str().expect("fixture path utf-8"),
        ])
        .output()
        .expect("run lazuli inspect");
    assert!(
        output.status.success(),
        "lazuli inspect failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspect stdout is valid JSON");

    let actual = canonicalise(actual);
    let expected = canonicalise(expected);

    assert_eq!(
        actual, expected,
        "first-pass IR JSON drifted from expected.ir.json — update the snapshot if the drift is intentional"
    );
}

#[test]
fn route_guard_roundtrip_ir_shape_is_closed_under_re_deserialise() {
    // Pass 1: source → IR (via inspect).
    let fixture = fixture_dir().join("fixture.lzx");
    let output = Command::new(cli_bin())
        .args([
            "inspect",
            "--format=json",
            fixture.to_str().expect("fixture path utf-8"),
        ])
        .output()
        .expect("run lazuli inspect");
    assert!(output.status.success(), "lazuli inspect failed");
    let ir_1: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("first inspect JSON");

    // Pull the lzx routes block and lift through serde back into IR
    // structs. This proves the wire shape round-trips cleanly without
    // needing a `lazuli emit lzx` (which doesn't exist at HEAD).
    let routes_json = ir_1
        .get("ir")
        .and_then(|x| x.get("routes"))
        .cloned()
        .expect("ir.routes block present");
    let routes: Vec<lazuli_ir::AppRoute> =
        serde_json::from_value(routes_json).expect("routes deserialize as Vec<AppRoute>");

    // Cell A acceptance for the three new slots: confirm shape lands
    // in the IR variant fields (not silently dropped to "unknown" or
    // discarded). Sentinel checks per slot.
    let route = routes
        .iter()
        .find(|r| r.name == "roundtrip_canonical_demo")
        .expect("roundtrip_canonical_demo route");
    let guard = route.guard.as_ref().expect("guard");
    let allow = guard
        .requires_lifecycle_in
        .as_ref()
        .expect("requires_lifecycle_in survived round-trip");
    assert_eq!(allow.resource, "Host");
    assert_eq!(
        allow.allowed_states,
        vec!["basic_details_pending".to_string(), "address_pending".to_string()],
    );
    assert_eq!(guard.requires_field.len(), 1);
    let rf = &guard.requires_field[0];
    assert_eq!(rf.feature, "user");
    assert_eq!(rf.field, "is_phone_verified");
    assert_eq!(rf.on_unmet_redirect, "/demo/phone-verify");
    assert!(matches!(
        rf.expected,
        lazuli_ir::DefaultValue::Boolean(true)
    ));
    assert_eq!(guard.forbid_when.len(), 1);
    let fw = &guard.forbid_when[0];
    assert_eq!(fw.atom_ref, "@role.guest");
    let owl = fw
        .only_when_lifecycle
        .as_ref()
        .expect("only_when_lifecycle survived round-trip");
    assert_eq!(owl.resource, "Host");
    assert_eq!(owl.state, "complete");

    // Pass 2: re-serialise + re-deserialise the routes block. The
    // resulting JSON MUST be identical (modulo serde key ordering;
    // canonicalise via to_value).
    let ir_2 = serde_json::to_value(&routes).expect("re-serialise routes");
    let routes_back: Vec<lazuli_ir::AppRoute> =
        serde_json::from_value(ir_2.clone()).expect("re-deserialize routes");
    assert_eq!(
        routes, routes_back,
        "IR did not round-trip through serde re-deserialisation"
    );
}
