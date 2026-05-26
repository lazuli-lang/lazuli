//! Rate-limit emission tests — exercise the `format_rate_limit_struct`
//! emitter (lives in `tier4.rs`) through `emit_command_file`. The
//! cluster covers the three shapes specified by
//! `ir-rate-limit-env-aware` Cell 2:
//!
//!   - default-only → compact single-line `lazuli.RateLimit{...}`
//!     literal (byte-stable with the pre-RULE-VOCAB-04 fixtures).
//!   - default + ByEnv → multi-line struct literal with the env-
//!     qualified entries listed verbatim.
//!   - `"unlimited"` keyword → empty-string `Limit` (the runtime's
//!     `IsUnlimited()` reads the empty string as "no throttle").
//!
//! Lifted out of `tier4.rs` (wave R8-2c) so the parent file stays
//! under the ≤500-LOC gold standard. The companion inline tests in
//! `tier4.rs` cover the Tier-4 field-emission axis (Approval /
//! ExternalCalls / Timeout / Retry / Idempotency / Deprecation).

#![cfg(test)]

use super::test_support::{
    base_command, base_feature, emit_with_customer_fallback as emit, local_qname, typed_slot,
};
use lazuli_ir::{
    Assignment, BuiltinType, CommandEffect, CommandInput, CreateEffect, EnvName, Expr, Path,
    RateLimitByEnv, RateLimitSpec,
};

// `ir-rate-limit-env-aware` Cell 2 — codegen emission tests.

#[test]
fn rate_limit_default_only_emits_compact_struct_literal() {
    // Backward-compat: legacy single-line `rate_limit "X"` source
    // lowers to `RateLimitSpec { default: "X", by_env: [] }` and
    // emits the compact one-liner struct literal so existing
    // fixtures are byte-stable.
    let mut feature = base_feature("customer");
    let mut cmd = base_command("create");
    cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "name".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "name"])),
        }],
    });
    cmd.rate_limit = Some(RateLimitSpec::from_default(
        "5 per 10 minutes per ip".to_owned(),
    ));
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("RateLimit: lazuli.RateLimit{Default: \"5 per 10 minutes per ip\"},"),
        "compact single-line shape expected for default-only spec, got:\n{out}"
    );
    // No multi-line `ByEnv` block in the compact shape.
    assert!(
        !out.contains("ByEnv: []lazuli.RateLimitByEnv"),
        "unexpected ByEnv block in default-only emission:\n{out}"
    );
}

#[test]
fn rate_limit_with_by_env_emits_multi_line_struct_literal() {
    // The 22+ playwright trigger case: production strict + dev /
    // staging / test loose. Emission must list every env-qualified
    // entry verbatim (RULE-VOCAB-04 read-through).
    let mut feature = base_feature("customer");
    let mut cmd = base_command("register");
    cmd.input = CommandInput::Typed(vec![typed_slot("email", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "email".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "email"])),
        }],
    });
    cmd.rate_limit = Some(RateLimitSpec {
        default: "5 per 10 minutes per ip".to_owned(),
        by_env: vec![RateLimitByEnv {
            envs: vec![EnvName::Dev, EnvName::Staging, EnvName::Test],
            unknown_envs: Vec::new(),
            limit: "1000 per 10 minutes per ip".to_owned(),
            span_ref: None,
        }],
        span_ref: None,
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    // The opening of the struct literal sits on the kv-aligned line.
    assert!(
        out.contains("RateLimit: lazuli.RateLimit{"),
        "multi-line struct opening missing, got:\n{out}"
    );
    // Default + ByEnv block emitted with absolute indentation so
    // gofmt accepts the file without rewriting.
    assert!(
        out.contains("\t\tDefault: \"5 per 10 minutes per ip\","),
        "Default row not at expected indent, got:\n{out}"
    );
    assert!(
        out.contains("\t\tByEnv: []lazuli.RateLimitByEnv{"),
        "ByEnv slice declaration missing, got:\n{out}"
    );
    assert!(
        out.contains(
            "{Envs: []string{\"dev\", \"staging\", \"test\"}, Limit: \"1000 per 10 minutes per ip\"},"
        ),
        "by-env entry not emitted verbatim, got:\n{out}"
    );
}

#[test]
fn rate_limit_with_unlimited_keyword_emits_empty_string_limit() {
    // The `"unlimited"` keyword lowers to an empty `limit` string
    // (proposal §4.4). Codegen pastes the empty string verbatim;
    // the runtime's `IsUnlimited()` reads the empty as "no throttle".
    let mut feature = base_feature("customer");
    let mut cmd = base_command("register");
    cmd.input = CommandInput::Typed(vec![typed_slot("email", BuiltinType::Text, true)]);
    cmd.effect = CommandEffect::Creates(CreateEffect {
        resource: local_qname("Customer"),
        from_input: false,
        assignments: vec![Assignment {
            field: "email".to_owned(),
            value: Expr::Path(Path::from_segments(["input", "email"])),
        }],
    });
    cmd.rate_limit = Some(RateLimitSpec {
        default: "5 per 10 minutes per ip".to_owned(),
        by_env: vec![RateLimitByEnv {
            envs: vec![EnvName::Test],
            unknown_envs: Vec::new(),
            limit: String::new(), // "unlimited" lowered.
            span_ref: None,
        }],
        span_ref: None,
    });
    feature.commands.push(cmd);

    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("{Envs: []string{\"test\"}, Limit: \"\"},"),
        "unlimited (empty string) by-env entry missing, got:\n{out}"
    );
}
