//! Regression test for the route-guard escape-hatch proposal
//! (`ir-route-guard-escape-hatch-2026-05-28`), Cell B-2 — Go codegen side.
//!
//! ## Why this test exists
//!
//! The proposal added three new slots to `ViewGuard`. Slot 3
//! (`requires <feature>.lookup_my.<field> = <literal>`) is a CLIENT-SIDE
//! gate: the TS codegen at
//! `crates/lazuli_codegen_ts/src/routes/emit/before_load.rs` emits a
//! `beforeLoad` that calls the existing `lookup_my_<resource>` query,
//! reads `<field>` off the returned struct, and redirects on mismatch.
//!
//! The Go codegen has **no new emit branch** for the slot — the
//! responsibility splits into two parts that the Go side ALREADY ships
//! out of the box:
//!
//!   1. The resource struct must expose `<field>` as a typed Go field.
//!      Handled by `emitter/resource/struct_emit.rs` for every authored
//!      `resource.<field>: <type>` line.
//!   2. The `lookup_my_<resource>` query must return the full resource
//!      struct so the client can read `<field>` off the JSON response.
//!      Handled by `emitter/query/lookup.rs` +
//!      `emitter/query/lookup_wrapper.rs` — the wrapper signature is
//!      `func LookupMyX(ctx *lazuli.Ctx) (X, error)` for `conventions [me]`
//!      synths.
//!
//! This test pins both pieces against a fixture whose shape mirrors the
//! hostpoint `requires user.lookup_my.is_phone_verified = true` site
//! the proposal calls out (§5.5 defense-in-depth pair). If either piece
//! ever regresses, the client-side gate emitted by `before_load.rs`
//! would compile-fail or read a non-existent field — caught here instead
//! of at pilot integration time.
//!
//! See also: `crates/lazuli_codegen_go/src/lib.rs` module docs
//! ("Route-guard escape hatch") for the broader rationale.

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::Module;

/// Canonical capsule mirroring the hostpoint pattern: a `user` feature
/// declares a `User` resource with `is_phone_verified: Bool` (the field
/// every Shape-C guard in hostpoint references today) plus a
/// `query.lookup lookup_my_user` synth that exposes the row to the
/// `beforeLoad` guard via `LookupMyUser(ctx) (User, error)`.
const USER_FEATURE: &str = r#"feature user
  domain
    resource User
      is_phone_verified: Bool required
      created_at: DateTime required

  query.lookup lookup_my_user
"#;

fn parsed_module(source: &str) -> Module {
    let features = lazuli_syntax::parse_feature_skeletons(source)
        .expect("user feature source should parse")
        .into_iter()
        .map(|feature| {
            lazuli_analyzer::lower_feature_skeleton(&feature)
                .expect("user feature source should lower")
        })
        .collect();
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

#[test]
fn lookup_my_user_query_exposes_is_phone_verified_field_to_client_guard() {
    let module = parsed_module(USER_FEATURE);

    // Sanity — the analyzer lifted the resource + query the rest of
    // the test inspects. Without this, a silent skeleton-parse change
    // could produce a "no resource emitted" failure instead of the
    // actual diagnostic.
    let feature = &module.features[0];
    assert_eq!(feature.name, "user");
    assert_eq!(
        feature.resources.len(),
        1,
        "expected one resource (User) lifted from the capsule; got {:#?}",
        feature.resources,
    );
    assert_eq!(feature.resources[0].name, "User");
    assert_eq!(
        feature.queries.len(),
        1,
        "expected one query (lookup_my_user) lifted from the capsule; got {:#?}",
        feature.queries,
    );

    let files = generate_v1(&module, &GoEmitOptions::default());

    // Part 1 — the resource struct must carry the boolean field. The
    // proposal's Shape-C client-side guard reads `row.is_phone_verified`
    // off the JSON response; if the Go struct omits it the JSON shape
    // diverges and the `beforeLoad` redirect logic silently no-ops.
    let resource_file = files
        .iter()
        .find(|f| f.path == "user/resource.gen.go")
        .unwrap_or_else(|| {
            panic!(
                "expected user/resource.gen.go to be emitted; got {:#?}",
                files.iter().map(|f| &f.path).collect::<Vec<_>>(),
            )
        });
    assert!(
        resource_file.contents.contains("IsPhoneVerified"),
        "User struct must expose the `IsPhoneVerified` field (the typed \
         landing slot for the client-side Shape-C guard); got:\n{}",
        resource_file.contents,
    );
    assert!(
        resource_file
            .contents
            .contains("json:\"is_phone_verified\""),
        "User struct field must carry the `is_phone_verified` JSON tag — \
         the client guard reads `row.is_phone_verified` off the wire \
         response; got:\n{}",
        resource_file.contents,
    );

    // Part 2 — the `lookup_my_user` query wrapper must return the full
    // `User` resource type so the client gate has the row to read.
    // Both wrapper shapes (actor-keyed `conventions [me]` synth or the
    // bare `query.lookup lookup_my_user` skeleton form used by this
    // fixture) share the `(User, error)` return — that's the load-bearing
    // contract the Shape-C client guard depends on.
    let query_file = files
        .iter()
        .find(|f| f.path == "user/query.gen.go")
        .unwrap_or_else(|| {
            panic!(
                "expected user/query.gen.go to be emitted; got {:#?}",
                files.iter().map(|f| &f.path).collect::<Vec<_>>(),
            )
        });
    assert!(
        query_file.contents.contains("func LookupMyUser(")
            && query_file.contents.contains(") (User, error) {"),
        "lookup_my_user wrapper must return `(User, error)` so the client \
         Shape-C gate can read `is_phone_verified` off the resolved row; \
         got:\n{}",
        query_file.contents,
    );

    // Negative — confirm no surprise Go-side route-guard scaffold leaked
    // in. The proposal's §5 explicitly carves out route guards as
    // TS-only; if a future change tries to materialise them in Go,
    // expect this assertion to flag it so the doctor + analyzer story
    // can be revisited intentionally.
    for file in &files {
        assert!(
            !file.contents.contains("requires_field"),
            "no Go-side artefact should mention `requires_field` — \
             route guards are client-driven only; got drift in {}:\n{}",
            file.path,
            file.contents,
        );
        assert!(
            !file.contents.contains("ViewGuard"),
            "no Go-side artefact should mention `ViewGuard` — \
             route guards are client-driven only; got drift in {}:\n{}",
            file.path,
            file.contents,
        );
    }
}
