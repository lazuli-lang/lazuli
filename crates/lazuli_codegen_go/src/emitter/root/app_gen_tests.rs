//! lazuli_app.gen.go and locale/logging/tracing/cors/encryption tests.
//! Fixtures live in `test_support`.

use super::*;
use super::test_support::*;
use helpers::parse_duration_to_seconds;
use lazuli_ir::{AppLocale, AppLogging, AppTracing, AppCors, AppManifest, Feature, LocaleFallback, EncryptionAlgorithm, EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate};

#[test]
fn lazuli_app_gen_emits_app_name_for_minimal_manifest() {
    let module = module_with(Vec::new(), Some(manifest("AcmeCRM")));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
    assert!(out.contains("package main"));
    assert!(
        out.contains("const AppName = \"AcmeCRM\""),
        "expected `AppName` constant in lazuli_app.gen.go:\n{}",
        out
    );
}

#[test]
fn lazuli_app_gen_emits_locale_contract() {
    let mut app = manifest("AcmeCRM");
    app.locale = Some(AppLocale {
        default: "pt-BR".to_owned(),
        supported: vec!["pt-BR".to_owned(), "en-US".to_owned()],
        fallbacks: vec![LocaleFallback {
            from: "en-US".to_owned(),
            to: "pt-BR".to_owned(),
        }],
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

    assert!(out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
    assert!(out.contains("var LocaleContract = i18n.LocaleContract{"));
    assert!(out.contains("Default: \"pt-BR\","));
    assert!(out.contains("\"pt-BR\","));
    assert!(out.contains("\"en-US\","));
    assert!(out.contains("[]i18n.Fallback{"));
    assert!(out.contains("{From: \"en-US\", To: \"pt-BR\"},"));
}

#[test]
fn lazuli_app_gen_emits_logging_contract_with_known_constants() {
    let mut app = manifest("AcmeCRM");
    app.logging = Some(AppLogging {
        level: Some("info".to_owned()),
        format: Some("json".to_owned()),
        redact: Some("pii".to_owned()),
        sample_rate: Some(0.5),
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

    assert!(out.contains("\"lazuli.dev/runtime/lazuli/observability\""));
    assert!(out.contains("var LoggingContract = observability.LoggingContract{"));
    // Keys are padded to the widest in the block (`SampleRate:` at
    // 11 chars). Sample-rate stays unpadded; the others get
    // trailing spaces before the value column.
    assert!(out.contains("Level:      observability.LogLevelInfo,"));
    assert!(out.contains("Format:     observability.LogFormatJSON,"));
    assert!(out.contains("Redact:     observability.RedactPII,"));
    assert!(out.contains("SampleRate: 0.5,"));
}

#[test]
fn lazuli_app_gen_emits_tracing_contract_with_exporter() {
    let mut app = manifest("AcmeCRM");
    app.tracing = Some(AppTracing {
        propagate: Some(true),
        sample_rate: Some(0.05),
        exporter: Some("@adapter.otlp".to_owned()),
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

    assert!(out.contains("var TracingContract = observability.TracingContract{"));
    // `SampleRate:` (11 chars) is the widest key — `Propagate:`
    // (10 chars) and `Exporter:` (9 chars) pad accordingly.
    assert!(out.contains("Propagate:  true,"));
    assert!(out.contains("SampleRate: 0.05,"));
    assert!(out.contains("Exporter:   \"@adapter.otlp\","));
}

#[test]
fn lazuli_app_gen_emits_cors_contract() {
    let mut app = manifest("AcmeCRM");
    app.cors = Some(AppCors {
        allow_origins: vec![
            lazuli_ir::AppCorsOriginRule {
                environment: "production".to_owned(),
                origins: vec!["https://app.example.com".to_owned()],
            },
            lazuli_ir::AppCorsOriginRule {
                environment: "local".to_owned(),
                origins: vec![
                    "http://localhost:5173".to_owned(),
                    "http://localhost:5174".to_owned(),
                ],
            },
        ],
        allow_credentials: true,
        max_age: Some("1h".to_owned()),
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

    // Real codegen now — no TODO marker, an actual lazuli.AppCors
    // value + init() that registers with the runtime middleware.
    assert!(
        out.contains("var CorsContract = lazuli.AppCors{"),
        "expected CorsContract var, got:\n{}",
        out
    );
    assert!(out.contains("\"production\": {\"https://app.example.com\"}"));
    assert!(out.contains("\"local\": {\"http://localhost:5173\", \"http://localhost:5174\"}"));
    assert!(out.contains("AllowCredentials: true,"));
    assert!(out.contains("MaxAge: 3600,"));
    assert!(out.contains("lazuli.SetCorsContract(&CorsContract)"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
}

#[test]
fn parse_duration_to_seconds_unit_table() {
    assert_eq!(parse_duration_to_seconds("30s"), Some(30));
    assert_eq!(parse_duration_to_seconds("10 minutes"), Some(600));
    assert_eq!(parse_duration_to_seconds("1h"), Some(3600));
    assert_eq!(parse_duration_to_seconds("2 hours"), Some(7200));
    assert_eq!(parse_duration_to_seconds("1d"), Some(86_400));
    assert_eq!(parse_duration_to_seconds("nonsense"), None);
    assert_eq!(parse_duration_to_seconds(""), None);
    assert_eq!(parse_duration_to_seconds("-5s"), None);
}

#[test]
fn lazuli_app_gen_skips_file_when_manifest_absent() {
    let module = module_with(Vec::new(), None);
    assert!(emit_lazuli_app_gen(&module, "X").is_none());
}

#[test]
fn lazuli_app_gen_skips_file_when_manifest_empty() {
    // Manifest with empty name + no sub-blocks → nothing meaningful
    // to render. The minimal `name = ""` case shouldn't materialise
    // an empty file with only the banner.
    let mut app = manifest("");
    app.locale = None;
    app.logging = None;
    app.tracing = None;
    let module = module_with(Vec::new(), Some(app));
    assert!(
        emit_lazuli_app_gen(&module, "anonymous").is_none(),
        "expected None when manifest carries no observable surface"
    );
}

#[test]
fn deterministic_main_go_across_runs() {
    let module = module_with(
        vec![
            empty_feature("zebra"),
            empty_feature("alpha"),
            empty_feature("mango"),
        ],
        Some(manifest("test_app")),
    );
    let a = emit_main_go(&module, "lazuli/test-app", "test_app", None);
    let b = emit_main_go(&module, "lazuli/test-app", "test_app", None);
    assert_eq!(a, b);
}

#[test]
fn deterministic_lazuli_app_gen_across_runs() {
    let mut app = manifest("AcmeCRM");
    app.locale = Some(AppLocale {
        default: "pt-BR".to_owned(),
        supported: vec!["pt-BR".to_owned(), "en-US".to_owned()],
        fallbacks: Vec::new(),
    });
    app.logging = Some(AppLogging {
        level: Some("info".to_owned()),
        format: Some("json".to_owned()),
        redact: None,
        sample_rate: None,
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let a = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
    let b = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
    assert_eq!(a, b);
}

#[test]
fn unknown_log_level_falls_back_to_info() {
    let mut app = manifest("AcmeCRM");
    app.logging = Some(AppLogging {
        level: Some("verbose".to_owned()), // not in closed catalog
        format: None,
        redact: None,
        sample_rate: None,
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
    // Unknown token defers to the runtime default constant — the
    // file always compiles. Doctor surfaces the structural smell.
    // The block only has one key here, so no padding is needed.
    assert!(out.contains("Level: observability.LogLevelInfo,"));
}

// Encryption bucket cycle — `var EncryptionBindings` + `init()`
// registers each binding with the runtime registry. The emitter
// never names AES or any concrete crypto; it threads catalog
// tokens (`encryption.AlgorithmAES256GCM`, etc.) through to the
// runtime so the wire-thin principle holds.
#[test]
fn lazuli_app_gen_emits_encryption_bindings() {
    let mut app = manifest("AcmeCRM");
    app.encryption_bindings.push(EncryptionBinding {
        scope: "@key.tenant".to_owned(),
        source: EncryptionSource::Env(EncryptionTemplate::parse(
            "CRYPT_KEY_TENANT_{tenant_id}",
        )),
        algorithm: EncryptionAlgorithm::Aes256Gcm,
        rotation: EncryptionRotation::Manual,
        rotation_profile: None,
        span_ref: None,
    });
    let module = module_with(Vec::new(), Some(app));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

    // Import shows up.
    assert!(
        out.contains("\"lazuli.dev/runtime/lazuli/encryption\""),
        "expected encryption runtime import in lazuli_app.gen.go:\n{out}"
    );
    // Catalog declared.
    assert!(out.contains("var EncryptionBindings = []encryption.Binding{"));
    // Closed-catalog tokens — codegen never names AES, GCM, nonces.
    assert!(out.contains("Source:    encryption.SourceEnv,"));
    assert!(out.contains("Algorithm: encryption.AlgorithmAES256GCM,"));
    assert!(out.contains("Rotation:  encryption.RotationManual,"));
    // Template axis lifted from the literal.
    assert!(out.contains("Axes:      []encryption.TemplateAxis{encryption.AxisTenantID},"));
    // Template literal preserved verbatim for runtime substitution.
    assert!(out.contains("Template:  \"CRYPT_KEY_TENANT_{tenant_id}\","));
    // `init()` walks the catalog so registration happens at boot.
    assert!(out.contains("func init() {"));
    assert!(out.contains("encryption.Register(b)"));
}

#[test]
fn lazuli_app_gen_no_bindings_omits_encryption_import() {
    let module = module_with(Vec::new(), Some(manifest("AcmeCRM")));
    let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
    assert!(!out.contains("encryption.Binding"));
    assert!(!out.contains("encryption.Register"));
    assert!(!out.contains("lazuli/encryption"));
}
