//! Env-aware rate-limit specification IR.
//!
//! `ir-rate-limit-env-aware §4` — the lowered shape behind a
//! `rate_limit "<text>"` (default-only) authoring line, or the
//! multi-line `rate_limit "<text>" in <env, env>` form that overrides
//! per environment. The runtime helper `ResolveLimit()` (cell 2)
//! reads `LAZULI_ENV`, scans `by_env` in source order, and returns
//! the first matching `limit` — falling through to `default`.
//!
//! The `"unlimited"` keyword (proposal §4.4) lowers to the empty
//! string (either as the default or inside a `RateLimitByEnv.limit`)
//! so downstream consumers can treat absence of throttle uniformly.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// ir-rate-limit-env-aware §4.1 — env-qualified rate limit container.
///
/// Backward-compat: the single-line `rate_limit "X"` source shape lowers
/// to `RateLimitSpec { default: "X", by_env: vec![] }`. The runtime helper
/// `ResolveLimit()` (cell 2) reads `LAZULI_ENV`, scans `by_env` in source
/// order, and returns the matching `limit` or falls through to `default`.
///
/// The `"unlimited"` keyword (proposal §4.4) lowers to an empty string —
/// either as the `default` (no throttle by default) or inside a
/// `RateLimitByEnv.limit` (no throttle for the listed envs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitSpec {
    /// Default rate limit applied when no env-qualified entry matches.
    /// Empty string == no rate limit (the `"unlimited"` sentinel lowers
    /// here). The single-line `rate_limit "X"` source shape populates
    /// only this field; `by_env` stays empty.
    pub default: String,
    /// Env-qualified overrides scanned linearly at request time. Each
    /// entry covers one-or-more `EnvName`s sharing a limit string.
    /// Source-order is preserved; the first matching entry wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_env: Vec<RateLimitByEnv>,
    /// Span of the FIRST `rate_limit` line for this spec — points at the
    /// default-declaring line. Per-`by_env` entries carry their own span.
    /// Optional so synth-emitted specs without a source location work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

impl RateLimitSpec {
    /// Backward-compat constructor — lifts the legacy single-string
    /// shape (or the lowered output of a one-line `rate_limit "X"`) into
    /// the new `RateLimitSpec` container. Call-sites that previously
    /// wrote `rate_limit: Some("X".to_owned())` swap to
    /// `rate_limit: Some(RateLimitSpec::from_default("X".to_owned()))`.
    pub fn from_default(s: String) -> Self {
        Self {
            default: s,
            by_env: Vec::new(),
            span_ref: None,
        }
    }
}

/// ir-rate-limit-env-aware §4.1 — single env-qualified override row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitByEnv {
    /// Env names this entry matches. Catalog is closed (`EnvName`);
    /// identifiers outside the catalog land in `unknown_envs` and trigger
    /// the doctor diagnostic `rate_limit_unknown_env` (Cell 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envs: Vec<EnvName>,
    /// Forward-compatible bucket for env identifiers outside the closed
    /// catalog. Parses OK at AST level; Cell 3 doctor emits
    /// `rate_limit_unknown_env`. Empty for well-formed source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_envs: Vec<String>,
    /// Limit string (e.g. `"100 per 10 minutes per ip"`) or the empty
    /// string when the source authored `"unlimited"` (proposal §4.4).
    pub limit: String,
    /// Span of this `rate_limit ... in <envs>` line in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// ir-rate-limit-env-aware §4.3 — closed catalog of recognized
/// `LAZULI_ENV` values. Adding a variant is an IR change requiring a
/// proposal. The JSON form is snake_case (matches the runtime
/// `LAZULI_ENV` strings the existing CORS/session code reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvName {
    Production,
    Staging,
    Test,
    Dev,
    Local,
}

impl EnvName {
    /// Parse a lowercase identifier into the closed catalog. Returns
    /// `None` for identifiers outside the catalog; callers (the parser
    /// today, Cell 3 doctor tomorrow) decide whether to surface a
    /// warning.
    pub fn from_ident(ident: &str) -> Option<Self> {
        match ident {
            "production" => Some(EnvName::Production),
            "staging" => Some(EnvName::Staging),
            "test" => Some(EnvName::Test),
            "dev" => Some(EnvName::Dev),
            "local" => Some(EnvName::Local),
            _ => None,
        }
    }

    /// Canonical lowercase identifier (matches `LAZULI_ENV` strings).
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvName::Production => "production",
            EnvName::Staging => "staging",
            EnvName::Test => "test",
            EnvName::Dev => "dev",
            EnvName::Local => "local",
        }
    }
}
