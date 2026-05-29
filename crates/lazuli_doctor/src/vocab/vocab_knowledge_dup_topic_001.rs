//! VOCAB-KNOWLEDGE-DUP-TOPIC-001 — two `gold` docs cover the same topic in
//! one sector with no supersession relation between them.
//!
//! Trigger cue: within a single `knowledge/<sector>/`, two or more
//! `tier: gold` documents share a topic slug (the `<slug>` of
//! `NNNN-<slug>.md`) and *none* of them declares a `supersedes` / `replaces`
//! / `deprecated`-style relation. Two live "trust this" docs on the same
//! topic without a stated winner is the contradiction the proposal calls the
//! anti-`lixão` (anti-dump) concern: a reader can't tell which gold doc is
//! authoritative. The escape hatch is an explicit supersession relation —
//! the moment one doc says it `replaces` / is `deprecated` by the other, the
//! ambiguity is resolved and the rule goes silent.
//!
//! File-only check: reads frontmatter + filenames; no IR or git. Silent on
//! the canonical examples (no `knowledge` sector declared, so never driven).
//!
//! Severity: warning (category `Vocabulary`).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::knowledge_vault::{VaultDoc, scan_sector};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-DUP-TOPIC-001 finding: a cluster of unsuperseded gold
/// docs on a single topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The sector the duplicate cluster lives under.
    pub sector: String,
    /// The shared topic slug.
    pub topic: String,
    /// Paths of the colliding gold docs (sorted, ≥ 2).
    pub docs: Vec<PathBuf>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-DUP-TOPIC-001";

    /// Render the "two unsuperseded gold docs" message, listing the
    /// colliding paths.
    pub fn message(&self) -> String {
        let list = self
            .docs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{} gold docs in sector `{}` cover topic `{}` with no `supersedes`/`replaces`/\
             `deprecated` relation between them ({}). A reader cannot tell which is \
             authoritative — add a supersession relation to the older doc or merge them. \
             See docs/proposals/knowledge-sector-field.md.",
            self.docs.len(),
            self.sector,
            self.topic,
            list
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-DUP-TOPIC-001 over a sector's vault.
pub fn check(project_root: &Path, sector: &str) -> Vec<Finding> {
    let docs = scan_sector(project_root, sector);
    check_docs(&docs, sector)
}

/// Pure core: group gold docs by topic, flag any topic with ≥ 2 docs where
/// no member declares a supersession relation.
pub fn check_docs(docs: &[VaultDoc], sector: &str) -> Vec<Finding> {
    // topic_slug -> (paths, any_member_has_supersession)
    let mut by_topic: BTreeMap<String, (Vec<PathBuf>, bool)> = BTreeMap::new();
    for d in docs.iter().filter(|d| d.is_gold()) {
        let entry = by_topic.entry(d.topic_slug.clone()).or_default();
        entry.0.push(d.path.clone());
        entry.1 |= d.has_supersession;
    }

    let mut out = Vec::new();
    for (topic, (mut paths, any_supersession)) in by_topic {
        if paths.len() >= 2 && !any_supersession {
            paths.sort();
            out.push(Finding {
                sector: sector.to_string(),
                topic,
                docs: paths,
            });
        }
    }
    out.sort_by(|a, b| a.topic.cmp(&b.topic));
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::knowledge_vault::{Tier, VaultDoc, sector_dir};
    use super::*;

    fn doc(path: &str, topic: &str, tier: Tier, supersedes: bool) -> VaultDoc {
        VaultDoc {
            path: PathBuf::from(path),
            tier: Some(tier),
            has_supersession: supersedes,
            revalidate_by: None,
            cites: vec![],
            topic_slug: topic.to_string(),
        }
    }

    #[test]
    fn two_gold_same_topic_no_relation_fires() {
        let docs = vec![
            doc("0001-charge.md", "charge", Tier::Gold, false),
            doc("0002-charge.md", "charge", Tier::Gold, false),
        ];
        let findings = check_docs(&docs, "billing");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].topic, "charge");
        assert_eq!(findings[0].docs.len(), 2);
        assert!(findings[0].message().contains("authoritative"));
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-DUP-TOPIC-001");
    }

    #[test]
    fn supersession_relation_silences() {
        // Newer doc declares it supersedes the old one => not a dup.
        let docs = vec![
            doc("0001-charge.md", "charge", Tier::Gold, false),
            doc("0002-charge.md", "charge", Tier::Gold, true),
        ];
        assert!(check_docs(&docs, "billing").is_empty());
    }

    #[test]
    fn distinct_topics_do_not_collide() {
        let docs = vec![
            doc("0001-charge.md", "charge", Tier::Gold, false),
            doc("0002-refund.md", "refund", Tier::Gold, false),
        ];
        assert!(check_docs(&docs, "billing").is_empty());
    }

    #[test]
    fn non_gold_dupes_are_ignored() {
        // Two drafts on the same topic are fine — only gold gates.
        let docs = vec![
            doc("0001-charge.md", "charge", Tier::Draft, false),
            doc("0002-charge.md", "charge", Tier::Gold, false),
        ];
        assert!(check_docs(&docs, "billing").is_empty(), "only one gold => no dup");
    }

    #[test]
    fn three_gold_same_topic_reports_all() {
        let docs = vec![
            doc("0001-charge.md", "charge", Tier::Gold, false),
            doc("0002-charge.md", "charge", Tier::Gold, false),
            doc("0003-charge.md", "charge", Tier::Gold, false),
        ];
        let findings = check_docs(&docs, "billing");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].docs.len(), 3);
    }

    #[test]
    fn single_gold_doc_passes() {
        let docs = vec![doc("0001-charge.md", "charge", Tier::Gold, false)];
        assert!(check_docs(&docs, "billing").is_empty());
    }

    #[test]
    fn end_to_end_disk_scan() {
        let dir = tempfile::tempdir().expect("tmp");
        let sector = sector_dir(dir.path(), "billing");
        std::fs::create_dir_all(&sector).unwrap();
        // Two gold docs, same topic slug `charge`, no relation.
        std::fs::write(sector.join("0001-charge.md"), "---\ntier: gold\n---\n").unwrap();
        std::fs::write(sector.join("0002-charge.md"), "---\ntier: gold\n---\n").unwrap();
        // A third gold doc on a different topic — must not collide.
        std::fs::write(sector.join("0003-refund.md"), "---\ntier: gold\n---\n").unwrap();
        let findings = check(dir.path(), "billing");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].topic, "charge");
    }

    #[test]
    fn end_to_end_supersession_silences() {
        let dir = tempfile::tempdir().expect("tmp");
        let sector = sector_dir(dir.path(), "billing");
        std::fs::create_dir_all(&sector).unwrap();
        std::fs::write(sector.join("0001-charge.md"), "---\ntier: gold\n---\n").unwrap();
        std::fs::write(
            sector.join("0002-charge.md"),
            "---\ntier: gold\nreplaces: 0001-charge\n---\n",
        )
        .unwrap();
        assert!(check(dir.path(), "billing").is_empty());
    }
}
