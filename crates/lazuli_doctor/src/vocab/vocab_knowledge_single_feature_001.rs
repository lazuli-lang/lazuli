//! VOCAB-KNOWLEDGE-SINGLE-FEATURE-001 — a `knowledge <sector>` slug is
//! declared by exactly ONE feature across the package.
//!
//! Fires when: a sector is meant to be a SHARED, cross-feature corpus
//! (1:N — many features draw on the same documentation vault). When a
//! sector slug is declared by exactly one feature, that's a smell: it may
//! not really be a *sector* at all — it might be feature-private context
//! that belongs in the feature's co-located `<feature>.ctx.md` convention
//! sidecar instead of a shared `knowledge/<sector>/` vault.
//!
//! This is the PACKAGE-LEVEL (cross-feature) member of the
//! `VOCAB-KNOWLEDGE-*` family: unlike the per-feature `SECTOR-UNKNOWN`
//! (grammar↔file) check, it can only be decided by counting declarations
//! across every feature in the package. It fires once per solo sector,
//! anchored at the single declaring feature's `.lzi`.
//!
//! Severity: warning (category `Vocabulary`, same posture as the rest of
//! the `VOCAB-KNOWLEDGE-*` / `VOCAB-CONTEXT-*` families).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-KNOWLEDGE-SINGLE-FEATURE-001 finding: a sector slug declared
/// by exactly one feature in the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file of the single declaring feature.
    pub path: PathBuf,
    /// Name of the one feature carrying the solo sector.
    pub feature: String,
    /// The sector slug declared by exactly one feature.
    pub sector: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-KNOWLEDGE-SINGLE-FEATURE-001";

    /// Render the "solo sector" message, naming the sector, the single
    /// declaring feature, and the cut-in/cut-out decision the author owes.
    pub fn message(&self) -> String {
        format!(
            "`knowledge {}` is declared by only one feature (`{}`). A knowledge sector is \
             meant to be a SHARED, cross-feature corpus (1:N) — a solo declaration suggests \
             this may not be a genuine shared sector but feature-private context that belongs \
             in `{}.ctx.md` instead. Either have a second feature draw on `{}`, or move the \
             material into the feature's co-located `.ctx.md`. See \
             docs/proposals/knowledge-sector-field.md.",
            self.sector, self.feature, self.feature, self.sector
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// One feature's contribution to the package-wide sector tally: the source
/// `.lzi` path (anchor) paired with the lowered `Feature` (sector slug
/// source). Mirrors the `(PathBuf, Feature)` pairing the package dispatcher
/// already lifts for the rest of the `VOCAB-KNOWLEDGE-*` family.
pub type FeatureEntry<'a> = (&'a Path, &'a Feature);

/// Run VOCAB-KNOWLEDGE-SINGLE-FEATURE-001 across the whole package.
///
/// `features` is every feature lifted from the package, each paired with the
/// `.lzi` path that declared it. The rule counts how many *distinct* features
/// declare each non-empty `knowledge <sector>` slug and fires once for every
/// sector declared by exactly one feature, anchored at that feature's `.lzi`.
///
/// Package-level by construction: a single feature in isolation can never
/// satisfy or refute the 1:N invariant, so the check is meaningless per-file
/// and lives at the package layer (called from `package_methods.rs`).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_knowledge_single_feature_001::check;
/// use lazuli_ir::Feature;
///
/// let solo: Feature = unimplemented!("a feature with `knowledge solo-sector`");
/// let _ = check(&[(Path::new("solo.lzi"), &solo)]);
/// ```
pub fn check(features: &[FeatureEntry<'_>]) -> Vec<Finding> {
    // Tally distinct declaring features per sector. We key by sector slug and
    // collect the (path, feature-name) of each declarer; a feature that
    // declares the same sector twice (impossible today — `knowledge` is a
    // single field) would still count once because the dispatcher lifts one
    // `Feature` per declaration. BTreeMap keeps the output deterministic
    // (sorted by sector slug).
    let mut declarers: BTreeMap<String, Vec<(PathBuf, String)>> = BTreeMap::new();
    for (path, feature) in features {
        let Some(sector) = feature.knowledge.as_deref() else {
            continue;
        };
        let sector = sector.trim();
        if sector.is_empty() {
            continue;
        }
        declarers
            .entry(sector.to_string())
            .or_default()
            .push((path.to_path_buf(), feature.name.clone()));
    }

    let mut out = Vec::new();
    for (sector, decls) in declarers {
        if decls.len() == 1 {
            let (path, feature) = decls.into_iter().next().expect("len == 1");
            out.push(Finding {
                path,
                feature,
                sector,
            });
        }
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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

    #[test]
    fn code_is_stable() {
        assert_eq!(Finding::CODE, "VOCAB-KNOWLEDGE-SINGLE-FEATURE-001");
    }

    #[test]
    fn solo_sector_fires_once_at_declaring_feature() {
        let a = mk_feature("alpha", Some("solo-sector"));
        let b = mk_feature("beta", None);
        let pa = Path::new("alpha.lzi");
        let pb = Path::new("beta.lzi");
        let findings = check(&[(pa, &a), (pb, &b)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sector, "solo-sector");
        assert_eq!(findings[0].feature, "alpha");
        assert_eq!(findings[0].path, pa.to_path_buf());
        assert!(findings[0].message().contains("solo-sector"));
        assert!(findings[0].message().contains("alpha.ctx.md"));
    }

    #[test]
    fn shared_sector_does_not_fire() {
        let a = mk_feature("alpha", Some("shared-sector"));
        let b = mk_feature("beta", Some("shared-sector"));
        let pa = Path::new("alpha.lzi");
        let pb = Path::new("beta.lzi");
        let findings = check(&[(pa, &a), (pb, &b)]);
        assert!(
            findings.is_empty(),
            "shared sector must not fire: {findings:?}"
        );
    }

    #[test]
    fn empty_and_absent_slugs_are_silent() {
        let a = mk_feature("alpha", None);
        let b = mk_feature("beta", Some("   "));
        let pa = Path::new("alpha.lzi");
        let pb = Path::new("beta.lzi");
        assert!(check(&[(pa, &a), (pb, &b)]).is_empty());
    }

    #[test]
    fn mixed_solo_and_shared_fires_only_for_solo() {
        // shared-sector: 2 features (silent); solo-sector: 1 feature (fires).
        let a = mk_feature("alpha", Some("shared-sector"));
        let b = mk_feature("beta", Some("shared-sector"));
        let c = mk_feature("gamma", Some("solo-sector"));
        let pa = Path::new("alpha.lzi");
        let pb = Path::new("beta.lzi");
        let pc = Path::new("gamma.lzi");
        let findings = check(&[(pa, &a), (pb, &b), (pc, &c)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].sector, "solo-sector");
        assert_eq!(findings[0].feature, "gamma");
    }
}
