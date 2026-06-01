//! Spec 0021 — typed, kind-discriminated plugin manifest.
//!
//! These tests freeze the contract specs 0022 (verify) and 0023
//! (scaffold) build against:
//!  * every real plugin `manifest.toml` (vendored under
//!    `tests/fixtures/plugin_manifests/`) deserialises (back-compat gate);
//!  * the inferred `kind` is correct for the grounded cases;
//!  * the adapter fields (`implements` / `[env]` / `[binds]`) round-trip;
//!  * a structurally-malformed adapter is rejected (schema is load-bearing);
//!  * the 0019 semantic-types struct is unchanged (drift guard).
//!
//! Run via `cargo test -p lazuli_manifest plugin_manifest`.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_manifest::plugin_manifest::{PluginKind, PluginManifest};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("plugin_manifests")
}

fn read_fixture(short: &str) -> String {
    let path = fixtures_dir().join(format!("{short}.toml"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn parse_fixture(short: &str) -> PluginManifest {
    let body = read_fixture(short);
    toml::from_str::<PluginManifest>(&body).unwrap_or_else(|e| panic!("parse {short}.toml: {e}"))
}

/// Back-compat gate: every real manifest deserialises. A missing fixture
/// fails loudly via the count floor.
#[test]
fn plugin_manifest_all_real_manifests_deserialize() {
    let dir = fixtures_dir();
    let mut count = 0usize;
    for entry in fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        // Only the real-manifest .toml fixtures; skip the malformed
        // fixture and the SOURCE.md note.
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let file = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if file == "malformed_adapter.toml" {
            continue;
        }
        let body = fs::read_to_string(&path).expect("read fixture");
        toml::from_str::<PluginManifest>(&body)
            .unwrap_or_else(|e| panic!("real manifest {} failed to parse: {e}", path.display()));
        count += 1;
    }
    // The real corpus is the 24 `lazuli-plugin-*` dirs (scalars-br is the
    // lone semantic one among them). Floor at 24 so a dropped fixture
    // fails loudly.
    assert!(
        count >= 24,
        "expected >= 24 real manifest fixtures, found {count}"
    );
}

/// scalars-br is the only semantic-kind manifest; the 0019 path is intact
/// (all 4 types, including the 4th `BrazilianPhone`).
#[test]
fn plugin_manifest_scalars_br_infers_semantic() {
    let m = parse_fixture("scalars-br");
    assert_eq!(m.resolved_kind(), PluginKind::Semantic);
    assert_eq!(m.semantic_types.len(), 4);
    assert_eq!(m.semantic_types[3].name, "BrazilianPhone");
}

/// mercadopago is an adapter; `implements` + `[env].required` round-trip.
#[test]
fn plugin_manifest_mercadopago_infers_adapter_with_contract() {
    let m = parse_fixture("mercadopago");
    assert_eq!(m.resolved_kind(), PluginKind::Adapter);
    assert_eq!(m.implements, vec!["payments.PaymentGateway".to_owned()]);
    let env = m.env.expect("[env] present");
    assert_eq!(
        env.required,
        vec![
            "MERCADOPAGO_ACCESS_TOKEN".to_owned(),
            "MERCADOPAGO_WEBHOOK_SECRET".to_owned(),
        ]
    );
    assert!(env.optional.is_empty());
}

/// smtp exercises `[binds]`, `[env].required_for_auth`, the tolerated
/// free-form `[plugin].kind` string, and the `module`→`go_module` fallback.
#[test]
fn plugin_manifest_smtp_models_binds_and_env() {
    let m = parse_fixture("smtp");
    assert_eq!(m.resolved_kind(), PluginKind::Adapter);

    let binds = m.binds.expect("[binds] present");
    assert_eq!(
        binds.interface.as_deref(),
        Some("github.com/lazuli-lang/lazuli-plugin-smtp.EmailSender")
    );
    assert_eq!(
        binds.methods,
        vec!["SendEmail".to_owned(), "SendEmailBatch".to_owned()]
    );

    let env = m.env.expect("[env] present");
    assert_eq!(
        env.required_for_auth,
        vec!["SMTP_USERNAME".to_owned(), "SMTP_PASSWORD".to_owned()]
    );

    let plugin = m.plugin.expect("[plugin] present");
    // Free-form catalog string, NOT the PluginKind enum.
    assert_eq!(plugin.kind.as_deref(), Some("notifications/email-sender"));
    // smtp spells the module key `module`, not `go_module`.
    assert!(plugin.go_module.is_none());
    assert_eq!(
        plugin.effective_go_module(),
        Some("github.com/lazuli-lang/lazuli-plugin-smtp")
    );
}

/// object-store's multi-line `[env].optional` array parses fully.
#[test]
fn plugin_manifest_object_store_optional_env_multiline() {
    let m = parse_fixture("object-store");
    assert_eq!(m.resolved_kind(), PluginKind::Adapter);
    let env = m.env.expect("[env] present");
    assert_eq!(env.optional.len(), 5);
    assert!(env.optional.contains(&"S3_ENDPOINT".to_owned()));
}

/// Explicit `kind` wins over inference in both directions.
#[test]
fn plugin_manifest_explicit_kind_overrides_inference() {
    // capability with no semantic/adapter sections — only the explicit
    // kind + a [capability] block.
    let cap = r#"
kind = "capability"
[plugin]
name = "cap"
namespace = "@lazuli/plugin-cap"
[capability]
provides = ["rate-limit"]
"#;
    let m = toml::from_str::<PluginManifest>(cap).expect("capability parses");
    assert_eq!(m.resolved_kind(), PluginKind::Capability);
    assert_eq!(
        m.capability.expect("[capability]").provides,
        vec!["rate-limit".to_owned()]
    );

    // explicit adapter overrides the semantic-present rule.
    let adapter_with_semantic = r#"
kind = "adapter"
[plugin]
name = "hybrid"
namespace = "@lazuli/plugin-hybrid"
[[semantic_types]]
name = "Foo"
alias = "@semantic.Foo"
carrier_type = "String"
validator = "ValidateFoo"
"#;
    let m2 = toml::from_str::<PluginManifest>(adapter_with_semantic).expect("hybrid parses");
    assert_eq!(m2.resolved_kind(), PluginKind::Adapter);
    // semantic_types still readable regardless of inferred kind.
    assert_eq!(m2.semantic_types.len(), 1);
}

/// An identity-only manifest (viacep) defaults to Semantic (historical
/// default; harmless — contributes no aliases).
#[test]
fn plugin_manifest_identity_only_defaults_semantic() {
    let m = parse_fixture("viacep");
    assert_eq!(m.resolved_kind(), PluginKind::Semantic);
    assert!(m.semantic_types.is_empty());
    assert!(m.implements.is_empty());
    assert!(m.env.is_none());
    assert!(m.binds.is_none());
}

/// Structurally-malformed adapter is rejected — schema is load-bearing.
#[test]
fn plugin_manifest_malformed_adapter_rejected() {
    let body = read_fixture("malformed_adapter");
    let result = toml::from_str::<PluginManifest>(&body);
    assert!(
        result.is_err(),
        "malformed adapter must fail to deserialise, got {result:?}"
    );
}

/// Drift guard: the 0019 `[[semantic_types]]` struct still carries every
/// field the resolver depends on.
#[test]
fn plugin_manifest_semantic_decl_struct_unchanged() {
    let body = r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-scalars-br"

[[semantic_types]]
name = "BrazilianCPF"
alias = "@semantic.BrazilianCPF"
carrier_type = "String"
validator = "ValidateCPF"
formatter = "FormatCPF"
ts_validator = "validateCPF"
error_code = "cpf_invalid"
message_key = "validation.cpf_invalid"
"#;
    let m = toml::from_str::<PluginManifest>(body).expect("parses");
    let t = &m.semantic_types[0];
    assert_eq!(t.name, "BrazilianCPF");
    assert_eq!(t.alias, "@semantic.BrazilianCPF");
    assert_eq!(t.carrier_type, "String");
    assert_eq!(t.validator, "ValidateCPF");
    assert_eq!(t.formatter.as_deref(), Some("FormatCPF"));
    assert_eq!(t.error_code.as_deref(), Some("cpf_invalid"));
    assert_eq!(t.message_key.as_deref(), Some("validation.cpf_invalid"));
    assert_eq!(t.ts_validator.as_deref(), Some("validateCPF"));
}
