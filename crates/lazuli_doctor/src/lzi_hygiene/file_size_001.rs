//! `LZI-FILE-SIZE-001` — flag `.lzi` files above the configured LOC
//! threshold.
//!
//! Default `Info` (per-rule severity); under `tdd-strict` warns; under
//! `tdd-iron-hand` errors. The thresholds (300 LOC warn / 600 LOC
//! warn-tier-2) were derived empirically from the `examples/` corpus
//! (p95 = 181 LOC at proposal time, mean = 71) plus headroom — see the
//! grader-hardener review notes in the proposal log.
//!
//! ## Why two warn tiers, no error tier
//!
//! There is no measurable LSP / parser failure mode at any `.lzi` size
//! the way `INTERNAL-FILE-SIZE-001` has at 5k Rust LOC (rust-analyzer
//! slows perceptibly). So this rule never returns an inherent `Error`
//! severity — escalation to `Error` only happens via the workspace
//! preset (`tdd-iron-hand`). The two LOC tiers stay as informative
//! metadata in the diagnostic body.
//!
//! ## Example fires
//!
//! - 320-LOC feature `.lzi` with 1 feature and 12 commands → warns at
//!   tier 1; suggests splitting a sub-feature into a separate file.
//! - 750-LOC feature `.lzi` → warns at tier 2; cold-read cost is real
//!   even for one cohesive feature.
//!
//! ## Example silent
//!
//! - 992-LOC `examples/full-capsule/full-capsule.lzi` (6 cohesive
//!   features) — fires at tier 2 since LOC > 600, BUT in conjunction
//!   with `LZI-FEATURE-COHESION-001` passing (all features share the
//!   `customer*` prefix), the recommended fix is per-feature file split,
//!   not "shrink the file". The diagnostic body says so.

use std::path::PathBuf;

use crate::lzi_hygiene::walker::LziSourceFile;

/// Two warn-band tiers for LOC. No `error` tier — see the module-doc
/// rationale: there's no observable failure mode that warrants it
/// inherently; preset escalation does the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// File above the soft threshold (default 300 LOC).
    WarnSoft,
    /// File above the hard threshold (default 600 LOC).
    WarnHard,
}

const WARN_SOFT_LOC: usize = 300;
const WARN_HARD_LOC: usize = 600;

/// One `.lzi` file above the file-size threshold.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Total line count.
    pub loc_count: usize,
    /// Which tier the file lands in.
    pub tier: Tier,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "LZI-FILE-SIZE-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::file_size_001::{Finding, Tier};
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/billing/billing.lzi"),
    ///     loc_count: 720,
    ///     tier: Tier::WarnHard,
    /// };
    /// assert!(f.message().contains("720"));
    /// assert!(f.message().contains("600"));
    /// ```
    pub fn message(&self) -> String {
        let (tier_word, limit) = match self.tier {
            Tier::WarnSoft => ("soft", WARN_SOFT_LOC),
            Tier::WarnHard => ("hard", WARN_HARD_LOC),
        };
        format!(
            "{}:{} `.lzi` exceeds the {} threshold ({} LOC; limit {} LOC). \
             Cold-read cost is real for LLM + human authors; consider \
             extracting a sub-feature into a sibling `.lzi` file, or \
             move helper rules into a separate feature block in its own \
             file. See CLAUDE.md `.lzi` hygiene section.",
            self.path.display(),
            self.loc_count,
            tier_word,
            self.loc_count,
            limit,
        )
    }
}

/// Run the rule against the pre-walked `.lzi` source files. Returns one
/// finding per file above either threshold.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::file_size_001::check;
/// use lazuli_doctor::lzi_hygiene::walker::LziSourceFile;
/// use std::path::PathBuf;
///
/// // A file under the soft threshold produces no findings.
/// let small = LziSourceFile {
///     relative_path: PathBuf::from("features/billing/billing.lzi"),
///     absolute_path: PathBuf::from("/abs/features/billing/billing.lzi"),
///     source: "feature billing\n".repeat(50),
///     loc_count: 50,
/// };
/// assert!(check(&[small]).is_empty());
///
/// // A file above the hard threshold lands in `WarnHard`.
/// let big = LziSourceFile {
///     relative_path: PathBuf::from("features/billing/billing.lzi"),
///     absolute_path: PathBuf::from("/abs/features/billing/billing.lzi"),
///     source: "feature billing\n".repeat(700),
///     loc_count: 700,
/// };
/// let findings = check(&[big]);
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check(files: &[LziSourceFile]) -> Vec<Finding> {
    use crate::allow_comment::source_contains_doctor_allow;
    let mut out = Vec::new();
    for file in files {
        if file.loc_count <= WARN_SOFT_LOC {
            continue;
        }
        // Whole-file opt-out — silence the LOC-threshold advisory when
        // the `.lzi` carries the canonical allow comment. The rule's
        // message advertises the `# doctor:allow` opt-out; this honors
        // it. Use case: features whose cohesion genuinely warrants
        // staying together (cross-feature splits would harm cold-read
        // more than the LOC tax).
        if source_contains_doctor_allow(&file.source, Finding::CODE) {
            continue;
        }
        let tier = if file.loc_count > WARN_HARD_LOC {
            Tier::WarnHard
        } else {
            Tier::WarnSoft
        };
        out.push(Finding {
            path: file.relative_path.clone(),
            loc_count: file.loc_count,
            tier,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(loc: usize) -> LziSourceFile {
        let source = "feature billing\n".repeat(loc);
        LziSourceFile {
            relative_path: PathBuf::from("features/billing/billing.lzi"),
            absolute_path: PathBuf::from("/abs/features/billing/billing.lzi"),
            source,
            loc_count: loc,
        }
    }

    #[test]
    fn under_soft_silent() {
        assert!(check(&[file(50)]).is_empty());
        assert!(check(&[file(WARN_SOFT_LOC)]).is_empty());
    }

    #[test]
    fn soft_tier_fires_just_above_300() {
        let findings = check(&[file(WARN_SOFT_LOC + 1)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::WarnSoft);
    }

    #[test]
    fn hard_tier_fires_just_above_600() {
        let findings = check(&[file(WARN_HARD_LOC + 1)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::WarnHard);
    }

    #[test]
    fn exactly_hard_threshold_stays_soft() {
        // The hard threshold is exclusive — `> WARN_HARD_LOC` means
        // exactly WARN_HARD_LOC sits in soft.
        let findings = check(&[file(WARN_HARD_LOC)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::WarnSoft);
    }

    #[test]
    fn multiple_files_yield_multiple_findings() {
        let findings = check(&[file(50), file(WARN_SOFT_LOC + 1), file(WARN_HARD_LOC + 100)]);
        assert_eq!(findings.len(), 2);
        let tiers: Vec<_> = findings.iter().map(|f| f.tier).collect();
        assert!(tiers.contains(&Tier::WarnSoft));
        assert!(tiers.contains(&Tier::WarnHard));
    }

    #[test]
    fn message_includes_loc_and_limit() {
        let findings = check(&[file(720)]);
        let msg = findings[0].message();
        assert!(msg.contains("720"));
        assert!(msg.contains("600"));
        assert!(msg.contains("hard"));
    }
}
