//! `rate_limit: ...` suffix projection — IR Rate-Limit env-aware
//! Cell 3 inspect surface. Spec:
//! `docs/proposals/ir-rate-limit-env-aware.md` §11.2.
//!
//! Renders the per-row rate-limit suffix in three shapes (no slot
//! omitted, legacy compact, env-qualified `| <limit> in <envs>...`).
//! The helpers stay dead-code-allowlisted until Cell 1 flips the
//! `Command.rate_limit` type — at that point `render_one_feature` will
//! pipe each row through here.

use lazuli_ir::{EnvName, RateLimitSpec};

/// `rate_limit: <default> [(default) | <limit> in <envs>...]` projection
/// for a single command/query/api row — see
/// `docs/proposals/ir-rate-limit-env-aware.md` §11.2.
///
/// Three shapes:
/// - `Option::None` returns `String::new()` (no suffix).
/// - `Some(spec)` with `by_env` empty: legacy compact shape,
///   `rate_limit: <default>` (no `(default)` marker; backward-compat
///   with single-line declarations).
/// - `Some(spec)` with one-or-more `by_env` entries: env-qualified
///   shape, `rate_limit: <default> (default) | <limit> in <envs>...`.
///
/// The literal string from `default` / `by_env[i].limit` is used as-is;
/// `"unlimited"` is surfaced verbatim (empty-string limits — the
/// lowering target of `"unlimited"` in Cell 1 — also render as
/// `unlimited` so the projection is round-trippable for humans).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::inspect::features_summary::rate_limit::format_rate_limit_suffix;
///
/// assert_eq!(format_rate_limit_suffix(None), "");
/// ```
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
pub fn format_rate_limit_suffix(spec: Option<&RateLimitSpec>) -> String {
    let Some(spec) = spec else {
        return String::new();
    };
    if spec.by_env.is_empty() {
        return format!("rate_limit: {}", display_limit(&spec.default));
    }
    let mut out = format!("rate_limit: {} (default)", display_limit(&spec.default));
    for entry in &spec.by_env {
        let envs = entry
            .envs
            .iter()
            .map(env_name_str)
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(" | {} in {}", display_limit(&entry.limit), envs));
    }
    out
}

/// Surface an `unlimited`-equivalent empty-string limit verbatim as
/// `unlimited` (the sentinel lowering from `"unlimited"` per
/// proposal §4.4). Non-empty literals pass through unchanged so the
/// inspect view shows the author's text exactly.
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
fn display_limit(literal: &str) -> &str {
    if literal.is_empty() {
        "unlimited"
    } else {
        literal
    }
}

/// Map `EnvName` to its lowercase wire-form string — same identifiers
/// the parser / runtime / doctor catalog agree on.
#[allow(dead_code)] // wired into `render_one_feature` when Cell 1 flips Command.rate_limit type.
fn env_name_str(env: &EnvName) -> &'static str {
    match env {
        EnvName::Production => "production",
        EnvName::Staging => "staging",
        EnvName::Test => "test",
        EnvName::Dev => "dev",
        EnvName::Local => "local",
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // ----------------------------------------------------------------
    // IR Rate-Limit env-aware — Cell 3 inspect surface. Spec:
    // `docs/proposals/ir-rate-limit-env-aware.md` §11.2.
    // The `format_rate_limit_suffix` helper projects the three
    // shapes: no slot (omitted), legacy single-line, env-qualified.
    // ----------------------------------------------------------------

    use lazuli_ir::{EnvName, RateLimitByEnv, RateLimitSpec};

    /// No `rate_limit` slot → empty string (the per-row renderer
    /// omits the suffix entirely). This is the backward-compat
    /// fixture for commands without a rate_limit declaration.
    #[test]
    fn format_rate_limit_suffix_none_omitted() {
        let out = super::format_rate_limit_suffix(None);
        assert_eq!(out, "", "absent rate_limit must render no suffix");
    }

    /// Legacy single-line shape (default only, no `by_env` entries)
    /// renders the compact form: `rate_limit: <literal>` with no
    /// `(default)` marker. Backward-compat with the 56 pilot-A
    /// declarations.
    #[test]
    fn format_rate_limit_suffix_legacy_default_only() {
        let spec = RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: Vec::new(),
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        assert_eq!(
            out, "rate_limit: 5 per 10 minutes per ip",
            "legacy single-line rate_limit should render the compact form"
        );
    }

    /// Env-qualified shape: default + one `by_env` entry covering
    /// three envs. Renders the §11.2 verbatim shape:
    /// `rate_limit: <default> (default) | <limit> in <envs>`.
    /// "unlimited" lowers to empty-string in the IR; the projection
    /// surfaces the word back to humans.
    #[test]
    fn format_rate_limit_suffix_env_qualified_register() {
        let spec = RateLimitSpec {
            default: "5 per 10 minutes per ip".to_owned(),
            by_env: vec![RateLimitByEnv {
                envs: vec![EnvName::Dev, EnvName::Staging, EnvName::Test],
                unknown_envs: Vec::new(),
                // empty string == lowered "unlimited" (§4.4).
                limit: String::new(),
                span_ref: None,
            }],
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        assert_eq!(
            out, "rate_limit: 5 per 10 minutes per ip (default) | unlimited in dev,staging,test",
            "env-qualified shape should match §11.2 verbatim line"
        );
    }

    /// Multiple env-qualified entries — `request_password_reset`
    /// style: strict in prod, looser in dev/staging, unlimited in
    /// test. Each entry renders as a `| <limit> in <envs>` slot.
    #[test]
    fn format_rate_limit_suffix_multiple_env_entries() {
        let spec = RateLimitSpec {
            default: "3 per 10 minutes per ip".to_owned(),
            by_env: vec![
                RateLimitByEnv {
                    envs: vec![EnvName::Dev, EnvName::Staging],
                    unknown_envs: Vec::new(),
                    limit: "60 per 10 minutes per ip".to_owned(),
                    span_ref: None,
                },
                RateLimitByEnv {
                    envs: vec![EnvName::Test],
                    unknown_envs: Vec::new(),
                    limit: String::new(),
                    span_ref: None,
                },
            ],
            span_ref: None,
        };
        let out = super::format_rate_limit_suffix(Some(&spec));
        let expected = "rate_limit: 3 per 10 minutes per ip (default) | \
                        60 per 10 minutes per ip in dev,staging | \
                        unlimited in test";
        assert_eq!(
            out, expected,
            "multi-env shape should chain `| <limit> in <envs>` slots in source order"
        );
    }
}
