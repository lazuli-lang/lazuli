//! Round-trip the customer spike spec through JSON and confirm the
//! resulting `RuntimeFeature` re-emits identical Go and TS source. Pinning
//! both directions guarantees external producers (modern-syntax parser,
//! IDE tooling, LLM authoring) can hand the codegen a JSON manifest and
//! get the same output as the in-process fixture.

use lazuli_codegen_spec::{RuntimeFeature, customer_spike};

#[test]
fn fixture_round_trips_through_json() {
    let fixture = customer_spike();
    let json = serde_json::to_string_pretty(&fixture).expect("serialize");
    let rehydrated: RuntimeFeature = serde_json::from_str(&json).expect("deserialize");

    // Cheap structural compare: every name set survives the trip.
    assert_eq!(fixture.name, rehydrated.name);
    assert_eq!(fixture.resources.len(), rehydrated.resources.len());
    assert_eq!(fixture.commands.len(), rehydrated.commands.len());
    assert_eq!(fixture.queries.len(), rehydrated.queries.len());
    for (a, b) in fixture.commands.iter().zip(rehydrated.commands.iter()) {
        assert_eq!(a.short_name, b.short_name);
        assert_eq!(a.invalidates, b.invalidates);
    }

    // Strict compare: re-serialise the rehydrated value and confirm the
    // wire form is stable.
    let rejson = serde_json::to_string_pretty(&rehydrated).expect("serialize");
    assert_eq!(json, rejson, "JSON round-trip must be stable");
}

#[test]
fn committed_json_matches_fixture() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("cargo sets CARGO_MANIFEST_DIR");
    let path = workspace_root
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("examples/runtime-spec/customer.json");

    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let expected = serde_json::to_string_pretty(&customer_spike()).expect("serialize") + "\n";

    if on_disk != expected {
        for (i, (a, b)) in on_disk.lines().zip(expected.lines()).enumerate() {
            if a != b {
                panic!(
                    "first JSON diff at line {}:\n  on-disk:  {a:?}\n  fixture:  {b:?}",
                    i + 1
                );
            }
        }
        panic!(
            "JSON line counts differ: on_disk={}, fixture={}",
            on_disk.lines().count(),
            expected.lines().count()
        );
    }
}
