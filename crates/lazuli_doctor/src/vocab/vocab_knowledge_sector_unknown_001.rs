//! VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001 — `knowledge <sector>` names a vault
//! that does not exist on disk.
//!
//! Trigger cue: a feature declares `knowledge <sector>` but no
//! `knowledge/<sector>/` folder exists under the project root. The
//! sector slug is a forward reference to a documentation vault; a missing
//! folder means the reference dangles (typo, un-scaffolded sector, or a
//! vault that moved). This is the grammar ↔ file cross-check of the
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

use super::knowledge_vault::sector_dir;

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

    /// Render the "vault folder missing" message, naming the feature, the
    /// sector, and the expected on-disk location.
    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares `knowledge {}` but no `knowledge/{}/` vault folder \
             exists ({}). Create the sector folder (with at least one `NNNN-<slug>.md` doc) or \
             fix the sector slug. See docs/proposals/knowledge-sector-field.md.",
            self.feature,
            self.sector,
            self.sector,
            self.expected_dir.display()
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001 for one feature.
///
/// `lzi_path` anchors the finding at the declaring source file.
/// `project_root` is the resolution base for the `knowledge/`
/// vault (the same project root the doctor walker threads to
/// `vocab_context_ctxmd_001::check`). When `project_root` is `None` the rule
/// cannot locate the vault root and stays silent (skip, don't false-fire).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_knowledge_sector_unknown_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `knowledge billing`");
/// let _ = check(&feature, Path::new("billing.lzi"), Some(Path::new("/proj")));
/// ```
pub fn check(feature: &Feature, lzi_path: &Path, project_root: Option<&Path>) -> Vec<Finding> {
    // Silent unless the feature actually declares a sector.
    let Some(sector) = feature.knowledge.as_deref() else {
        return Vec::new();
    };
    let sector = sector.trim();
    if sector.is_empty() {
        return Vec::new();
    }
    // Without a project root we have no vault anchor — skip rather than
    // resolve against an arbitrary cwd.
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

    #[test]
    fn no_knowledge_line_is_silent() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", None);
        assert!(check(&feature, &lzi, Some(dir.path())).is_empty());
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001");
    }

    #[test]
    fn declared_sector_with_missing_folder_fires() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("billing"));
        let findings = check(&feature, &lzi, Some(dir.path()));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sector, "billing");
        assert!(findings[0].message().contains("knowledge/billing/"));
        assert!(findings[0].message().contains("billing"));
    }

    #[test]
    fn declared_sector_with_existing_folder_passes() {
        let (dir, lzi) = temp_setup();
        std::fs::create_dir_all(sector_dir(dir.path(), "billing")).expect("mk vault");
        let feature = mk_feature("billing", Some("billing"));
        assert!(check(&feature, &lzi, Some(dir.path())).is_empty());
    }

    #[test]
    fn empty_sector_slug_is_silent() {
        let (dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("   "));
        assert!(check(&feature, &lzi, Some(dir.path())).is_empty());
    }

    #[test]
    fn missing_project_root_skips() {
        let (_dir, lzi) = temp_setup();
        let feature = mk_feature("billing", Some("billing"));
        // No project root => cannot anchor the vault => skip, don't fire.
        assert!(check(&feature, &lzi, None).is_empty());
    }

    /// Tabled coverage — one row per disposition.
    #[test]
    fn tabled_cases() {
        // (label, knowledge, create_folder, expect_finding)
        let cases: &[(&str, Option<&str>, bool, bool)] = &[
            ("no_knowledge", None, false, false),
            ("empty_slug", Some("  "), false, false),
            ("missing_folder", Some("billing"), false, true),
            ("present_folder", Some("billing"), true, false),
        ];
        for (label, knowledge, create, expect) in cases {
            let dir = tempfile::tempdir().expect("tmp");
            let lzi = dir.path().join("f.lzi");
            std::fs::write(&lzi, "feature billing\n").unwrap();
            if *create {
                std::fs::create_dir_all(sector_dir(dir.path(), "billing")).unwrap();
            }
            let feature = mk_feature("billing", *knowledge);
            let got = !check(&feature, &lzi, Some(dir.path())).is_empty();
            assert_eq!(got, *expect, "case `{label}`");
        }
    }
}
