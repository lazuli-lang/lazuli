//! Doctor rule categories — taxonomy used by severity overrides, JSON
//! grouping (`summary.by_category`), and per-profile escalation.
//!
//! Wave 0.5 introduces this enum so the framework can express
//! "TestDiscipline rules behave differently at `strict` than other
//! categories". Before this enum, `doctor_rule_severity()` was a single
//! global function that applied one mapping to every rule. See
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5.
//!
//! Variants mirror the existing module layout under `lazuli_doctor/src/`
//! (`vocab/`, `correctness/`, `domain/`, etc.) plus the new
//! `test_discipline/` module introduced in this wave.

use serde::{Deserialize, Serialize};

/// Coarse taxonomy of doctor rules. Used for severity overrides and JSON
/// rollups; every diagnostic carries one (possibly defaulted via
/// `from_code_prefix`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    Vocabulary,
    Correctness,
    Security,
    TestDiscipline,
    Design,
    Encryption,
    Lifecycle,
    Domain,
    CrossFeature,
    ErrorVocab,
    Poller,
    Report,
    /// Wave 3 (rails-style-refactor) — rules that audit the Lazuli
    /// framework's own Rust source code (file size, missing rustdoc,
    /// missing `## Examples`, unpaired tests). Fires under `lazuli
    /// doctor --self` against `crates/lazuli_*/src/`. Other categories
    /// audit user `.lzi`/`.lzx` source; `InternalHygiene` is the
    /// dogfooding counterpart for the framework's own code.
    InternalHygiene,
    /// Iron-hand 4th dimension — error-handling discipline across the
    /// three layers Lazuli owns: framework Rust source (no `.unwrap()`,
    /// named error types, `#[non_exhaustive]` pub error enums), `.lzi`
    /// contract (exhaustive `errors:`, messageKey, HTTP status, retriable
    /// classification, audit emit), `.lzx` UX (on_error wiring, empty
    /// states, field validation mapping), and Go handlers (no `panic`,
    /// no string errors, `%w` chain preservation). Fires under both
    /// `lazuli doctor` (user `.lzi`/`.lzx`/handlers) and `--self`
    /// (framework Rust + bridged Go).
    ErrorHandling,
    /// `.lzi` source-shape hygiene — file size warnings, file/feature
    /// name alignment, and cohesion-based multi-feature gating. Catches
    /// patterns that hurt AI-first cold-read without forcing the
    /// 1-feature-per-file dogma (cohesion-aware: bundles of related
    /// features sharing anchor/resource/domain pass). Fires under
    /// `lazuli doctor <path>` (user .lzi source), never under `--self`.
    LziHygiene,
}

impl RuleCategory {
    /// Map a rule code prefix to its canonical category.
    ///
    /// Used by diagnostic construction sites that have not yet been
    /// migrated to set `category` explicitly. The Wave 0.5 risk note
    /// (see proposal) calls out that the rewrite is mechanical but
    /// error-prone across ~300 sites; this helper lets the migration
    /// proceed site-by-site over follow-up cells without blocking
    /// Wave 1.
    ///
    /// The fallback is `Vocabulary` — the largest existing module — so
    /// unmigrated codes land in the broadest bucket rather than
    /// accidentally claiming a narrower category like `Security`.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::RuleCategory;
    ///
    /// assert_eq!(
    ///     RuleCategory::from_code_prefix("TEST-MISSING-AUTHORED-001"),
    ///     RuleCategory::TestDiscipline
    /// );
    /// assert_eq!(
    ///     RuleCategory::from_code_prefix("INTERNAL-UNDOC-PUB-001"),
    ///     RuleCategory::InternalHygiene
    /// );
    /// // Unknown prefix falls back to Vocabulary (auditor flags later).
    /// assert_eq!(
    ///     RuleCategory::from_code_prefix("unknown-code"),
    ///     RuleCategory::Vocabulary
    /// );
    /// ```
    pub fn from_code_prefix(code: &str) -> Self {
        match code.split('-').next() {
            Some("TEST") | Some("DOCTOR") => Self::TestDiscipline,
            Some("VOCAB") | Some("MONEY") => Self::Vocabulary,
            // `SESSION-*` — the session-cookie transport family
            // (`SESSION-COOKIE-INSECURE-IN-PROD-001`,
            // `SESSION-COOKIE-SAMESITE-NONE-INSECURE-001`,
            // `SESSION-COOKIE-MISSING-001`,
            // `SESSION-COOKIE-PROFILE-CONFLICT-001`,
            // `SESSION-COOKIE-HOST-PREFIX-VIOLATION-001`). They audit the
            // `auth.sessions.cookie` transport envelope, so they join the
            // cookie-hygiene peers under Security alongside `AUTH-*`.
            Some("SECURITY") | Some("FIELD") | Some("WEBHOOK") | Some("AUTH") | Some("SESSION") => {
                Self::Security
            }
            Some("HOOK") | Some("DUPLICATE") | Some("ROUTE") | Some("UPDATES")
            | Some("MUTATION") | Some("MISSING") | Some("MANUAL") | Some("IMPORT")
            | Some("CAP") | Some("SCHEMA") => Self::Correctness,
            // `COMPOSE-*` — the `query.compose` composite-read structural /
            // security codes (`docs/proposals/ir-composite-read-primitive-2026-05-29.md`
            // §7). They make a malformed composite read a build-time
            // failure; route them to Correctness alongside the other
            // query-shape checks (rather than the Vocabulary catch-all) so
            // `summary.by_category` + `--fail-on category:correctness` bucket
            // them correctly.
            Some("COMPOSE") => Self::Correctness,
            // `LZX-*` — `.lzx` ViewModel-surface rules (route binding, view
            // mode, tab/wizard refs, cells-mixed-form, arrow-glyph-mixed
            // hygiene). They audit user `.lzx` source shape; route them to
            // Correctness alongside the other surface-shape checks rather than
            // letting them fall through to the Vocabulary catch-all.
            Some("LZX") => Self::Correctness,
            Some("LIFECYCLE") => Self::Lifecycle,
            Some("DOMAIN") => Self::Domain,
            Some("CROSS") => Self::CrossFeature,
            // Existing error-vocabulary rules use the `ERR-VOCAB-*` prefix
            // (live codes: ERR-VOCAB-001..003, ERR-VOCAB-CODE-UNKNOWN, etc.).
            Some("ERR") => Self::ErrorVocab,
            // Iron-hand 4th dimension — `.lzi` / `.lzx` / Go handler error
            // contract rules.
            Some("ERROR") => Self::ErrorHandling,
            Some("POLLER") => Self::Poller,
            Some("REPORT") => Self::Report,
            Some("DESIGN") => Self::Design,
            Some("ENCRYPTION") | Some("ENCRYPT") => Self::Encryption,
            Some("INTERNAL") => match code.split('-').nth(1) {
                // INTERNAL-PANIC-* / INTERNAL-ERROR-* belong to error_handling,
                // not the rails-style hygiene bucket. Discriminator on the
                // second segment.
                Some("PANIC") | Some("ERROR") => Self::ErrorHandling,
                _ => Self::InternalHygiene,
            },
            Some("HANDLER") => Self::ErrorHandling,
            // JOB-* — runtime-execution gaps on `job` bodies (today:
            // `JOB-DECLARATIVE-BODY-UNSUPPORTED-001`). A declarative job
            // body that lowers to a no-op silently drops the declared
            // work — the job-level twin of a swallowed handler error, so
            // it lands in the error-handling 4th dimension alongside the
            // `HANDLER-*` family (and is preset-governed by
            // `[doctor.error_handling]`).
            Some("JOB") => Self::ErrorHandling,
            Some("LZI") => Self::LziHygiene,
            _ => Self::Vocabulary, // safe fallback; auditor flags
        }
    }

    /// Wave 2.2 — parse a category name from CLI `--fail-on category:<name>`.
    /// Accepts both `snake_case` (canonical, matches `as_str()`) and
    /// `PascalCase` (matches the enum variant name) for ergonomics.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::RuleCategory;
    ///
    /// assert_eq!(RuleCategory::parse("test_discipline"), Some(RuleCategory::TestDiscipline));
    /// assert_eq!(RuleCategory::parse("TestDiscipline"), Some(RuleCategory::TestDiscipline));
    /// assert_eq!(RuleCategory::parse("nonsense"), None);
    /// ```
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim() {
            "vocabulary" | "Vocabulary" => Some(Self::Vocabulary),
            "correctness" | "Correctness" => Some(Self::Correctness),
            "security" | "Security" => Some(Self::Security),
            "test_discipline" | "TestDiscipline" => Some(Self::TestDiscipline),
            "design" | "Design" => Some(Self::Design),
            "encryption" | "Encryption" => Some(Self::Encryption),
            "lifecycle" | "Lifecycle" => Some(Self::Lifecycle),
            "domain" | "Domain" => Some(Self::Domain),
            "cross_feature" | "CrossFeature" => Some(Self::CrossFeature),
            "error_vocab" | "ErrorVocab" => Some(Self::ErrorVocab),
            "poller" | "Poller" => Some(Self::Poller),
            "report" | "Report" => Some(Self::Report),
            "internal_hygiene" | "InternalHygiene" => Some(Self::InternalHygiene),
            "error_handling" | "ErrorHandling" => Some(Self::ErrorHandling),
            "lzi_hygiene" | "LziHygiene" => Some(Self::LziHygiene),
            _ => None,
        }
    }

    /// Stable snake_case identifier for JSON serialization and TOML
    /// override keys.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::RuleCategory;
    ///
    /// assert_eq!(RuleCategory::Vocabulary.as_str(), "vocabulary");
    /// assert_eq!(RuleCategory::TestDiscipline.as_str(), "test_discipline");
    /// assert_eq!(RuleCategory::InternalHygiene.as_str(), "internal_hygiene");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Correctness => "correctness",
            Self::Security => "security",
            Self::TestDiscipline => "test_discipline",
            Self::Design => "design",
            Self::Encryption => "encryption",
            Self::Lifecycle => "lifecycle",
            Self::Domain => "domain",
            Self::CrossFeature => "cross_feature",
            Self::ErrorVocab => "error_vocab",
            Self::Poller => "poller",
            Self::Report => "report",
            Self::InternalHygiene => "internal_hygiene",
            Self::ErrorHandling => "error_handling",
            Self::LziHygiene => "lzi_hygiene",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_routes_to_test_discipline() {
        assert_eq!(
            RuleCategory::from_code_prefix("TEST-MISSING-AUTHORED-001"),
            RuleCategory::TestDiscipline
        );
        assert_eq!(
            RuleCategory::from_code_prefix("DOCTOR-OVERRIDE-NEEDS-REASON-001"),
            RuleCategory::TestDiscipline
        );
    }

    #[test]
    fn vocab_prefix_routes_to_vocabulary() {
        assert_eq!(
            RuleCategory::from_code_prefix("VOCAB-TESTS-MISSING-001"),
            RuleCategory::Vocabulary
        );
        assert_eq!(
            RuleCategory::from_code_prefix("MONEY-COMPARE-001"),
            RuleCategory::Vocabulary
        );
    }

    #[test]
    fn internal_prefix_routes_to_internal_hygiene() {
        assert_eq!(
            RuleCategory::from_code_prefix("INTERNAL-FILE-SIZE-001"),
            RuleCategory::InternalHygiene
        );
        assert_eq!(
            RuleCategory::from_code_prefix("INTERNAL-UNDOC-PUB-001"),
            RuleCategory::InternalHygiene
        );
    }

    #[test]
    fn error_handling_prefix_routing() {
        // Live error-vocab catalog stays in ErrorVocab.
        assert_eq!(
            RuleCategory::from_code_prefix("ERR-VOCAB-001"),
            RuleCategory::ErrorVocab
        );
        // New iron-hand 4th-dimension prefixes route to ErrorHandling.
        assert_eq!(
            RuleCategory::from_code_prefix("ERROR-DECLARED-EXHAUSTIVE-001"),
            RuleCategory::ErrorHandling
        );
        assert_eq!(
            RuleCategory::from_code_prefix("HANDLER-NO-PANIC-001"),
            RuleCategory::ErrorHandling
        );
        // JOB-* — declarative job bodies the runtime can't execute route
        // to the error-handling 4th dimension (preset-governed).
        assert_eq!(
            RuleCategory::from_code_prefix("JOB-DECLARATIVE-BODY-UNSUPPORTED-001"),
            RuleCategory::ErrorHandling
        );
        // INTERNAL-PANIC-* and INTERNAL-ERROR-* are error_handling, not
        // internal_hygiene — they cover error-handling discipline on the
        // framework Rust source.
        assert_eq!(
            RuleCategory::from_code_prefix("INTERNAL-PANIC-UNWRAP-001"),
            RuleCategory::ErrorHandling
        );
        assert_eq!(
            RuleCategory::from_code_prefix("INTERNAL-ERROR-NAMING-001"),
            RuleCategory::ErrorHandling
        );
        // Other INTERNAL-* prefixes (rails-style hygiene) stay put.
        assert_eq!(
            RuleCategory::from_code_prefix("INTERNAL-UNDOC-PUB-001"),
            RuleCategory::InternalHygiene
        );
    }

    #[test]
    fn error_handling_parses_both_cases() {
        assert_eq!(
            RuleCategory::parse("error_handling"),
            Some(RuleCategory::ErrorHandling)
        );
        assert_eq!(
            RuleCategory::parse("ErrorHandling"),
            Some(RuleCategory::ErrorHandling)
        );
    }

    #[test]
    fn lzi_hygiene_prefix_and_parse() {
        // LZI-* codes route to the new LziHygiene category.
        assert_eq!(
            RuleCategory::from_code_prefix("LZI-FILE-SIZE-001"),
            RuleCategory::LziHygiene
        );
        assert_eq!(
            RuleCategory::from_code_prefix("LZI-FEATURE-NAMING-MATCHES-FILE-001"),
            RuleCategory::LziHygiene
        );
        assert_eq!(
            RuleCategory::from_code_prefix("LZI-FEATURE-COHESION-001"),
            RuleCategory::LziHygiene
        );
        // Both casings round-trip through parse().
        assert_eq!(
            RuleCategory::parse("lzi_hygiene"),
            Some(RuleCategory::LziHygiene)
        );
        assert_eq!(
            RuleCategory::parse("LziHygiene"),
            Some(RuleCategory::LziHygiene)
        );
        // Serde snake_case round-trip.
        assert_eq!(RuleCategory::LziHygiene.as_str(), "lzi_hygiene");
    }

    #[test]
    fn session_cookie_prefix_routes_to_security() {
        // The session-cookie transport family routes to Security (joins
        // the `AUTH-*` / cookie-hygiene peers).
        for code in [
            "SESSION-COOKIE-INSECURE-IN-PROD-001",
            "SESSION-COOKIE-SAMESITE-NONE-INSECURE-001",
            "SESSION-COOKIE-MISSING-001",
            "SESSION-COOKIE-PROFILE-CONFLICT-001",
            "SESSION-COOKIE-HOST-PREFIX-VIOLATION-001",
        ] {
            assert_eq!(
                RuleCategory::from_code_prefix(code),
                RuleCategory::Security,
                "{code} should route to Security"
            );
        }
    }

    #[test]
    fn lzx_prefix_routes_to_correctness() {
        // `.lzx` surface-shape rules (correctness family) — both the
        // uppercase numbered codes and the new arrow-glyph hygiene rule.
        assert_eq!(
            RuleCategory::from_code_prefix("LZX-ARROW-GLYPH-MIXED-001"),
            RuleCategory::Correctness
        );
        assert_eq!(
            RuleCategory::from_code_prefix("LZX-WIZARD-STEPS-EXPR-001"),
            RuleCategory::Correctness
        );
        assert_eq!(
            RuleCategory::from_code_prefix("LZX-ROUTE-001"),
            RuleCategory::Correctness
        );
    }

    #[test]
    fn internal_hygiene_parses_both_cases() {
        assert_eq!(
            RuleCategory::parse("internal_hygiene"),
            Some(RuleCategory::InternalHygiene)
        );
        assert_eq!(
            RuleCategory::parse("InternalHygiene"),
            Some(RuleCategory::InternalHygiene)
        );
    }

    #[test]
    fn unknown_prefix_falls_back_to_vocabulary() {
        assert_eq!(
            RuleCategory::from_code_prefix("unknown-code"),
            RuleCategory::Vocabulary
        );
        assert_eq!(RuleCategory::from_code_prefix(""), RuleCategory::Vocabulary);
    }

    #[test]
    fn as_str_round_trips_via_serde() {
        let cats = [
            RuleCategory::Vocabulary,
            RuleCategory::TestDiscipline,
            RuleCategory::Security,
        ];
        for cat in cats {
            let json = serde_json::to_string(&cat).unwrap();
            assert!(
                json.contains(cat.as_str()),
                "{json} should contain {}",
                cat.as_str()
            );
        }
    }
}
