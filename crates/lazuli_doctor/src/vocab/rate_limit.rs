//! Env-qualified `rate_limit` diagnostics — Cell 3 surface for the
//! env-aware rate-limit proposal.
//!
//! References:
//! - `docs/proposals/ir-rate-limit-env-aware.md` §11.1.
//!
//! Catalog (5 codes):
//!
//! | Code                                            | Severity | Trigger                                                             |
//! |-------------------------------------------------|----------|---------------------------------------------------------------------|
//! | `rate_limit_unknown_env`                        | warning  | `in <env>` where `<env>` is not in the closed catalog.              |
//! | `rate_limit_invalid_spec`                       | error    | malformed spec literal that doesn't parse as `N per UNIT per scope`.|
//! | `rate_limit_duplicate_env`                      | error    | same env appears in two qualified entries for one command.         |
//! | `rate_limit_duplicate_default`                  | error    | two unqualified `rate_limit` lines on one command.                 |
//! | `rate_limit_no_default_with_qualifications`     | warning  | only env-qualified lines, no unqualified default.                  |
//!
//! Each diagnostic carries its verbatim message string from §11.1.
//! The actual firing of these codes is wired upstream by Cell 1 (parser
//! emits `rate_limit_duplicate_default` / `rate_limit_duplicate_env`
//! at parse time) and the analyzer's `rate_limit` lint pass for the
//! unknown-env / invalid-spec / no-default warnings; this module owns
//! the canonical Finding structs + message strings so every wiring
//! site emits a single phrasing.

use std::path::PathBuf;

// =============================================================================
// rate_limit_unknown_env — warning emitted by the analyzer when an `in <env>`
//                          tail references an identifier outside the closed
//                          env catalog (production, staging, test, dev, local).
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitUnknownEnvFinding {
    pub path: PathBuf,
    /// `<feature>.<command>` (or `<feature>.<query>` / `<feature>.<api>`).
    pub feature_command: String,
    /// The env identifier the author wrote in `in <env>`.
    pub env: String,
    /// Nearest-match suggestion from the closed catalog (Levenshtein
    /// distance ≤ 2). Lowered into the message slot below.
    pub suggestion: String,
}

impl RateLimitUnknownEnvFinding {
    pub const CODE: &'static str = "rate_limit_unknown_env";

    pub fn message(&self) -> String {
        format!(
            "`rate_limit` on `{}` declares `in {}`, but `{}` is not in the closed catalog. \
             Valid envs: `production`, `staging`, `test`, `dev`, `local`. Did you mean `{}`?",
            self.feature_command, self.env, self.env, self.suggestion
        )
    }
}

// =============================================================================
// rate_limit_invalid_spec — error emitted when the spec literal doesn't parse
//                           as `N per UNIT per scope` (or the `unlimited`
//                           keyword). Catches typos like `"unlimted"`.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitInvalidSpecFinding {
    pub path: PathBuf,
    /// `<feature>.<command>` (or `<feature>.<query>` / `<feature>.<api>`).
    pub feature_command: String,
    /// The raw string literal the author wrote (without surrounding quotes).
    pub literal: String,
}

impl RateLimitInvalidSpecFinding {
    pub const CODE: &'static str = "rate_limit_invalid_spec";

    pub fn message(&self) -> String {
        format!(
            "`rate_limit` literal `{}` on `{}` doesn't parse as `N per UNIT per scope`. \
             Expected forms: `\"100 per 10 minutes per ip\"`, `\"5 per second per user\"`, \
             or `\"unlimited\"`.",
            self.literal, self.feature_command
        )
    }
}

// =============================================================================
// rate_limit_duplicate_env — error emitted at parse / analyzer time when the
//                            same env appears in two qualified `rate_limit`
//                            entries on one command. Resolution would be
//                            ambiguous; author must dedupe.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitDuplicateEnvFinding {
    pub path: PathBuf,
    /// `<feature>.<command>` (or `<feature>.<query>` / `<feature>.<api>`).
    pub feature_command: String,
    /// The env identifier that appears twice.
    pub env: String,
    /// 1-based source line of the first occurrence.
    pub first_line: u32,
    /// 1-based source line of the duplicate occurrence.
    pub second_line: u32,
}

impl RateLimitDuplicateEnvFinding {
    pub const CODE: &'static str = "rate_limit_duplicate_env";

    pub fn message(&self) -> String {
        format!(
            "`{}` declares `rate_limit ... in {}` twice (lines {} + {}). Each env can appear \
             at most once across all `rate_limit` declarations for a command.",
            self.feature_command, self.env, self.first_line, self.second_line
        )
    }
}

// =============================================================================
// rate_limit_duplicate_default — error emitted at parse / analyzer time when
//                                two unqualified `rate_limit` lines appear on
//                                one command. Only one default is allowed.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitDuplicateDefaultFinding {
    pub path: PathBuf,
    /// `<feature>.<command>` (or `<feature>.<query>` / `<feature>.<api>`).
    pub feature_command: String,
}

impl RateLimitDuplicateDefaultFinding {
    pub const CODE: &'static str = "rate_limit_duplicate_default";

    pub fn message(&self) -> String {
        format!(
            "`{}` declares two unqualified `rate_limit` lines. The first is the default; \
             subsequent unqualified declarations are ambiguous. Remove one or add an `in <env>` \
             qualifier.",
            self.feature_command
        )
    }
}

// =============================================================================
// rate_limit_no_default_with_qualifications — warning when only env-qualified
//                                              lines appear and no unqualified
//                                              default exists. Envs outside
//                                              the qualifiers fall through to
//                                              "no rate limit at all".
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitNoDefaultWithQualificationsFinding {
    pub path: PathBuf,
    /// `<feature>.<command>` (or `<feature>.<query>` / `<feature>.<api>`).
    pub feature_command: String,
}

impl RateLimitNoDefaultWithQualificationsFinding {
    pub const CODE: &'static str = "rate_limit_no_default_with_qualifications";

    pub fn message(&self) -> String {
        format!(
            "`{}` declares env-qualified `rate_limit` line(s) but no default. Requests in envs \
             not covered by your qualifiers will have NO rate limit, which may be unintentional. \
             Add an unqualified `rate_limit \"...\"` line for the default case.",
            self.feature_command
        )
    }
}

/// Closed-catalog list of legal env identifiers accepted in
/// `rate_limit "<spec>" in <env>` qualifications. Mirrored from
/// `lazuli_ir::EnvName` — adding a name here without the matching IR
/// variant + proposal is incorrect.
pub const RATE_LIMIT_ENV_CATALOG: &[&str] = &["production", "staging", "test", "dev", "local"];

/// Suggest the nearest closed-catalog env name for a typo'd identifier
/// using a small Levenshtein-distance scan (cutoff = 2). Returns the
/// best candidate or the first catalog entry as a defensive default.
pub fn suggest_env(input: &str) -> &'static str {
    let lower = input.to_ascii_lowercase();
    let mut best: Option<(usize, &'static str)> = None;
    for candidate in RATE_LIMIT_ENV_CATALOG {
        let d = levenshtein(&lower, candidate);
        match best {
            Some((current, _)) if d >= current => {}
            _ => best = Some((d, candidate)),
        }
    }
    match best {
        Some((d, name)) if d <= 2 => name,
        _ => RATE_LIMIT_ENV_CATALOG[0],
    }
}

/// Small Levenshtein distance — used only for env-name suggestions.
/// Not optimized for long strings; env identifiers are <= 10 chars.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let (m, n) = (a_bytes.len(), b_bytes.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0_usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_unknown_env_message_matches_spec() {
        let f = RateLimitUnknownEnvFinding {
            path: PathBuf::from("account.lzi"),
            feature_command: "account.register".to_owned(),
            env: "statging".to_owned(),
            suggestion: "staging".to_owned(),
        };
        assert_eq!(
            f.message(),
            "`rate_limit` on `account.register` declares `in statging`, but `statging` is not in \
             the closed catalog. Valid envs: `production`, `staging`, `test`, `dev`, `local`. \
             Did you mean `staging`?"
        );
        assert_eq!(RateLimitUnknownEnvFinding::CODE, "rate_limit_unknown_env");
    }

    #[test]
    fn rate_limit_invalid_spec_message_matches_spec() {
        let f = RateLimitInvalidSpecFinding {
            path: PathBuf::from("account.lzi"),
            feature_command: "account.register".to_owned(),
            literal: "unlimted".to_owned(),
        };
        assert_eq!(
            f.message(),
            "`rate_limit` literal `unlimted` on `account.register` doesn't parse as \
             `N per UNIT per scope`. Expected forms: `\"100 per 10 minutes per ip\"`, \
             `\"5 per second per user\"`, or `\"unlimited\"`."
        );
        assert_eq!(RateLimitInvalidSpecFinding::CODE, "rate_limit_invalid_spec");
    }

    #[test]
    fn rate_limit_duplicate_env_message_matches_spec() {
        let f = RateLimitDuplicateEnvFinding {
            path: PathBuf::from("audit.lzi"),
            feature_command: "audit.export".to_owned(),
            env: "staging".to_owned(),
            first_line: 42,
            second_line: 44,
        };
        assert_eq!(
            f.message(),
            "`audit.export` declares `rate_limit ... in staging` twice (lines 42 + 44). Each env \
             can appear at most once across all `rate_limit` declarations for a command."
        );
        assert_eq!(
            RateLimitDuplicateEnvFinding::CODE,
            "rate_limit_duplicate_env"
        );
    }

    #[test]
    fn rate_limit_duplicate_default_message_matches_spec() {
        let f = RateLimitDuplicateDefaultFinding {
            path: PathBuf::from("account.lzi"),
            feature_command: "account.register".to_owned(),
        };
        assert_eq!(
            f.message(),
            "`account.register` declares two unqualified `rate_limit` lines. The first is the \
             default; subsequent unqualified declarations are ambiguous. Remove one or add an \
             `in <env>` qualifier."
        );
        assert_eq!(
            RateLimitDuplicateDefaultFinding::CODE,
            "rate_limit_duplicate_default"
        );
    }

    #[test]
    fn rate_limit_no_default_with_qualifications_message_matches_spec() {
        let f = RateLimitNoDefaultWithQualificationsFinding {
            path: PathBuf::from("agent.lzi"),
            feature_command: "agent.suggest_tags".to_owned(),
        };
        assert_eq!(
            f.message(),
            "`agent.suggest_tags` declares env-qualified `rate_limit` line(s) but no default. \
             Requests in envs not covered by your qualifiers will have NO rate limit, which may \
             be unintentional. Add an unqualified `rate_limit \"...\"` line for the default case."
        );
        assert_eq!(
            RateLimitNoDefaultWithQualificationsFinding::CODE,
            "rate_limit_no_default_with_qualifications"
        );
    }

    #[test]
    fn env_catalog_lists_all_five() {
        assert_eq!(
            RATE_LIMIT_ENV_CATALOG,
            &["production", "staging", "test", "dev", "local"]
        );
    }

    #[test]
    fn suggest_env_finds_near_match() {
        assert_eq!(suggest_env("statging"), "staging");
        assert_eq!(suggest_env("prod"), "production");
        assert_eq!(suggest_env("tst"), "test");
        assert_eq!(suggest_env("dv"), "dev");
        // Far typo falls back to first catalog entry.
        assert_eq!(suggest_env("zzzzzzz"), "production");
    }
}
