use lazuli_codegen_go::{generate_v1, GeneratedFile, GoEmitOptions};
use lazuli_ir::{
    Api, AppManifest, Auth, AuthIdentity, AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessions,
    BackoffStrategy, BuiltinType, CacheTtl, CacheTtlLiteral, CapabilityRef, Defaults, Feature,
    Field, FieldRef, FileCapability, FileSize, FileSizeLiteral, FileVisibility, HttpMethod,
    IdempotencyKey, ListQuery, MimeType, Module, OrderBy, OrderDir, Path, PathRef, Policies,
    PolicyRef, QualifiedName, Query, QueryCache, ReplayMode, ReplaySpec, Resource, RetryPolicy,
    TenantFromSpec, TypeRef, TypedSlot, VerifyScheme, VerifySpec, Webhook,
};

#[test]
fn hostpoint_mini_codegen_covers_core_surfaces_without_fixture_dependency() {
    let module = hostpoint_mini_module();
    let files = generate_v1(&module, &GoEmitOptions::default());
    let paths = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();

    for expected in [
        "customer_auth/resource.gen.go",
        "customer_auth/auth.gen.go",
        "inventory/resource.gen.go",
        "inventory/query.gen.go",
        "inventory/storage.gen.go",
        "payments/resource.gen.go",
        "payments/api.gen.go",
        "payments/webhook.gen.go",
    ] {
        assert!(
            paths.contains(&expected),
            "expected generated Hostpoint mini surface `{expected}`, got: {paths:?}"
        );
    }

    let auth = contents(&files, "customer_auth/auth.gen.go");
    assert!(auth.contains("var customerAuthAuthIdentity = auth.FieldRef"));
    assert!(auth.contains("Resource: \"Guest\", Field: \"email\""));
    assert!(auth.contains("var customerAuthAuthPassword = auth.PasswordContract{"));
    assert!(auth.contains("Algorithm: auth.AlgoArgon2id,"));
    assert!(auth.contains("var customerAuthAuthSessions = auth.SessionsContract{"));
    assert!(auth.contains("Resource: \"GuestSession\","));
    assert!(auth.contains("TTL:      7 * 24 * time.Hour,"));
    assert!(auth.contains("var customerAuthAuthOAuthGoogle = auth.OAuthContract{"));
    assert!(auth.contains("var customerAuthAuthMfa = auth.MfaContract{"));

    let inventory_resource = contents(&files, "inventory/resource.gen.go");
    assert!(inventory_resource.contains("type Stay struct"));
    assert!(inventory_resource.contains("\"github.com/cridenour/go-postgis\""));
    assert!(inventory_resource.contains("\"lazuli.dev/runtime/lazuli/storage\""));
    assert!(inventory_resource.contains("Coordinates  postgis.Point"));
    assert!(inventory_resource.contains("NightlyPrice lazuli.Money"));
    assert!(inventory_resource.contains("Currency     lazuli.Currency"));
    assert!(inventory_resource.contains("Photos       storage.FileRef"));
    assert!(inventory_resource.contains("db:\"coordinates,type:geography(point,4326)\""));
    assert!(
        inventory_resource.contains("var stayResource = lazuli.Resource[Stay]{"),
        "expected generated resource contract:\n{inventory_resource}"
    );

    let inventory_query = contents(&files, "inventory/query.gen.go");
    assert!(inventory_query.contains("type ListStaysArgs struct {"));
    assert!(inventory_query.contains("City *string `json:\"city,omitempty\"`"));
    assert!(inventory_query.contains("var listStays = lazuli.Query[ListStaysArgs, Stay]{"));
    assert!(inventory_query.contains("Kind:     lazuli.QueryList,"));
    assert!(inventory_query.contains("Paginate: 20,"));
    assert!(inventory_query.contains("TTL: 10 * time.Minute,"));

    let inventory_storage = contents(&files, "inventory/storage.gen.go");
    assert!(inventory_storage.contains("var inventoryPhotosFile = storage.FileContract{"));
    assert!(inventory_storage.contains("Resource:   \"Stay\","));
    assert!(inventory_storage.contains("Field:      \"photos\","));
    assert!(inventory_storage.contains("Visibility: storage.VisibilitySigned,"));
    assert!(inventory_storage.contains("SignedTTL:  30 * time.Minute,"));

    let payment_resource = contents(&files, "payments/resource.gen.go");
    assert!(payment_resource.contains("type PaymentIntent struct"));
    assert!(payment_resource.contains("Amount            lazuli.Money"));
    assert!(payment_resource.contains("Currency          lazuli.Currency"));
    assert!(
        payment_resource.contains("var paymentIntentResource = lazuli.Resource[PaymentIntent]{")
    );

    let payment_api = contents(&files, "payments/api.gen.go");
    assert!(payment_api.contains("type CreatePaymentIntentArgs struct {"));
    assert!(payment_api.contains("BookingID lazuli.ID `json:\"booking_id\"`"));
    assert!(payment_api
        .contains("var createPaymentIntent = lazuli.Api[CreatePaymentIntentArgs, PaymentIntent]{"));
    assert!(payment_api.contains("Method:    lazuli.MethodPost,"));
    assert!(payment_api.contains("Path:      \"/api/payments/{booking_id}/intents\","));
    assert!(payment_api.contains("RateLimit: \"20 per minute per user\","));

    let payment_webhook = contents(&files, "payments/webhook.gen.go");
    assert!(payment_webhook.contains("\"lazuli.dev/runtime/lazuli/jobs\""));
    assert!(payment_webhook.contains("\"lazuli.dev/runtime/lazuli/webhooks\""));
    assert!(
        payment_webhook.contains("var stripePaymentSucceededWebhook = webhooks.WebhookContract{")
    );
    assert!(payment_webhook.contains("webhooks.VerifySpec{Scheme: webhooks.VerifyHmac"));
    assert!(payment_webhook.contains("SecretEnv: \"STRIPE_WEBHOOK_SECRET\""));
    assert!(
        payment_webhook.contains("&webhooks.TenantFromSpec{Path: \"payload.metadata.host_id\"}")
    );
    assert!(payment_webhook.contains("IdempotencyBy:"));
    assert!(payment_webhook.contains("\"payload.id\","));
    assert!(payment_webhook.contains("ReturnsType:"));
    assert!(payment_webhook.contains("\"PaymentIntent\","));
    assert!(payment_webhook.contains("[]string{\"payment_succeeded\"},"));
    assert!(payment_webhook.contains("&webhooks.ReplaySpec{Mode: webhooks.ReplayAllow"));
    assert!(
        payment_webhook.contains("&jobs.RetryPolicy{Count: 5, Backoff: jobs.BackoffExponential},")
    );
}

fn hostpoint_mini_module() -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app_manifest("hostpoint_mini")),
        registry: None,
        profiles: Vec::new(),
        features: vec![
            customer_auth_feature(),
            inventory_feature(),
            payments_feature(),
        ],
    }
}

fn customer_auth_feature() -> Feature {
    let mut feature = empty_feature("customer_auth");
    feature.resources.push(resource(
        "Guest",
        vec![
            field("email", TypeRef::Builtin(BuiltinType::SemanticEmail), true),
            field("name", TypeRef::Builtin(BuiltinType::Text), true),
            field("phone", TypeRef::Builtin(BuiltinType::SemanticPhone), false),
        ],
    ));
    feature.resources.push(resource(
        "GuestSession",
        vec![field("guest_id", TypeRef::Builtin(BuiltinType::Id), true)],
    ));
    feature.auth = Some(Auth {
        identity: AuthIdentity {
            field: FieldRef {
                resource: qname("Guest"),
                field: "email".to_owned(),
            },
        },
        password: Some(AuthPassword {
            algorithm: "argon2id".to_owned(),
            hash: "@fn.hash_guest_password".to_owned(),
            verify: "@fn.verify_guest_password".to_owned(),
            rate_limit: Some("5 per 10 minutes".to_owned()),
        }),
        sessions: Some(AuthSessions {
            resource: qname("GuestSession"),
            ttl: "7 days".to_owned(),
            refresh: true,
        }),
        mfa: Some(AuthMfa {
            method: "totp".to_owned(),
            enroll: "@fn.enroll_guest_totp".to_owned(),
            verify: "@validator.verify_guest_totp".to_owned(),
            adapter: None,
        }),
        oauth: vec![AuthOAuthProvider {
            provider: "google".to_owned(),
            adapter: "@adapter.google_oauth".to_owned(),
        }],
        span_ref: None,
    });
    feature
}

fn inventory_feature() -> Feature {
    let mut feature = empty_feature("inventory");
    feature.defaults.timestamps = true;
    feature.defaults.policy = Some(PolicyRef::Local("read".to_owned()));
    feature.resources.push(resource(
        "Stay",
        vec![
            field("title", TypeRef::Builtin(BuiltinType::Text), true),
            field("city", TypeRef::Builtin(BuiltinType::Text), true),
            field(
                "coordinates",
                TypeRef::Builtin(BuiltinType::SemanticGeoPoint),
                true,
            ),
            field(
                "nightly_price",
                TypeRef::Builtin(BuiltinType::SemanticMoney),
                true,
            ),
            field(
                "currency",
                TypeRef::Builtin(BuiltinType::SemanticCurrency),
                true,
            ),
            field(
                "photos",
                TypeRef::Capability(CapabilityRef::File(file_capability(
                    FileSizeLiteral::Mb(8),
                    vec![("image", "*")],
                    Some(FileVisibility::Signed),
                    Some("30m"),
                ))),
                true,
            ),
        ],
    ));
    feature.queries.push(Query::List(ListQuery {
        name: "list_stays".to_owned(),
        params: vec![typed_slot(
            "city",
            TypeRef::Builtin(BuiltinType::Text),
            false,
        )],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: vec![OrderBy {
            field: "nightly_price".to_owned(),
            direction: OrderDir::Asc,
        }],
        paginate: Some(20),
        modifier: None,
        cache: Some(QueryCache {
            key: "inventory.list_stays(params)".to_owned(),
            ttl: CacheTtl::Literal(CacheTtlLiteral::Minutes(10)),
            tags: Vec::new(),
            namespace: None,
        }),
        previous_names: Vec::new(),
        span_ref: None,
    }));
    feature
}

fn payments_feature() -> Feature {
    let mut feature = empty_feature("payments");
    feature.defaults.policy = Some(PolicyRef::Atom("@actor.system".to_owned()));
    feature.resources.push(resource(
        "PaymentIntent",
        vec![
            field("booking_id", TypeRef::Builtin(BuiltinType::Id), true),
            field("provider", TypeRef::Builtin(BuiltinType::Text), true),
            field(
                "provider_payment_id",
                TypeRef::Builtin(BuiltinType::Text),
                true,
            ),
            field("amount", TypeRef::Builtin(BuiltinType::SemanticMoney), true),
            field(
                "currency",
                TypeRef::Builtin(BuiltinType::SemanticCurrency),
                true,
            ),
            field("status", TypeRef::Builtin(BuiltinType::Text), true),
        ],
    ));
    feature.apis.push(Api {
        name: "create_payment_intent".to_owned(),
        method: HttpMethod::Post,
        path: "/api/payments/{booking_id}/intents".to_owned(),
        policy: PolicyRef::Local("pay".to_owned()),
        rate_limit: Some("20 per minute per user".to_owned()),
        output: TypeRef::UserDefined(qname("PaymentIntent")),
        handler: PathRef::authored("./api/create_payment_intent.go"),
        locale_negotiate: None,
        span_ref: None,
    });

    let mut webhook = Webhook {
        name: "stripe_payment_succeeded".to_owned(),
        route: "/webhooks/stripe/payment-succeeded".to_owned(),
        verify: PathRef::convention("./webhooks/stripe_payment_succeeded_verify.go"),
        structured_verify: Some(VerifySpec {
            scheme: VerifyScheme::Hmac,
            algorithm: "sha256".to_owned(),
            secret_env: "STRIPE_WEBHOOK_SECRET".to_owned(),
            header: "Stripe-Signature".to_owned(),
        }),
        tenant_from: Some(TenantFromSpec {
            path: path(&["payload", "metadata", "host_id"]),
        }),
        idempotency: Some(IdempotencyKey {
            by: path(&["payload", "id"]),
        }),
        policy: None,
        handler: PathRef::authored("./webhooks/stripe_payment_succeeded.go"),
        returns: Some(TypeRef::UserDefined(qname("PaymentIntent"))),
        emits: vec!["payment_succeeded".to_owned()],
        payload_from: None,
        replay: Some(ReplaySpec {
            mode: ReplayMode::Allow,
            within: Some("24h".to_owned()),
            dedupe_by: Some(path(&["payload", "id"])),
        }),
        dlq: None,
        retry: Some(RetryPolicy {
            count: 5,
            backoff: BackoffStrategy::Exponential,
        }),
        previous_names: Vec::new(),
        span_ref: None,
    };
    webhook.policy = Some(PolicyRef::Atom("@actor.system".to_owned()));
    feature.webhooks.push(webhook);
    feature
}

fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
        },
        uses: Vec::new(),
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
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn minimal_app_manifest(name: &str) -> AppManifest {
    AppManifest {
        name: name.to_owned(),
        title: None,
        version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        locale: None,
        span_ref: None,
    }
}

fn resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.to_owned(),
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        default: None,
        derived_from: None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

fn typed_slot(name: &str, type_ref: TypeRef, required: bool) -> TypedSlot {
    TypedSlot {
        name: name.to_owned(),
        type_ref,
        required,
    }
}

fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

fn path(segments: &[&str]) -> Path {
    Path::from_segments(segments.iter().copied())
}

fn file_capability(
    literal: FileSizeLiteral,
    accept: Vec<(&str, &str)>,
    visibility: Option<FileVisibility>,
    signed_ttl: Option<&str>,
) -> FileCapability {
    FileCapability {
        max_size: FileSize {
            bytes: literal.bytes(),
            literal,
        },
        accept: accept
            .into_iter()
            .map(|(family, subtype)| MimeType {
                family: family.to_owned(),
                subtype: subtype.to_owned(),
            })
            .collect(),
        visibility,
        signed_ttl: signed_ttl.map(str::to_owned),
    }
}

fn contents<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("expected generated file `{path}`"))
        .contents
        .as_str()
}
