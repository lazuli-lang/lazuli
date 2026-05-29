//! VOCAB-KNOWLEDGE-DANGLING-CITE-001 — a vault doc `cites:` a symbol that
//! does not exist in the IR.
//!
//! Trigger cue: a `knowledge/<sector>/` document lists a `cites:`
//! entry naming a feature or `feature.symbol` that the compiler does not
//! know about. A citation is a promise that the doc is anchored to a real
//! piece of the system; a dangling cite means the doc points at a renamed,
//! deleted, or never-existent symbol — exactly the drift the "memory is the
//! compiler" framing exists to catch. This is the file ↔ IR cross-check of
//! the family (sibling-in-spirit to the way `VOCAB-CONTEXT-CTXMD-001`
//! cross-checks `attach_ctx` against disk).
//!
//! Resolution: the known-symbol index is built from the lifted IR features —
//! every feature name, plus every `<feature>.<resource|command|query|event|
//! job>` qualified name (the same `<feature>.<symbol>` form `lazuli inspect`
//! accepts). A cite that doesn't match any of those, in either bare or
//! qualified form, dangles.
//!
//! Silent on the canonical examples: they declare no `knowledge` sector, so
//! the driver never scans a vault for them. Also conservative — an empty
//! symbol index (no IR features loaded) skips entirely so the rule can never
//! false-fire when the IR is unavailable.
//!
//! Severity: warning (category `Vocabulary`).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use super::knowledge_vault::{VaultDoc, scan_sector};

// ── known-symbol index ──────────────────────────────────────────────────────────

/// The set of symbols a `cites:` entry may legally name. Built once from the
/// IR and reused across every doc in a scan.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    symbols: BTreeSet<String>,
}

impl SymbolIndex {
    /// Build the index from the lifted IR features.
    ///
    /// Inserts, per feature `f`:
    ///   * `f` (bare feature name)
    ///   * `f.<resource>`, `f.<command>`, `f.<query>`, `f.<event>`, `f.<job>`
    ///
    /// Matching is case-sensitive: Lazuli symbols are case-significant
    /// (`account.User` is a resource, `catalog.list` a query).
    pub fn from_features(features: &[Feature]) -> Self {
        let mut symbols = BTreeSet::new();
        for f in features {
            symbols.insert(f.name.clone());
            let q = |name: &str| format!("{}.{}", f.name, name);
            for r in &f.resources {
                symbols.insert(q(&r.name));
            }
            for c in &f.commands {
                symbols.insert(q(&c.name));
            }
            for query in &f.queries {
                symbols.insert(q(query.name()));
            }
            for e in &f.events {
                symbols.insert(q(&e.name));
            }
            for j in &f.jobs {
                symbols.insert(q(&j.name));
            }
        }
        Self { symbols }
    }

    /// Build directly from a list of fully-qualified symbol strings — the
    /// test seam, and the entry point a future driver can use when it has a
    /// pre-computed symbol table rather than `&[Feature]`.
    pub fn from_symbols<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            symbols: iter.into_iter().map(Into::into).collect(),
        }
    }

    /// Is the index empty (no IR loaded)? The rule skips in this state.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Does this cite resolve to a known symbol?
    pub fn resolves(&self, cite: &str) -> bool {
        self.symbols.contains(cite.trim())
    }
}

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-DANGLING-CITE-001 finding: a single unresolved cite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the citing document.
    pub path: PathBuf,
    /// The sector the doc lives under.
    pub sector: String,
    /// The cite string that did not resolve.
    pub cite: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-DANGLING-CITE-001";

    /// Render the "dangling citation" message.
    pub fn message(&self) -> String {
        format!(
            "knowledge doc `{}` (sector `{}`) cites `{}`, which is not a feature or \
             `<feature>.<symbol>` known to the compiler. Fix the cite, or remove it if the \
             symbol was renamed/deleted. See docs/proposals/knowledge-sector-field.md.",
            self.path.display(),
            self.sector,
            self.cite
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-DANGLING-CITE-001 over a sector's vault.
///
/// `index` is the IR-derived known-symbol set. When the index is empty (no
/// IR features were lifted) the rule skips — it must never fire when it
/// cannot prove a symbol is absent.
pub fn check(project_root: &Path, sector: &str, index: &SymbolIndex) -> Vec<Finding> {
    let docs = scan_sector(project_root, sector);
    check_docs(&docs, sector, index)
}

/// Pure core over a pre-scanned doc set.
pub fn check_docs(docs: &[VaultDoc], sector: &str, index: &SymbolIndex) -> Vec<Finding> {
    if index.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for d in docs {
        for cite in &d.cites {
            let c = cite.trim();
            if c.is_empty() {
                continue;
            }
            if !index.resolves(c) {
                out.push(Finding {
                    path: d.path.clone(),
                    sector: sector.to_string(),
                    cite: c.to_string(),
                });
            }
        }
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::knowledge_vault::{Tier, VaultDoc, sector_dir};
    use super::*;

    fn doc(path: &str, cites: &[&str]) -> VaultDoc {
        VaultDoc {
            path: PathBuf::from(path),
            tier: Some(Tier::Gold),
            has_supersession: false,
            revalidate_by: None,
            cites: cites.iter().map(|s| s.to_string()).collect(),
            topic_slug: "x".into(),
        }
    }

    fn idx() -> SymbolIndex {
        SymbolIndex::from_symbols(["billing", "billing.charge", "billing.Invoice"])
    }

    #[test]
    fn resolving_cites_pass() {
        let docs = vec![doc("0001-x.md", &["billing", "billing.charge", "billing.Invoice"])];
        assert!(check_docs(&docs, "billing", &idx()).is_empty());
    }

    #[test]
    fn dangling_cite_fires() {
        let docs = vec![doc("0001-x.md", &["billing.ghost"])];
        let findings = check_docs(&docs, "billing", &idx());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cite, "billing.ghost");
        assert!(findings[0].message().contains("not a feature"));
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-DANGLING-CITE-001");
    }

    #[test]
    fn mixed_resolving_and_dangling_reports_only_dangling() {
        let docs = vec![doc("0001-x.md", &["billing.charge", "billing.ghost", "other.thing"])];
        let findings = check_docs(&docs, "billing", &idx());
        assert_eq!(findings.len(), 2);
        let cited: Vec<&str> = findings.iter().map(|f| f.cite.as_str()).collect();
        assert!(cited.contains(&"billing.ghost"));
        assert!(cited.contains(&"other.thing"));
    }

    #[test]
    fn case_sensitive_resolution() {
        // `billing.invoice` (lower) must NOT resolve `billing.Invoice`.
        let docs = vec![doc("0001-x.md", &["billing.invoice"])];
        let findings = check_docs(&docs, "billing", &idx());
        assert_eq!(findings.len(), 1, "symbol resolution is case-sensitive");
    }

    #[test]
    fn empty_index_skips() {
        // No IR loaded => cannot prove absence => never fire.
        let docs = vec![doc("0001-x.md", &["anything.at.all"])];
        assert!(check_docs(&docs, "billing", &SymbolIndex::default()).is_empty());
    }

    #[test]
    fn empty_cites_are_ignored() {
        let docs = vec![doc("0001-x.md", &["", "  "])];
        assert!(check_docs(&docs, "billing", &idx()).is_empty());
    }

    #[test]
    fn index_from_features_indexes_feature_name() {
        // `from_features` over a bare feature indexes at minimum the feature
        // name. Qualified-symbol indexing (`f.<resource>` etc.) is exercised
        // through the `from_symbols` seam in the other tests — `Resource` /
        // `Command` are not `Default` and their full literals would couple
        // this test to unrelated IR-struct churn.
        let feature = mk_bare_feature("billing");
        let index = SymbolIndex::from_features(std::slice::from_ref(&feature));
        assert!(index.resolves("billing"));
        assert!(!index.resolves("billing.refund"));
        assert!(!index.is_empty());
    }

    fn mk_bare_feature(name: &str) -> lazuli_ir::Feature {
        use lazuli_ir::{Defaults, Feature, Policies};
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn end_to_end_disk_scan() {
        let dir = tempfile::tempdir().expect("tmp");
        let sector = sector_dir(dir.path(), "billing");
        std::fs::create_dir_all(&sector).unwrap();
        std::fs::write(
            sector.join("0001-ok.md"),
            "---\ntier: gold\ncites: [billing.charge]\n---\n",
        )
        .unwrap();
        std::fs::write(
            sector.join("0002-bad.md"),
            "---\ntier: gold\ncites: [billing.ghost]\n---\n",
        )
        .unwrap();
        let findings = check(dir.path(), "billing", &idx());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].path.ends_with("0002-bad.md"));
        assert_eq!(findings[0].cite, "billing.ghost");
    }
}
