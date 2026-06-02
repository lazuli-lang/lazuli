//! Root `lazuli_app.gen.go` emit tests + the full-capsule integration
//! snapshot that walks the entire `examples/full-capsule` fixture
//! through the emitter.

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{AppLocale, AppLogging, AppTracing, Module};

use super::builders::{minimal_app_manifest, minimal_module};

#[test]
fn root_lazuli_app_gen_emits_app_name_and_locale_contract() {
    // Cell I2 — when `module.app: Option<AppManifest>` carries an
    // `app.locale` block the emitter renders an `i18n.LocaleContract`
    // value matching the runtime team's hand-rolled contract type
    // (`runtime/go/lazuli/i18n/contract.go`).
    let mut module = minimal_module("test_app", "customer");
    if let Some(app) = module.app.as_mut() {
        app.locale = Some(AppLocale {
            default: "pt-BR".to_owned(),
            supported: vec!["pt-BR".to_owned(), "en-US".to_owned()],
            fallbacks: Vec::new(),
        });
    }
    let files = generate_v1(&module, &GoEmitOptions::default());
    let app_gen = files
        .iter()
        .find(|f| f.path == "lazuli_app.gen.go")
        .expect("expected root lazuli_app.gen.go");
    assert!(
        app_gen.contents.contains("\npackage main\n"),
        "expected `package main` in lazuli_app.gen.go:\n{}",
        app_gen.contents
    );
    assert!(
        app_gen.contents.contains("const AppName = \"test_app\""),
        "expected `AppName` constant in lazuli_app.gen.go:\n{}",
        app_gen.contents
    );
    assert!(
        app_gen
            .contents
            .contains("var LocaleContract = i18n.LocaleContract{"),
        "expected `LocaleContract` declaration in lazuli_app.gen.go:\n{}",
        app_gen.contents
    );
    assert!(
        app_gen.contents.contains("Default: \"pt-BR\","),
        "expected lowered default locale tag:\n{}",
        app_gen.contents
    );
}

#[test]
fn root_lazuli_app_gen_emits_logging_and_tracing_contracts() {
    // Cell I2 — `app.logging` + `app.tracing` lower to the runtime's
    // `observability` package contract values (already declared in
    // `runtime/go/lazuli/observability/`).
    let mut module = minimal_module("AcmeCRM", "customer");
    if let Some(app) = module.app.as_mut() {
        app.logging = Some(AppLogging {
            level: Some("info".to_owned()),
            format: Some("json".to_owned()),
            redact: Some("pii".to_owned()),
            sample_rate: None,
            span_ref: None,
        });
        app.tracing = Some(AppTracing {
            propagate: Some(true),
            sample_rate: Some(0.1),
            exporter: Some("@adapter.otlp".to_owned()),
            span_ref: None,
        });
    }
    let files = generate_v1(&module, &GoEmitOptions::default());
    let app_gen = files
        .iter()
        .find(|f| f.path == "lazuli_app.gen.go")
        .expect("expected root lazuli_app.gen.go");

    assert!(
        app_gen
            .contents
            .contains("\"lazuli.dev/runtime/lazuli/observability\""),
        "expected observability import in lazuli_app.gen.go:\n{}",
        app_gen.contents
    );
    assert!(
        app_gen
            .contents
            .contains("var LoggingContract = observability.LoggingContract{"),
        "expected `LoggingContract` declaration:\n{}",
        app_gen.contents
    );
    // The struct-literal block aligns `Level:`/`Format:`/`Redact:` to
    // the widest key in the block. The integration fixture omits
    // `SampleRate`, so the widest key here is `Format:` (7 chars).
    assert!(
        app_gen
            .contents
            .contains("Level:  observability.LogLevelInfo,"),
        "expected lowered LogLevel constant:\n{}",
        app_gen.contents
    );
    assert!(
        app_gen
            .contents
            .contains("var TracingContract = observability.TracingContract{"),
        "expected `TracingContract` declaration:\n{}",
        app_gen.contents
    );
    // `SampleRate:` is the widest key in the tracing block, so
    // `Exporter:` (9 chars) gets two trailing spaces before the
    // value column.
    assert!(
        app_gen.contents.contains("Exporter:   \"@adapter.otlp\","),
        "expected adapter exporter literal:\n{}",
        app_gen.contents
    );
}

#[test]
fn root_lazuli_app_gen_skipped_when_manifest_has_no_observable_surface() {
    // Cell I2 — if the only thing in `AppManifest` is an empty name
    // and no sub-blocks, emitting the file is noise. Mirrors the
    // per-feature skip rules (`resource.gen.go` / `enum.gen.go` /
    // `command.gen.go`).
    let mut module = minimal_module("", "customer");
    // Clear every sub-block — the helper already left them None.
    if let Some(app) = module.app.as_mut() {
        app.locale = None;
        app.logging = None;
        app.tracing = None;
    }
    let files = generate_v1(&module, &GoEmitOptions::default());
    assert!(
        files.iter().all(|f| f.path != "lazuli_app.gen.go"),
        "expected no lazuli_app.gen.go when manifest carries no observable surface, got files: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    // But main.go should still be present — the binary entry point is
    // mandatory even on a bare module.
    assert!(
        files.iter().any(|f| f.path == "main.go"),
        "expected root main.go regardless of manifest surface"
    );
}

#[test]
fn full_capsule_emits_expected_integration_snapshot_structure() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live under <repo>/crates/lazuli_codegen_go");
    let fixture_dir = repo_root.join("examples").join("full-capsule");

    let app_source = std::fs::read_to_string(fixture_dir.join("app.lzi"))
        .expect("expected examples/full-capsule/app.lzi to be readable");
    let app_name = app_source
        .lines()
        .find_map(|line| line.trim().strip_prefix("app "))
        .expect("expected full-capsule app.lzi to declare `app <Name>`");

    let feature_source = std::fs::read_to_string(fixture_dir.join("full-capsule.lzi"))
        .expect("expected examples/full-capsule/full-capsule.lzi to be readable");
    let features = lazuli_syntax::parse_feature_skeletons(&feature_source)
        .expect("expected full-capsule feature skeletons to parse")
        .into_iter()
        .map(|ast| {
            lazuli_analyzer::lower_feature_skeleton(&ast)
                .expect("expected full-capsule feature skeleton to lower")
        })
        .collect();

    let module = Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app_manifest(app_name)),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features,
    };
    let files = generate_v1(&module, &GoEmitOptions::default());
    let paths = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();

    assert!(
        files.len() >= 40,
        "expected at least 40 full-capsule files, got {}: {:?}",
        files.len(),
        paths
    );

    for feature in [
        "account",
        "customer",
        "customer_auth",
        "customer_import",
        "customer_outreach",
        "customer_tags",
    ] {
        assert!(
            paths
                .iter()
                .any(|path| path.starts_with(&format!("{feature}/"))),
            "expected generated directory for feature `{feature}`, got: {:?}",
            paths
        );
    }

    for root_file in ["go.mod", "main.go", "lazuli_app.gen.go"] {
        assert!(
            paths.contains(&root_file),
            "expected root file `{root_file}`, got: {:?}",
            paths
        );
    }

    assert!(
        paths
            .iter()
            .any(|path| path.starts_with("migrations/") && path.ends_with(".sql")),
        "expected at least one N3 DDL migration file, got: {:?}",
        paths
    );
}
