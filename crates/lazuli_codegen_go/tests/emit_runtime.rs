//! Snapshot test: emit the customer spike feature and compare to the
//! committed `dist/go/customer/customer.gen.go`. Drift means either the
//! emitter or the spec has diverged from the proven runtime form, and
//! whichever is wrong needs to be fixed.

use lazuli_codegen_go::emit_feature_go;
use lazuli_codegen_spec::customer_spike;

#[test]
fn emit_matches_dist_go_customer_spike() {
    let actual = emit_feature_go(&customer_spike());

    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("cargo sets CARGO_MANIFEST_DIR");
    let path = workspace_root
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("dist/go/customer/customer.gen.go");

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));

    if actual != expected {
        // Print a unified-ish diff so the test failure shows the first
        // difference cleanly.
        for (i, (a, b)) in actual.lines().zip(expected.lines()).enumerate() {
            if a != b {
                panic!(
                    "first diff at line {}:\n  emitter:  {a:?}\n  expected: {b:?}",
                    i + 1
                );
            }
        }
        let actual_lines = actual.lines().count();
        let expected_lines = expected.lines().count();
        panic!(
            "outputs share their common prefix but lengths differ: emitter={actual_lines}, expected={expected_lines}"
        );
    }
}
