fn module_with_features(features: Vec<Feature>) -> Module {
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

fn base_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies {
            categories: Vec::new(),
            fields: Vec::new(),
            span_ref: None,
        },
        errors: None,
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        pollers: vec![],
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: Vec::new(),
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
    }
}

fn extension(name: &str, contract: ExtensionContract) -> Extension {
    Extension {
        name: name.to_owned(),
        contract,
        resolved_path: PathRef {
            path: format!("./handlers/{name}.go"),
            source: PathSource::Convention,
        },
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn identity() -> AuthIdentity {
    AuthIdentity {
        field: FieldRef {
            resource: QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            },
            field: "email".to_owned(),
        },
        public_contract: None,
    }
}

#[test]
fn emits_auth_function_stub_with_extension_signature() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(Auth {
        identity: identity(),
        password: Some(AuthPassword {
            algorithm: "argon2id".to_owned(),
            hash: "@fn.hash_password".to_owned(),
            verify: "@fn.verify_password".to_owned(),
            rate_limit: None,
        }),
        sessions: None,
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    });
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

    assert!(hash.contents.contains("package customer_authhandlers"));
    assert!(hash.contents.contains(
        "func HashPassword(ctx *lazuli.Ctx, input string) (lazuli.HashedRef, error)"
    ));
    assert!(
        hash.contents
            .contains("//   Site: customer_auth.auth.password.hash")
    );
    // Spec 0025 — this site maps to a runtime symbol, so the stub now DELEGATES
    // to `auth.HashPassword` instead of emitting an empty `// IMPLEMENT ME`
    // body. The old `var zero` / `errors.New("... not yet implemented")` pair
    // is gone; the delegating call + the runtime `auth` import are present.
    assert!(
        hash.contents.contains(
            "auth.HashPassword(ctx, customer_authgen.CustomerAuthAuthPassword, input)"
        ),
        "hash stub must delegate to auth.HashPassword; got:\n{}",
        hash.contents
    );
    assert!(!hash.contents.contains("// IMPLEMENT ME"));
    assert!(!hash.contents.contains("not yet implemented"));
    assert!(
        hash.contents
            .contains("\"lazuli.dev/runtime/lazuli/auth\"")
    );
    assert!(hash.contents.contains("//lazuli:pattern extension_stub v1"));
    // Source-tag duplication cleanup (review bug #8, 2026-05-15):
    // the previous stub re-stamped `lazuli.WithSource(...)` inside
    // the handler body, silently overwriting the tag already
    // stamped by the calling `lazuli.Command[I, O]`. The starter
    // stub now leaves that to the caller and only keeps the
    // observability StartOp scope (which is intentionally local).
    assert!(
        !hash.contents.contains("lazuli.WithSource("),
        "starter stub must not re-stamp source tag; got:\n{}",
        hash.contents
    );
    assert!(
        !hash.contents.contains("lazuli.SourceTag{"),
        "starter stub must not construct SourceTag inline; got:\n{}",
        hash.contents
    );
    assert!(
        hash.contents
            .contains("ctx.Context, endOp = observability.StartOp(ctx.Context)")
    );
    // Inlined zero value (was generic `func zero[T any]() T` helper);
    // each stub carries its own `var zero <output>` so two stubs in
    // the same `<feature>handlers` package no longer collide on a
    // shared generic helper.
    assert_eq!(hash.contents.matches("func zero[T any]() T").count(), 0);
    assert_eq!(
        hash.contents.matches("var zero lazuli.HashedRef").count(),
        1
    );
}

#[test]
fn skips_relative_existing_files() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(Auth {
        identity: identity(),
        password: Some(AuthPassword {
            algorithm: "argon2id".to_owned(),
            hash: "@fn.hash_password".to_owned(),
            verify: "@fn.verify_password".to_owned(),
            rate_limit: None,
        }),
        sessions: None,
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    });
    let module = module_with_features(vec![feature]);
    let existing = BTreeSet::from([PathBuf::from(
        "app/features/customer_auth/handlers/hash_password.go",
    )]);

    let files = emit_handler_stubs(&module, "lazuli/test", &existing);

    assert!(
        !files
            .iter()
            .any(|file| file.path == "app/features/customer_auth/handlers/hash_password.go")
    );
    assert!(
        files
            .iter()
            .any(|file| file.path == "app/features/customer_auth/handlers/verify_password.go")
    );
}

#[test]
fn skips_dist_go_existing_files() {
    let mut feature = base_feature("customer_auth");
    feature.auth = Some(Auth {
        identity: identity(),
        password: Some(AuthPassword {
            algorithm: "argon2id".to_owned(),
            hash: "@fn.hash_password".to_owned(),
            verify: "@fn.verify_password".to_owned(),
            rate_limit: None,
        }),
        sessions: None,
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    });
    let module = module_with_features(vec![feature]);
    let existing = BTreeSet::from([PathBuf::from("dist/go/customer_auth/hash_password.go")]);

    let files = emit_handler_stubs(&module, "lazuli/test", &existing);

    assert!(
        !files
            .iter()
            .any(|file| file.path == "app/features/customer_auth/handlers/hash_password.go")
    );
    assert!(
        files
            .iter()
            .any(|file| file.path == "app/features/customer_auth/handlers/verify_password.go")
    );
}

#[test]
fn extracts_function_ref_from_path_encoded_call_expression() {
    let mut feature = base_feature("customer");
    feature.commands.push(Command {
        name: "recompute_score".to_owned(),
        public_contract: None,
        kind: CommandKind::Update,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: vec![lazuli_ir::LetBinding {
            name: "new_score".to_owned(),
            value: Expr::Path(Path {
                segments: vec!["@fn".to_owned(), "risk_score(target)".to_owned()],
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
        "risk_score",
        ExtensionContract::Function {
            input: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
            output: TypeRef::Builtin(BuiltinType::Integer),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());

    let risk = files
        .iter()
        .find(|file| file.path == "app/features/customer/handlers/risk_score.go")
        .expect("risk_score stub emitted");
    assert!(risk.contents.contains(
        "func RiskScore(ctx *lazuli.Ctx, input customergen.Customer) (int64, error)"
    ));
    assert!(
        risk.contents
            .contains("customergen \"lazuli/test/customer\"")
    );
    assert!(
        risk.contents
            .contains("//   Site: customer.recompute_score.let.new_score")
    );
}

#[test]
fn emits_hook_ref_with_hook_signature() {
    let mut feature = base_feature("customer");
    feature.uses.push("@hook.before_create".to_owned());
    feature.extensions.push(extension(
        "before_create",
        ExtensionContract::Hook {
            type_arg: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "CreateCustomer".to_owned(),
            }),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let hook = files
        .iter()
        .find(|file| file.path == "app/features/customer/handlers/before_create.go")
        .expect("hook stub emitted");

    assert!(hook.contents.contains("`@hook.before_create`"));
    assert!(hook.contents.contains(
        "func BeforeCreate(ctx *lazuli.Ctx, input customergen.CreateCustomer) (customergen.CreateCustomer, error)"
    ));
}

#[test]
fn command_returns_stub_uses_generated_input_and_output_types() {
    let mut feature = base_feature("account");
    feature.commands.push(Command {
        name: "login".to_owned(),
        public_contract: None,
        kind: CommandKind::Returns,
        route: Vec::new(),
        input: CommandInput::Typed(vec![TypedSlot {
            name: "email".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticEmail),
            required: true,
            constraints: FieldConstraints::default(),
            validate_skip: false,
        }]),
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::Returns(lazuli_ir::ReturnsEffect {
            return_type: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "AuthSession".to_owned(),
            }),
        }),
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
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "github.com/acme/app/generated", &BTreeSet::new());
    let login = files
        .iter()
        .find(|file| file.path == "app/features/account/handlers/login.go")
        .expect("login stub emitted");

    assert!(login.contents.contains(
        "func Login(ctx *lazuli.Ctx, input accountgen.LoginAuthSessionInput) (accountgen.AuthSession, error)"
    ));
    assert!(
        login
            .contents
            .contains("accountgen \"github.com/acme/app/generated/account\"")
    );
}
