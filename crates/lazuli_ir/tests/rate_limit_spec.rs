//! `ir-rate-limit-env-aware` cell 1 — IR round-trip + back-compat
//! constructor tests for the `RateLimitSpec` / `RateLimitByEnv` /
//! `EnvName` types introduced in proposal §4.1.

use lazuli_ir::{EnvName, RateLimitByEnv, RateLimitSpec};

#[test]
fn from_default_lifts_single_string_into_default_only_spec() {
    // The single-line `rate_limit "5 per minute per ip"` source shape
    // lowers via `RateLimitSpec::from_default`. The IR shape must be
    // exactly `{ default: "5 per minute per ip", by_env: [] }` so
    // adapters that read the raw string see the same thing they did
    // before the cell.
    let spec = RateLimitSpec::from_default("5 per minute per ip".to_owned());
    assert_eq!(spec.default, "5 per minute per ip");
    assert!(spec.by_env.is_empty());
}

#[test]
fn rate_limit_spec_default_only_serde_round_trip() {
    // JSON for a default-only spec must be `{ "default": "X" }` — no
    // `by_env` key when empty (so the JSON diff vs. older fixtures is
    // a wrap from string to single-key object, not a multi-key blob).
    let spec = RateLimitSpec::from_default("5 per 10 minutes per ip".to_owned());
    let json = serde_json::to_string(&spec).expect("serialize");
    assert_eq!(json, r#"{"default":"5 per 10 minutes per ip"}"#);

    let parsed: RateLimitSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, spec);
}

#[test]
fn rate_limit_spec_with_by_env_serde_round_trip() {
    let spec = RateLimitSpec {
        default: "5 per 10 minutes per ip".to_owned(),
        by_env: vec![RateLimitByEnv {
            envs: vec![EnvName::Dev, EnvName::Staging, EnvName::Test],
            unknown_envs: vec![],
            limit: String::new(),
            span_ref: None,
        }],
        span_ref: None,
    };
    let json = serde_json::to_string(&spec).expect("serialize");
    // Empty `unknown_envs` is skipped by serde; envs render as the
    // closed-catalog snake_case identifiers.
    assert_eq!(
        json,
        r#"{"default":"5 per 10 minutes per ip","by_env":[{"envs":["dev","staging","test"],"limit":""}]}"#
    );
    let parsed: RateLimitSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, spec);
}

#[test]
fn rate_limit_spec_default_only_deserializes_without_by_env_key() {
    // Forward-compat: a producer that omits the empty `by_env` array
    // (Cell 2 codegen) round-trips back to the canonical shape.
    let parsed: RateLimitSpec =
        serde_json::from_str(r#"{"default":"5 per 10 minutes per ip"}"#).expect("deserialize");
    assert_eq!(parsed.default, "5 per 10 minutes per ip");
    assert!(parsed.by_env.is_empty());
}

#[test]
fn env_name_from_ident_covers_closed_catalog() {
    assert_eq!(EnvName::from_ident("production"), Some(EnvName::Production));
    assert_eq!(EnvName::from_ident("staging"), Some(EnvName::Staging));
    assert_eq!(EnvName::from_ident("test"), Some(EnvName::Test));
    assert_eq!(EnvName::from_ident("dev"), Some(EnvName::Dev));
    assert_eq!(EnvName::from_ident("local"), Some(EnvName::Local));
    // Catalog is closed — `qa` is out per proposal §4.3.
    assert_eq!(EnvName::from_ident("qa"), None);
    // Identifiers are case-sensitive (parser normalises before calling).
    assert_eq!(EnvName::from_ident("Production"), None);
}

#[test]
fn env_name_as_str_round_trips_through_from_ident() {
    for env in [
        EnvName::Production,
        EnvName::Staging,
        EnvName::Test,
        EnvName::Dev,
        EnvName::Local,
    ] {
        assert_eq!(EnvName::from_ident(env.as_str()), Some(env));
    }
}

#[test]
fn unknown_envs_round_trip_through_serde() {
    // Forward-compat — pilots may author `in qa` to reserve a future
    // env; Cell 1 stores the raw identifier; Cell 3 doctor warns.
    let spec = RateLimitSpec {
        default: "5 per minute per ip".to_owned(),
        by_env: vec![RateLimitByEnv {
            envs: vec![EnvName::Dev, EnvName::Test],
            unknown_envs: vec!["qa".to_owned()],
            limit: "1000 per minute per ip".to_owned(),
            span_ref: None,
        }],
        span_ref: None,
    };
    let json = serde_json::to_string(&spec).expect("serialize");
    let parsed: RateLimitSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, spec);
}
