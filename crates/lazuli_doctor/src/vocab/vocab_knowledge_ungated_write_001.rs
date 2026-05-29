//! VOCAB-KNOWLEDGE-UNGATED-WRITE-001 — a `gold` vault doc was never gated
//! through `draft` in git history (anti-`lixão`).
//!
//! Trigger cue: a `tier: gold` document in `.lazuli/knowledge/<sector>/`
//! whose git history shows it was born gold — no prior revision ever carried
//! `tier: draft` (or `approved`). Promotion to gold is supposed to pass
//! through review; a doc that appears already-gold on its first commit
//! bypassed the gate, which is the "dump straight to authoritative"
//! anti-pattern (the anti-`lixão` concern) the proposal calls out. History
//! is git (append-only), not a bespoke engine — the rule reads prior blobs'
//! frontmatter via `git show`.
//!
//! Conservative by construction: when git is unavailable, the file is
//! untracked, or history is unreadable, [`knowledge_vault::passed_through_draft`]
//! returns `None` and this rule **skips** — it never fires on a state it
//! cannot prove, matching the test-harness "skip, don't fail when the
//! environment is unreachable" discipline. Local `cargo test` and
//! fresh-clone CI both stay green.
//!
//! Silent on the canonical examples: they declare no `knowledge` sector, so
//! no vault is scanned for them.
//!
//! Severity: warning (category `Vocabulary`).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::path::{Path, PathBuf};

use super::knowledge_vault::{VaultDoc, passed_through_draft, scan_sector};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-UNGATED-WRITE-001 finding: a gold doc that skipped the
/// draft gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the ungated `.md` document.
    pub path: PathBuf,
    /// The sector the doc lives under.
    pub sector: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-UNGATED-WRITE-001";

    /// Render the "promoted to gold without a draft phase" message.
    pub fn message(&self) -> String {
        format!(
            "gold knowledge doc `{}` (sector `{}`) was never gated through `draft` in git \
             history — it appears to have been committed already-gold, bypassing review. \
             Land docs as `tier: draft` first, then promote. \
             See docs/proposals/knowledge-sector-field.md.",
            self.path.display(),
            self.sector
        )
    }
}

// ── gate disposition ─────────────────────────────────────────────────────────────

/// Outcome of probing one doc's git history for the draft gate. Separated so
/// the firing decision is a pure function over a tri-state, unit-testable
/// without spawning git.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateProbe {
    /// A prior revision carried draft/approved — the gate was honored.
    PassedDraft,
    /// Every revision (incl. the first) was already gold — gate bypassed.
    BornGold,
    /// Git unavailable / untracked / unreadable — cannot evaluate (skip).
    Unknown,
}

impl GateProbe {
    /// Map the [`passed_through_draft`] tri-state into a `GateProbe`.
    fn from_history(history: Option<bool>) -> Self {
        match history {
            Some(true) => Self::PassedDraft,
            Some(false) => Self::BornGold,
            None => Self::Unknown,
        }
    }
}

/// Pure firing decision for one gold doc given its gate probe. Only
/// `BornGold` fires; `Unknown` skips (conservative).
pub fn fires(tier_is_gold: bool, probe: GateProbe) -> bool {
    tier_is_gold && probe == GateProbe::BornGold
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-UNGATED-WRITE-001 over a sector's vault.
///
/// `project_root` is both the vault anchor and the git working-tree root the
/// history probe runs `git -C` against.
pub fn check(project_root: &Path, sector: &str) -> Vec<Finding> {
    let docs = scan_sector(project_root, sector);
    let mut out = Vec::new();
    for d in docs.iter().filter(|d| d.is_gold()) {
        let probe = GateProbe::from_history(passed_through_draft(project_root, &d.path));
        if fires(true, probe) {
            out.push(Finding {
                path: d.path.clone(),
                sector: sector.to_string(),
            });
        }
    }
    out
}

/// Pure core over a pre-scanned doc set + an injected probe resolver. Used by
/// the end-to-end git test (real probe) and lets a future driver substitute a
/// cached history lookup.
pub fn check_docs_with<F>(docs: &[VaultDoc], sector: &str, mut probe_of: F) -> Vec<Finding>
where
    F: FnMut(&Path) -> Option<bool>,
{
    let mut out = Vec::new();
    for d in docs.iter().filter(|d| d.is_gold()) {
        let probe = GateProbe::from_history(probe_of(&d.path));
        if fires(true, probe) {
            out.push(Finding {
                path: d.path.clone(),
                sector: sector.to_string(),
            });
        }
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::knowledge_vault::{Tier, VaultDoc, sector_dir};
    use super::*;
    use std::process::Command;

    fn doc(path: &str, tier: Tier) -> VaultDoc {
        VaultDoc {
            path: PathBuf::from(path),
            tier: Some(tier),
            has_supersession: false,
            revalidate_by: None,
            cites: vec![],
            topic_slug: "x".into(),
        }
    }

    #[test]
    fn fires_decision_table() {
        assert!(fires(true, GateProbe::BornGold), "born-gold gold doc fires");
        assert!(!fires(true, GateProbe::PassedDraft), "gated doc passes");
        assert!(!fires(true, GateProbe::Unknown), "unknown history skips");
        assert!(!fires(false, GateProbe::BornGold), "non-gold never fires");
    }

    #[test]
    fn probe_maps_tri_state() {
        assert_eq!(GateProbe::from_history(Some(true)), GateProbe::PassedDraft);
        assert_eq!(GateProbe::from_history(Some(false)), GateProbe::BornGold);
        assert_eq!(GateProbe::from_history(None), GateProbe::Unknown);
    }

    #[test]
    fn check_docs_with_injected_probe() {
        let docs = vec![
            doc("0001-born-gold.md", Tier::Gold),
            doc("0002-gated.md", Tier::Gold),
            doc("0003-draft.md", Tier::Draft),
        ];
        // born-gold => None-draft history, gated => had draft, draft tier => ignored.
        let findings = check_docs_with(&docs, "billing", |p| {
            if p.ends_with("0001-born-gold.md") {
                Some(false) // never drafted
            } else {
                Some(true) // had a draft phase
            }
        });
        assert_eq!(findings.len(), 1);
        assert!(findings[0].path.ends_with("0001-born-gold.md"));
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-UNGATED-WRITE-001");
    }

    #[test]
    fn unknown_history_skips_all() {
        let docs = vec![doc("0001-x.md", Tier::Gold)];
        // git unreachable => None => skip.
        let findings = check_docs_with(&docs, "billing", |_| None);
        assert!(findings.is_empty());
    }

    #[test]
    fn non_repo_dir_skips_not_panics() {
        // `check` against a non-git temp dir must degrade to empty (the git
        // probe returns None), never panic — the "skip, don't fail" rule.
        let dir = tempfile::tempdir().expect("tmp");
        let sector = sector_dir(dir.path(), "billing");
        std::fs::create_dir_all(&sector).unwrap();
        std::fs::write(sector.join("0001-x.md"), "---\ntier: gold\n---\n").unwrap();
        assert!(check(dir.path(), "billing").is_empty());
    }

    /// Helper: run a git command in `root`, returning false if git is absent
    /// so the e2e tests can self-skip on a machine without git.
    fn git(root: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// End-to-end against a real temp git repo: a doc committed first as
    /// draft, then promoted to gold, must NOT fire. Self-skips if git is
    /// unavailable.
    #[test]
    fn e2e_gated_doc_does_not_fire() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        if !git(root, &["init", "-q"]) {
            eprintln!("git unavailable — skipping e2e_gated_doc_does_not_fire");
            return;
        }
        let sector = sector_dir(root, "billing");
        std::fs::create_dir_all(&sector).unwrap();
        let f = sector.join("0001-charge.md");

        // Commit 1: draft.
        std::fs::write(&f, "---\ntier: draft\n---\nbody\n").unwrap();
        assert!(git(root, &["add", "."]));
        assert!(git(root, &["commit", "-q", "-m", "draft"]));
        // Commit 2: promote to gold.
        std::fs::write(&f, "---\ntier: gold\n---\nbody\n").unwrap();
        assert!(git(root, &["add", "."]));
        assert!(git(root, &["commit", "-q", "-m", "promote"]));

        let findings = check(root, "billing");
        assert!(findings.is_empty(), "gated promotion must not fire: {findings:?}");
    }

    /// End-to-end: a doc that is gold on its very first commit fires.
    /// Self-skips if git is unavailable.
    #[test]
    fn e2e_born_gold_doc_fires() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        if !git(root, &["init", "-q"]) {
            eprintln!("git unavailable — skipping e2e_born_gold_doc_fires");
            return;
        }
        let sector = sector_dir(root, "billing");
        std::fs::create_dir_all(&sector).unwrap();
        let f = sector.join("0001-charge.md");

        // Single commit, already gold — bypassed the gate.
        std::fs::write(&f, "---\ntier: gold\n---\nbody\n").unwrap();
        assert!(git(root, &["add", "."]));
        assert!(git(root, &["commit", "-q", "-m", "born gold"]));

        let findings = check(root, "billing");
        assert_eq!(findings.len(), 1, "born-gold doc should fire: {findings:?}");
        assert!(findings[0].path.ends_with("0001-charge.md"));
    }
}
