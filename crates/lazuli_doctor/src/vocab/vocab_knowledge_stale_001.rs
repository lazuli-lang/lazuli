//! VOCAB-KNOWLEDGE-STALE-001 — a `gold` vault doc is past its
//! `revalidate_by` date.
//!
//! Trigger cue: a `tier: gold` document in `.lazuli/knowledge/<sector>/`
//! carries a `revalidate_by:` frontmatter date that is now in the past.
//! Gold docs are the load-bearing, "trust this" tier; a lapsed revalidation
//! window means the knowledge may have decayed and must be re-reviewed (the
//! decay axis the proposal lifts from `lazuli examples validate
//! --check-decay`). Only `gold` docs gate — drafts/approved/deprecated are
//! exempt.
//!
//! File-only check: reads the vault on disk, no IR or git needed. Silent on
//! the canonical examples (they declare no `knowledge` sector, so the rule
//! is never invoked for them by the driver).
//!
//! Date comparison is lexical on the ISO `YYYY-MM-DD` form, which is
//! order-preserving — no date crate dependency (wire-thin). `today` is
//! injected by the caller (the doctor walker passes `ctx.now`'s date),
//! keeping the rule deterministic and testable instead of reading the
//! system clock inline.
//!
//! Severity: warning (category `Vocabulary`).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::path::{Path, PathBuf};

use super::knowledge_vault::{VaultDoc, scan_sector};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-STALE-001 finding: a gold doc overdue for revalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the stale `.md` document.
    pub path: PathBuf,
    /// The sector the doc lives under.
    pub sector: String,
    /// The lapsed `revalidate_by` date (verbatim ISO string).
    pub revalidate_by: String,
    /// The "today" the lint compared against (verbatim ISO string).
    pub today: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-STALE-001";

    /// Render the "gold doc overdue for revalidation" message.
    pub fn message(&self) -> String {
        format!(
            "gold knowledge doc `{}` (sector `{}`) is past its `revalidate_by: {}` (today is \
             {}). Re-review the doc and bump `revalidate_by`, or demote its tier. \
             See docs/proposals/knowledge-sector-field.md.",
            self.path.display(),
            self.sector,
            self.revalidate_by,
            self.today
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-STALE-001 over a sector's vault.
///
/// `today` is the ISO `YYYY-MM-DD` reference date. A gold doc fires when its
/// `revalidate_by` is strictly less than `today`. Docs without a
/// `revalidate_by`, non-gold docs, and docs with a malformed date are
/// skipped (the date well-formedness is not this rule's job).
pub fn check(project_root: &Path, sector: &str, today: &str) -> Vec<Finding> {
    let docs = scan_sector(project_root, sector);
    check_docs(&docs, sector, today)
}

/// Pure core over a pre-scanned doc set — lets unit tests drive in-memory
/// [`VaultDoc`]s without disk.
pub fn check_docs(docs: &[VaultDoc], sector: &str, today: &str) -> Vec<Finding> {
    let today = today.trim();
    docs.iter()
        .filter(|d| d.is_gold())
        .filter_map(|d| {
            let rev = d.revalidate_by.as_deref()?.trim();
            if rev.is_empty() || !is_iso_date(rev) {
                return None;
            }
            // Lexical compare is correct for fixed-width ISO dates.
            if rev < today {
                Some(Finding {
                    path: d.path.clone(),
                    sector: sector.to_string(),
                    revalidate_by: rev.to_string(),
                    today: today.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Shallow ISO `YYYY-MM-DD` shape check (10 chars, digits + `-` in the right
/// slots). Not a calendar validator — just enough to avoid lexical-comparing
/// free-text.
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::knowledge_vault::{Tier, VaultDoc};
    use super::*;

    fn doc(name: &str, tier: Option<Tier>, revalidate_by: Option<&str>) -> VaultDoc {
        VaultDoc {
            path: PathBuf::from(name),
            tier,
            has_supersession: false,
            revalidate_by: revalidate_by.map(str::to_string),
            cites: vec![],
            topic_slug: name.trim_end_matches(".md").to_string(),
        }
    }

    #[test]
    fn gold_doc_past_revalidation_fires() {
        let docs = vec![doc("0001-x.md", Some(Tier::Gold), Some("2025-01-01"))];
        let findings = check_docs(&docs, "billing", "2026-05-29");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].revalidate_by, "2025-01-01");
        assert!(findings[0].message().contains("past its `revalidate_by"));
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-STALE-001");
    }

    #[test]
    fn gold_doc_future_revalidation_passes() {
        let docs = vec![doc("0001-x.md", Some(Tier::Gold), Some("2099-01-01"))];
        assert!(check_docs(&docs, "billing", "2026-05-29").is_empty());
    }

    #[test]
    fn revalidate_by_equal_today_passes() {
        // strictly-less-than => equal is not stale.
        let docs = vec![doc("0001-x.md", Some(Tier::Gold), Some("2026-05-29"))];
        assert!(check_docs(&docs, "billing", "2026-05-29").is_empty());
    }

    #[test]
    fn non_gold_overdue_doc_is_exempt() {
        for tier in [Tier::Draft, Tier::Approved, Tier::Deprecated] {
            let docs = vec![doc("0001-x.md", Some(tier), Some("2000-01-01"))];
            assert!(
                check_docs(&docs, "billing", "2026-05-29").is_empty(),
                "{tier:?} should be exempt"
            );
        }
    }

    #[test]
    fn gold_doc_without_revalidate_by_passes() {
        let docs = vec![doc("0001-x.md", Some(Tier::Gold), None)];
        assert!(check_docs(&docs, "billing", "2026-05-29").is_empty());
    }

    #[test]
    fn malformed_date_is_skipped_not_fired() {
        let docs = vec![doc("0001-x.md", Some(Tier::Gold), Some("last tuesday"))];
        assert!(check_docs(&docs, "billing", "2026-05-29").is_empty());
    }

    #[test]
    fn end_to_end_disk_scan() {
        let dir = tempfile::tempdir().expect("tmp");
        let sector = super::super::knowledge_vault::sector_dir(dir.path(), "billing");
        std::fs::create_dir_all(&sector).unwrap();
        std::fs::write(
            sector.join("0001-stale.md"),
            "---\ntier: gold\nrevalidate_by: 2020-01-01\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            sector.join("0002-fresh.md"),
            "---\ntier: gold\nrevalidate_by: 2099-01-01\n---\nbody\n",
        )
        .unwrap();
        let findings = check(dir.path(), "billing", "2026-05-29");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].path.ends_with("0001-stale.md"));
    }

    #[test]
    fn tabled_cases() {
        // (label, tier, revalidate_by, today, expect)
        let cases: &[(&str, Tier, Option<&str>, &str, bool)] = &[
            ("gold_past", Tier::Gold, Some("2020-01-01"), "2026-05-29", true),
            ("gold_future", Tier::Gold, Some("2099-01-01"), "2026-05-29", false),
            ("gold_equal", Tier::Gold, Some("2026-05-29"), "2026-05-29", false),
            ("draft_past", Tier::Draft, Some("2020-01-01"), "2026-05-29", false),
            ("gold_none", Tier::Gold, None, "2026-05-29", false),
            ("gold_garbage", Tier::Gold, Some("soon"), "2026-05-29", false),
        ];
        for (label, tier, rev, today, expect) in cases {
            let docs = vec![doc("0001-x.md", Some(*tier), *rev)];
            let got = !check_docs(&docs, "s", today).is_empty();
            assert_eq!(got, *expect, "case `{label}`");
        }
    }
}
