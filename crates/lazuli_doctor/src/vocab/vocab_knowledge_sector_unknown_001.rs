//! VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001 — `knowledge <sector>` names a sector
//! that is neither part of the closed core catalog, declared in
//! `Lazurite.toml [knowledge.sectors]`, nor backed by an on-disk folder.
//!
//! Trigger cue: a feature declares `knowledge <sector>` but the slug is
//! UNKNOWN — i.e. it is NONE of:
//!   * a member of the closed [`CORE_KNOWLEDGE_SECTORS`] catalog
//!     (`decisions`, `changes`, `gaps`, `lazuli-way`), nor
//!   * a custom sector declared under `[knowledge.sectors]` in
//!     `Lazurite.toml`, nor
//!   * a sector with a `knowledge/<sector>/` folder on disk.
//!
//! The design is a CLOSED CORE of opinionated sectors plus GOVERNED
//! flexibility (declare-or-scaffold) — not a free-for-all dialect. A
//! finding therefore means a typo, or an undeclared/unscaffolded sector.
//! This is the grammar ↔ catalog/file cross-check of the
//! `VOCAB-KNOWLEDGE-*` family — the direct sibling of
//! `VOCAB-CONTEXT-CTXMD-001`, which validates `attach_ctx` against the `.md`
//! on disk.
//!
//! Silent when the feature declares no `knowledge` line (`feature.knowledge
//! == None`) — the canonical `full-capsule` / `production-grade` examples
//! declare no sector today, so the rule never fires there.
//!
//! Severity: warning (category `Vocabulary`, same posture as the
//! `VOCAB-CONTEXT-*` family that governs `purpose`/`non_goals`/`attach_ctx`).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use super::knowledge_vault::{is_core_sector, sector_dir};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001 finding: a declared sector with no
/// backing vault folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file declaring the `knowledge` line.
    pub path: PathBuf,
    /// Name of the feature carrying the dangling sector.
    pub feature: String,
    /// The sector slug that resolved to no folder.
    pub sector: String,
    /// The `knowledge/<sector>/` path the lint looked for.
    pub expected_dir: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001";

    /// Render the "unknown sector" message, naming the feature, the
    /// sector, and the three ways to make the reference resolve.
    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares `knowledge {}` but `{}` is an unknown sector — it is not in \
             the core catalog (decisions, changes, gaps, lazuli-way), is not declared in \
             `Lazurite.toml [knowledge.sectors]`, and has no `knowledge/{}/` vault folder ({}). \
             Create the sector folder (with at least one `NNNN-<slug>.md` doc), fix the sector \
             slug, OR declare it under `[knowledge.sectors]` in `Lazurite.toml`. \
             See docs/proposals/knowledge-sector-field.md.",
            self.feature,
            self.sector,
            self.sector,
            self.sector,
            self.expected_dir.display()
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001 for one feature.
///
/// A `knowledge <sector>` is KNOWN — and the rule stays silent — when the
/// slug is ANY of:
///   1. a member of the closed [`CORE_KNOWLEDGE_SECTORS`] catalog;
///   2. listed in `declared_sectors` (the project's `[knowledge.sectors]`
///      table from `Lazurite.toml`);
///   3. backed by a `knowledge/<sector>/` folder under `project_root`.
///
/// The finding FIRES only when the sector is NONE of those — i.e. not
/// core, not declared, and unscaffolded (a typo or an undeclared sector).
///
/// `lzi_path` anchors the finding at the declaring source file.
/// `project_root` is the resolution base for the `knowledge/` vault (the
/// same project root the doctor walker threads to
/// `vocab_context_ctxmd_001::check`). When `project_root` is `None` the rule
/// cannot perform the folder leg of the check; core/declared sectors are
/// still honored, and a non-core/undeclared sector stays silent (skip,
/// don't false-fire — the canonical `attach_ctx` precedent).
///
/// [`CORE_KNOWLEDGE_SECTORS`]: super::knowledge_vault::CORE_KNOWLEDGE_SECTORS
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_knowledge_sector_unknown_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `knowledge billing`");
/// let declared = vec!["billing".to_string()];
/// let _ = check(&feature, Path::new("billing.lzi"), Some(Path::new("/proj")), &declared);
/// ```
pub fn check(
    feature: &Feature,
    lzi_path: &Path,
    project_root: Option<&Path>,
    declared_sectors: &[String],
) -> Vec<Finding> {
    // Silent unless the feature actually declares a sector.
    let Some(sector) = feature.knowledge.as_deref() else {
        return Vec::new();
    };
    let sector = sector.trim();
    if sector.is_empty() {
        return Vec::new();
    }

    // Leg 1 — closed core catalog. Always KNOWN, even with no folder.
    if is_core_sector(sector) {
        return Vec::new();
    }
    // Leg 2 — declared in `Lazurite.toml [knowledge.sectors]`. KNOWN even
    // before the folder is scaffolded (governed flexibility).
    if declared_sectors.iter().any(|s| s.trim() == sector) {
        return Vec::new();
    }
    // Leg 3 — backing folder on disk (the original, back-compatible
    // predicate). Without a project root we cannot test it; a non-core,
    // undeclared sector then stays silent rather than false-firing.
    let Some(root) = project_root else {
        return Vec::new();
    };
    let dir = sector_dir(root, sector);
    if dir.is_dir() {
        return Vec::new();
    }

    vec![Finding {
        path: lzi_path.to_path_buf(),
        feature: feature.name.clone(),
        sector: sector.to_string(),
        expected_dir: dir,
    }]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::knowledge_vault::sector_dir;
    use super::*;
    use lazuli_ir::{Defaults, Feature, Policies};

    fn mk_feature(name: &str, knowledge: Option<&str>) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: knowledge.map(|s| s.to_owned()),
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

    /// Build a temp project root + `.lzi` path, mirroring
    /// `vocab_context_ctxmd_001::temp_setup`.
    fn temp_setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let lzi = dir.path().join("features").join("billing");
        std::fs::create_dir_all(&lzi).expect("mkdir feature");
        let lzi = lzi.join("billing.lzi");
        std::fs::write(&lzi, "feature billing\n  knowledge billing\n").expect("seed lzi");
        (dir, lzi)
    }

    /// No declared sectors — the common case before any
    /// `[knowledge.sectors]` table exists.
    const NONE: &[String] = &[];

    #[test]
    fn no_knowledge_line_is_silent() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", None);
        assert!(check(&feature, &lzi, Some(dir.path()), NONE).is_empty());
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001");
    }

    #[test]
    fn undeclared_noncore_sector_with_missing_folder_fires() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("billing"));
        let findings = check(&feature, &lzi, Some(dir.path()), NONE);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sector, "billing");
        let msg = findings[0].message();
        assert!(msg.contains("knowledge/billing/"));
        assert!(msg.contains("[knowledge.sectors]"));
        assert!(msg.contains("unknown sector"));
    }

    #[test]
    fn declared_sector_with_existing_folder_passes() {
        let (dir, lzi) = temp_setup();
        std::fs::create_dir_all(sector_dir(dir.path(), "billing")).expect("mk vault");
        let feature = mk_feature("billing", Some("billing"));
        assert!(check(&feature, &lzi, Some(dir.path()), NONE).is_empty());
    }

    /// Back-compat leg: a non-core, undeclared sector with a folder on
    /// disk still passes (existing folder-based usage is preserved).
    #[test]
    fn folder_exists_back_compat_passes() {
        let (dir, lzi) = temp_setup();
        std::fs::create_dir_all(sector_dir(dir.path(), "billing")).expect("mk vault");
        let feature = mk_feature("billing", Some("billing"));
        assert!(check(&feature, &lzi, Some(dir.path()), NONE).is_empty());
    }

    /// Core-catalog leg: a core sector with NO folder does NOT fire.
    #[test]
    fn core_sector_without_folder_passes() {
        let (dir, lzi) = temp_setup();
        for core in ["decisions", "changes", "gaps", "lazuli-way"] {
            let feature = mk_feature("billing", Some(core));
            assert!(
                check(&feature, &lzi, Some(dir.path()), NONE).is_empty(),
                "core sector `{core}` must be known without a folder",
            );
        }
    }

    /// Declared-sector leg: a custom sector named in `[knowledge.sectors]`
    /// with NO folder does NOT fire.
    #[test]
    fn declared_custom_sector_without_folder_passes() {
        let (dir, lzi) = temp_setup();
        let declared = vec!["billing".to_string(), "compliance".to_string()];
        let feature = mk_feature("billing", Some("billing"));
        assert!(check(&feature, &lzi, Some(dir.path()), &declared).is_empty());
    }

    #[test]
    fn empty_sector_slug_is_silent() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("   "));
        assert!(check(&feature, &lzi, Some(dir.path()), NONE).is_empty());
    }

    #[test]
    fn missing_project_root_skips_noncore() {
        let (_dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("billing"));
        // No project root => cannot test the folder leg => a non-core,
        // undeclared sector stays silent (skip, don't fire).
        assert!(check(&feature, &lzi, None, NONE).is_empty());
    }

    #[test]
    fn missing_project_root_still_honors_core() {
        let (_dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("decisions"));
        // Core sectors are known regardless of folder resolution.
        assert!(check(&feature, &lzi, None, NONE).is_empty());
    }

    /// Tabled coverage — one row per disposition across all three legs.
    #[test]
    fn tabled_cases() {
        // (label, knowledge, declared, create_folder, expect_finding)
        let cases: &[(&str, Option<&str>, &[&str], bool, bool)] = &[
            ("no_knowledge", None, &[], false, false),
            ("empty_slug", Some("  "), &[], false, false),
            ("core_no_folder", Some("decisions"), &[], false, false),
            (
                "declared_no_folder",
                Some("billing"),
                &["billing"],
                false,
                false,
            ),
            ("undeclared_no_folder", Some("billing"), &[], false, true),
            ("undeclared_with_folder", Some("billing"), &[], true, false),
        ];
        for (label, knowledge, declared, create, expect) in cases {
            let dir = tempfile::tempdir().expect("tmp");
            let lzi = dir.path().join("f.lzi");
            std::fs::write(&lzi, "feature billing\n").unwrap();
            if *create {
                std::fs::create_dir_all(sector_dir(dir.path(), "billing")).unwrap();
            }
            let declared: Vec<String> = declared.iter().map(|s| s.to_string()).collect();
            let feature = mk_feature("billing", *knowledge);
            let got = !check(&feature, &lzi, Some(dir.path()), &declared).is_empty();
            assert_eq!(got, *expect, "case `{label}`");
        }
    }
}
