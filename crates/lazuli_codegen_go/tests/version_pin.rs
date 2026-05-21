//! Source-of-truth parity test for the Lazuli Go version constant.
//!
//! The default version constraint emitted in the generated `go.mod`'s
//! `require lazuli.dev/runtime` line lives at
//! `lazuli_codegen_go::LAZULI_GO_VERSION`. The Lazuli Go lib itself
//! is at `runtime/go/` with its canonical version pinned in
//! `runtime/go/VERSION` (single-line text file). These two must
//! never drift — a Lazuli Go release retags the Go module AND bumps
//! the Rust const in the same change, with this test as the gate.

use lazuli_codegen_go::LAZULI_GO_VERSION;

const VERSION_FILE: &str = include_str!("../../../runtime/go/VERSION");

#[test]
fn lazuli_go_version_const_matches_runtime_pin() {
    let runtime_version = VERSION_FILE.trim();
    assert_eq!(
        LAZULI_GO_VERSION, runtime_version,
        "LAZULI_GO_VERSION (`{LAZULI_GO_VERSION}`) drifted from runtime/go/VERSION (`{runtime_version}`). \
         Bump both together when releasing the Lazuli Go lib."
    );
}

#[test]
fn runtime_version_pin_is_valid_semver_tag() {
    let v = VERSION_FILE.trim();
    assert!(
        v.starts_with('v'),
        "runtime/go/VERSION must start with `v` (Go module tag convention); got `{v}`"
    );
    let body = &v[1..];
    let parts: Vec<&str> = body.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "runtime/go/VERSION must be `vMAJOR.MINOR.PATCH`; got `{v}`"
    );
    for (label, part) in ["major", "minor", "patch"].iter().zip(parts.iter()) {
        assert!(
            part.parse::<u32>().is_ok(),
            "{label} segment in `{v}` is not a valid u32"
        );
    }
}
