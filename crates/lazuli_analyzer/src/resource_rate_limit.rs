//! Inline rate-limit literal lowering, extracted from `resource.rs`
//! (Rails-style R9).
//!
//! `ir-rate-limit-env-aware` cell 1: project the parser's
//! `RateLimitSpecAst` onto the IR shape consumed by command, agent,
//! auth-password, and report lowering. The `"unlimited"` keyword carve-
//! out (proposal §4.4) collapses to the empty-string sentinel, which
//! Cell 3 doctor uses to surface `rate_limit_no_default_with_qualifications`.
//!
//! Fns are `pub(crate)`; the public re-exports stay in `resource.rs`.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// `ir-rate-limit-env-aware` cell 1 — lower a parser `RateLimitSpecAst`
/// into the IR `RateLimitSpec`.
///
/// The single-line back-compat case (`rate_limit "X"`) lands here with
/// `default = Some("X")` and `by_env = []`. The new env-qualified case
/// folds each `RateLimitByEnvAst` into a `RateLimitByEnv` with closed-
/// catalog `EnvName`s separated from unknown identifiers; the unknown
/// bucket is what Cell 3 doctor will surface as `rate_limit_unknown_env`.
/// The proposal-defined `"unlimited"` keyword (§4.4) lowers to the
/// empty-string sentinel for both the default and per-env slots.
///
/// When the source authored only env-qualified lines (no unqualified
/// default), the IR default becomes the empty string — same sentinel
/// as `"unlimited"`. Cell 3 doctor surfaces
/// `rate_limit_no_default_with_qualifications` so the silent default
/// is visible at lint time, per proposal §9.2.
pub(crate) fn lower_rate_limit_spec(spec: &syntax::RateLimitSpecAst) -> ir::RateLimitSpec {
    let default = match spec.default.as_deref() {
        Some(literal) => lower_rate_limit_literal(literal),
        None => String::new(),
    };
    let by_env = spec
        .by_env
        .iter()
        .map(|entry| {
            let mut known = Vec::with_capacity(entry.envs.len());
            let mut unknown = Vec::new();
            for raw in &entry.envs {
                if let Some(env) = ir::EnvName::from_ident(raw) {
                    known.push(env);
                } else {
                    unknown.push(raw.clone());
                }
            }
            ir::RateLimitByEnv {
                envs: known,
                unknown_envs: unknown,
                limit: lower_rate_limit_literal(&entry.limit),
                span_ref: None,
            }
        })
        .collect();
    ir::RateLimitSpec {
        default,
        by_env,
        span_ref: None,
    }
}

/// `ir-rate-limit-env-aware` cell 1 — lower a single literal into the
/// canonical IR form, applying the `"unlimited"` keyword carve-out
/// (proposal §4.4). The keyword is recognised verbatim (case-sensitive)
/// and lowers to the empty string; everything else passes through
/// unchanged.
pub(crate) fn lower_rate_limit_literal(literal: &str) -> String {
    if literal == "unlimited" {
        String::new()
    } else {
        literal.to_owned()
    }
}
