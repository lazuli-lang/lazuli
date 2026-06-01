// Spec 0025 — smart stubs (delegate-to-runtime starter bodies).
//
// These golden tests pin both halves of the contract:
//  - a stub whose SITE maps to a known runtime symbol emits the DELEGATING
//    runtime-call body (the `*_delegates` + `*_keeps_extension_stub_marker_*`
//    tests), and
//  - a stub whose site has NO table row emits today's `// IMPLEMENT ME` body
//    byte-for-byte unchanged (the `non_mapped_fn_still_plain_stub` back-compat
//    guard).
//
// Included from `tests.rs` (which provides `use super::*` + the IR imports and
// the `base_feature` / `extension` / `module_with_features` helpers).
//
// Test names contain `smart_stub` so `cargo test -p lazuli_codegen_go
// smart_stub` selects this file's golden set.

fn auth_password(hash: &str, verify: &str) -> Auth {
    Auth {
        identity: identity(),
        password: Some(AuthPassword {
            algorithm: "argon2id".to_owned(),
            hash: hash.to_owned(),
            verify: verify.to_owned(),
            rate_limit: None,
        }),
        sessions: None,
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    }
}

/// FLAGSHIP — the password.hash stub delegates to `auth.HashPassword`, wired to
/// the `<Feature>AuthPassword` contract var the auth emitter emits, and does NOT
/// emit `// IMPLEMENT ME` nor `not yet implemented`.
#[test]
fn smart_stub_password_hash_delegates() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(auth_password("@fn.hash_password", "@fn.verify_password"));
    feature.extensions.push(extension(
        "hash_password",
        ExtensionContract::Function {
            input: TypeRef::Builtin(BuiltinType::Text),
            output: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let hash = files
        .iter()
        .find(|file| file.path == "app/features/customer_auth/handlers/hash_password.go")
        .expect("hash stub emitted");

    // Delegating body: the contract var name is `<Feature>AuthPassword`, where
    // <Feature> = pascal_case("customer_auth") = "CustomerAuth", referenced
    // cross-package via the `customer_authgen` alias.
    assert!(
        hash.contents.contains(
            "hashed, err := auth.HashPassword(ctx, customer_authgen.CustomerAuthAuthPassword, input)"
        ),
        "expected delegating HashPassword call; got:\n{}",
        hash.contents
    );
    assert!(
        hash.contents
            .contains("return lazuli.HashedRef(hashed), nil"),
        "expected the runtime string returned as the @cap.Hashed output type; got:\n{}",
        hash.contents
    );

    // The empty-stub invitation is gone.
    assert!(!hash.contents.contains("// IMPLEMENT ME"));
    assert!(!hash.contents.contains("not yet implemented"));

    // Imports: the runtime auth pkg + the gen pkg (the contract var lives there).
    assert!(
        hash.contents
            .contains("\"lazuli.dev/runtime/lazuli/auth\""),
        "expected the runtime auth import; got:\n{}",
        hash.contents
    );
    assert!(
        hash.contents
            .contains("customer_authgen \"lazuli/test/customer_auth\""),
        "expected the gen import alias; got:\n{}",
        hash.contents
    );
}

/// G3 — the delegating stub stays user territory: same `//lazuli:pattern
/// extension_stub v1` marker on the fn + init, the `func init()` +
/// `lazuli.RegisterFn(...)` self-registration, and the "Lazuli will not
/// overwrite this file" header.
#[test]
fn smart_stub_keeps_extension_stub_marker_and_init() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(auth_password("@fn.hash_password", "@fn.verify_password"));
    feature.extensions.push(extension(
        "hash_password",
        ExtensionContract::Function {
            input: TypeRef::Builtin(BuiltinType::Text),
            output: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let hash = files
        .iter()
        .find(|file| file.path == "app/features/customer_auth/handlers/hash_password.go")
        .expect("hash stub emitted");

    // Still an extension_stub on BOTH the fn and init (the emitter lint requires
    // both); the marker is unchanged from the plain stub.
    assert_eq!(
        hash.contents
            .matches("//lazuli:pattern extension_stub v1")
            .count(),
        2,
        "expected the extension_stub marker on both fn and init; got:\n{}",
        hash.contents
    );
    // Still self-registers — the override point survives (spec G3 / §why-not-drop).
    assert!(hash.contents.contains("func init() {"));
    assert!(
        hash.contents
            .contains("lazuli.RegisterFn(\"customer_auth.hash_password\", HashPassword)"),
        "expected RegisterFn self-registration; got:\n{}",
        hash.contents
    );
    // Still carries the "will not overwrite" promise.
    assert!(
        hash.contents
            .contains("Lazuli will not overwrite this file on regenerate."),
        "expected the will-not-overwrite header; got:\n{}",
        hash.contents
    );
    // Still keeps the observability prologue exactly as the plain stub.
    assert!(
        hash.contents
            .contains("ctx.Context, endOp = observability.StartOp(ctx.Context)")
    );
}

/// G2 / back-compat — a stub whose site has NO table row emits today's
/// `// IMPLEMENT ME` body byte-for-byte: the `var zero` / `errors.New(...)`
/// pair, the `errors` import, and NO `auth` import.
#[test]
fn smart_stub_non_mapped_fn_still_plain_stub() {
    // A command `let` binding references an arbitrary `@fn.compute_score` — its
    // site is `scoring.recompute.let.new_score` (NOT `.auth.password.*`), so no
    // delegation row matches and the plain `// IMPLEMENT ME` stub must survive
    // byte-for-byte.
    let mut feature = base_feature("scoring");
    feature.commands.push(Command {
        name: "recompute".to_owned(),
        public_contract: None,
        kind: CommandKind::Update,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: vec![lazuli_ir::LetBinding {
            name: "new_score".to_owned(),
            value: Expr::Path(Path {
                segments: vec!["@fn".to_owned(), "compute_score(target)".to_owned()],
            }),
        }],
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
        derived_from: None,
    });
    feature.extensions.push(extension(
        "compute_score",
        ExtensionContract::Function {
            input: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Account".to_owned(),
            }),
            output: TypeRef::Builtin(BuiltinType::Integer),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let stub = files
        .iter()
        .find(|file| file.path == "app/features/scoring/handlers/compute_score.go")
        .expect("non-mapped fn stub emitted");

    // Unchanged plain-stub body.
    assert!(
        stub.contents.contains("// IMPLEMENT ME"),
        "non-mapped stub must keep the plain // IMPLEMENT ME body; got:\n{}",
        stub.contents
    );
    assert!(
        stub.contents
            .contains("return zero, errors.New(\"compute_score not yet implemented\")"),
        "expected the unchanged errors.New placeholder; got:\n{}",
        stub.contents
    );
    assert!(stub.contents.contains("\"errors\""));
    // The delegation imports must NOT leak into a non-mapped stub.
    assert!(
        !stub.contents.contains("lazuli.dev/runtime/lazuli/auth"),
        "non-mapped stub must not import the runtime auth pkg; got:\n{}",
        stub.contents
    );
    assert!(!stub.contents.contains("auth.HashPassword"));
}

/// COMPILE-SAFETY — an `.auth.password.hash` site whose `@fn` has NO extension
/// contract resolves to the `(any, any)` fallback. `auth.HashPassword` needs a
/// `plaintext string`, so passing `any` would NOT compile. The row's `applies`
/// type guard rejects this shape and the stub falls back to the plain
/// `// IMPLEMENT ME` body (the author tightens the `@fn` to
/// `Function[Text, Hashed(...)]`, then regenerate delegates).
#[test]
fn smart_stub_untyped_hash_falls_back_to_plain_stub() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(auth_password("@fn.hash_password", "@fn.verify_password"));
    // NOTE: intentionally NO `extension("hash_password", ...)` — the `@fn` is
    // un-typed, so the resolved signature is the `(any, any)` fallback.
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let hash = files
        .iter()
        .find(|file| file.path == "app/features/customer_auth/handlers/hash_password.go")
        .expect("hash stub emitted");

    // Un-typed → plain stub (would not compile as a delegation), NOT delegating.
    assert!(
        hash.contents.contains("func HashPassword(ctx *lazuli.Ctx, input any) (any, error)"),
        "expected the (any, any) fallback signature; got:\n{}",
        hash.contents
    );
    assert!(
        hash.contents.contains("// IMPLEMENT ME"),
        "un-typed hash @fn must fall back to the plain stub; got:\n{}",
        hash.contents
    );
    assert!(!hash.contents.contains("auth.HashPassword"));
    assert!(!hash.contents.contains("lazuli.dev/runtime/lazuli/auth"));

    // And the guard itself is unit-checkable: string in/out delegates; any does not.
    assert!(STUB_DELEGATION_TABLE_APPLIES_STRING());
    assert!(!STUB_DELEGATION_TABLE_APPLIES_ANY());
}

#[allow(non_snake_case)]
fn STUB_DELEGATION_TABLE_APPLIES_STRING() -> bool {
    let rule = lookup_delegation("f.auth.password.hash").unwrap();
    (rule.applies)("string", "lazuli.HashedRef")
}

#[allow(non_snake_case)]
fn STUB_DELEGATION_TABLE_APPLIES_ANY() -> bool {
    let rule = lookup_delegation("f.auth.password.hash").unwrap();
    (rule.applies)("any", "any")
}

/// ENFORCE / O(1)-growth — adding a synthetic `StubDelegation` row routes a new
/// `site_suffix` to its renderer through the same `.ends_with` lookup, with no
/// `emit_stub_contents` control-flow edit. Mirrors 0024's `table_is_extensible`.
#[test]
fn smart_stub_table_is_extensible() {
    fn synthetic_body(_ctx: &DelegationCtx) -> String {
        "\treturn synthetic, nil".to_owned()
    }
    fn synthetic_applies(_input: &str, _output: &str) -> bool {
        true
    }
    const SYNTHETIC: &[StubDelegation] = &[StubDelegation {
        site_suffix: ".widget.frobnicate",
        applies: synthetic_applies,
        render_body: synthetic_body,
        extra_imports: &["example.dev/widget"],
        family: "synthetic.frobnicate",
    }];

    // The lookup is a pure `.ends_with` over the table — feeding a synthetic
    // table proves a new family routes by data alone.
    let hit = SYNTHETIC
        .iter()
        .find(|r| "some_feature.widget.frobnicate".ends_with(r.site_suffix));
    assert!(hit.is_some(), "synthetic suffix must route via .ends_with");
    assert_eq!(hit.unwrap().family, "synthetic.frobnicate");

    // And the production lookup still routes the seeded flagship + misses
    // everything else, with no control-flow branching per family.
    assert!(lookup_delegation("customer_auth.auth.password.hash").is_some());
    assert!(lookup_delegation("anything.command.do_thing").is_none());
}
