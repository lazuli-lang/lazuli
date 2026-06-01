//! `LZI-FEATURE-COHESION-002` — flag a feature whose intra-feature
//! resource graph splits into ≥2 disconnected components.
//!
//! Fires when a single `feature`'s resources form two or more clusters
//! with no FK / `has_many` / `on_delete` edge between them — i.e. the
//! feature bundles independent capabilities under one name. This is the
//! resource-graph **sibling** of `LZI-FEATURE-COHESION-001` (which is
//! about multiple `feature` blocks per file lacking a shared name
//! prefix); the two are complementary and the -001 file is left
//! untouched.
//!
//! ## Default severity
//!
//! `Warning`. Under `tdd-iron-hand`: `Error` (preset escalation). The
//! finding is *non-waivable-in-spirit*: `# doctor:allow
//! LZI-FEATURE-COHESION-002` is honored mechanically (uniform
//! allow-comment contract, spec 0007), but the message states plainly
//! that you cannot honestly waive "these resources have no
//! relationship" — the fix is to split the feature.
//!
//! ## Example fires
//!
//! - hostpoint `platform.lzi`: `LegalDoc`, `PlatformConfig`,
//!   `DataRequest` — three isolated nodes, 0 edges → 3 clusters → fires.
//!
//! ## Example silent
//!
//! - hostpoint `account.lzi`: large, but one connected resource graph →
//!   1 component → silent (LOC is not the signal here).
//!
//! ## Info-level companions
//!
//! Two softer cross-feature heuristics ride along (info, never warn):
//! `uses` fan-out ≥4 (grab-bag candidate) and cross-feature
//! resource-name similarity ≥0.7 (duplicated-concern hint).

use std::path::PathBuf;

use lazuli_ir::Feature;

use crate::allow_comment::source_contains_doctor_allow;
use crate::lzi_hygiene::cohesion_graph::components;

/// One lowered `.lzi` feature paired with its workspace-relative path and
/// raw source. The caller (`lazuli_doctor_run` aggregator) lowers each
/// `.lzi` via `lazuli_analyzer::lower_feature_skeleton` — this crate
/// stays free of a runtime `lazuli_analyzer` dependency. `source` is
/// retained so the rule can honor the `# doctor:allow` opt-out.
pub struct LoweredFeature<'a> {
    /// Path relative to the workspace root (diagnostic anchor).
    pub path: PathBuf,
    /// The lowered IR feature.
    pub feature: &'a Feature,
    /// Raw `.lzi` source — scanned for the `# doctor:allow` comment.
    pub source: &'a str,
}

impl<'a> LoweredFeature<'a> {
    /// Convenience constructor.
    pub fn new(path: impl Into<PathBuf>, feature: &'a Feature, source: &'a str) -> Self {
        Self {
            path: path.into(),
            feature,
            source,
        }
    }
}

/// `uses` fan-out at/above which the info-level grab-bag-candidate
/// companion fires.
pub const USES_FANOUT_THRESHOLD: usize = 4;

/// Normalized name-similarity at/above which the info-level
/// duplicated-concern companion fires.
pub const NAME_SIMILARITY_THRESHOLD: f64 = 0.7;

/// The verbatim "you can't honestly waive this" sentence appended to
/// every `LZI-FEATURE-COHESION-002` diagnostic body. Pinned as a const
/// so the grader convention can match on it exactly.
pub const NON_WAIVABLE_NOTE: &str = "You can't honestly waive this: these resources have no \
     relationship. The fix is to split the feature, not to suppress the \
     finding (see `docs/lazuli_way/one-feature-one-capability.md`).";

/// One feature whose resource graph has ≥2 disconnected components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Feature name.
    pub feature: String,
    /// One sorted cluster of resource names per disconnected component.
    pub clusters: Vec<Vec<String>>,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "LZI-FEATURE-COHESION-002";

    /// Render the doctor-formatted diagnostic message: one named cluster
    /// line per component plus the verbatim non-waivable note.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::feature_cohesion_002::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/platform/platform.lzi"),
    ///     feature: "platform".to_string(),
    ///     clusters: vec![
    ///         vec!["LegalDoc".to_string()],
    ///         vec!["DataRequest".to_string()],
    ///         vec!["PlatformConfig".to_string()],
    ///     ],
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("Cluster 1: LegalDoc"));
    /// assert!(msg.contains("split the feature"));
    /// ```
    pub fn message(&self) -> String {
        let clusters_rendered: Vec<String> = self
            .clusters
            .iter()
            .enumerate()
            .map(|(i, members)| format!("Cluster {}: {}", i + 1, members.join(", ")))
            .collect();
        format!(
            "feature `{}` ({}): its resources split into {} disconnected clusters — \
             {} — no FK / has_many / on_delete edge connects them. {}",
            self.feature,
            self.path.display(),
            self.clusters.len(),
            clusters_rendered.join("; "),
            NON_WAIVABLE_NOTE,
        )
    }
}

/// An info-level cross-feature heuristic companion of
/// `LZI-FEATURE-COHESION-002`. Softer than the warn-level connectivity
/// finding (a hub feature can legitimately fan out; two features can
/// legitimately each carry a `Config`), so these inform, never warn.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoFinding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Feature name.
    pub feature: String,
    /// Which heuristic produced this hint.
    pub kind: InfoKind,
}

/// Sub-classification of the info-level companion hints.
#[derive(Debug, Clone, PartialEq)]
pub enum InfoKind {
    /// `uses` fan-out ≥4 — grab-bag candidate.
    UsesFanout { count: usize },
    /// Cross-feature resource-name similarity ≥0.7 — duplicated-concern
    /// hint. Carries the two similar resource names (this feature's,
    /// and the other feature's qualified `feature.Resource`).
    NameCollision {
        this_resource: String,
        other: String,
        similarity: f64,
    },
}

impl InfoFinding {
    /// Stable rule code shared by both info companions.
    pub const CODE: &'static str = "LZI-FEATURE-COHESION-002-INFO";

    /// Render the info-level message for whichever heuristic fired.
    pub fn message(&self) -> String {
        match &self.kind {
            InfoKind::UsesFanout { count } => format!(
                "feature `{}` ({}): grab-bag candidate — depends on {} other features via `uses` \
                 (≥{}). A feature that fans out across many others may be coordinating unrelated \
                 work; consider whether it is one capability.",
                self.feature,
                self.path.display(),
                count,
                USES_FANOUT_THRESHOLD,
            ),
            InfoKind::NameCollision {
                this_resource,
                other,
                similarity,
            } => format!(
                "feature `{}` ({}): duplicated-concern hint — resource `{}` ~ `{}` \
                 (name similarity {:.2} ≥ {:.2}). Two features naming near-identical resources \
                 may be splitting one concern; confirm they are genuinely distinct.",
                self.feature,
                self.path.display(),
                this_resource,
                other,
                similarity,
                NAME_SIMILARITY_THRESHOLD,
            ),
        }
    }
}

/// Run `LZI-FEATURE-COHESION-002` against the pre-lowered features.
/// Returns one finding per feature whose resource graph has ≥2
/// disconnected components, honoring the `# doctor:allow` opt-out.
///
/// The caller lowers each `.lzi` (the `lazuli_doctor` crate has no
/// runtime `lazuli_analyzer` dependency, so lowering happens in the
/// `lazuli_doctor_run` aggregator and in tests).
pub fn check(features: &[LoweredFeature<'_>]) -> Vec<Finding> {
    let mut out = Vec::new();
    for lf in features {
        if source_contains_doctor_allow(lf.source, Finding::CODE) {
            continue;
        }
        let clusters = components(lf.feature);
        if clusters.len() >= 2 {
            out.push(Finding {
                path: lf.path.clone(),
                feature: lf.feature.name.clone(),
                clusters,
            });
        }
    }
    out
}

/// Run the two info-level companion heuristics across the pre-lowered
/// features: `uses` fan-out ≥4, and cross-feature resource-name
/// similarity ≥0.7. Cross-feature name comparison needs the whole
/// corpus, so this is computed once over all features.
pub fn check_info(features: &[LoweredFeature<'_>]) -> Vec<InfoFinding> {
    let mut out = Vec::new();

    // Pass 1 — `uses` fan-out ≥ threshold.
    for lf in features {
        let count = lf.feature.uses.len();
        if count >= USES_FANOUT_THRESHOLD {
            out.push(InfoFinding {
                path: lf.path.clone(),
                feature: lf.feature.name.clone(),
                kind: InfoKind::UsesFanout { count },
            });
        }
    }

    // Pass 2 — cross-feature resource-name similarity ≥ threshold.
    // Compare every resource of feature i against every resource of a
    // *different* feature j. Emit one hint per (this_resource, other)
    // pair above the threshold, attributed to feature i.
    for (i, lf_i) in features.iter().enumerate() {
        for res_i in &lf_i.feature.resources {
            for (j, lf_j) in features.iter().enumerate() {
                if i == j {
                    continue;
                }
                for res_j in &lf_j.feature.resources {
                    let sim = name_similarity(&res_i.name, &res_j.name);
                    if sim >= NAME_SIMILARITY_THRESHOLD && res_i.name != res_j.name {
                        out.push(InfoFinding {
                            path: lf_i.path.clone(),
                            feature: lf_i.feature.name.clone(),
                            kind: InfoKind::NameCollision {
                                this_resource: res_i.name.clone(),
                                other: format!("{}.{}", lf_j.feature.name, res_j.name),
                                similarity: sim,
                            },
                        });
                    }
                }
            }
        }
    }

    out
}

/// Normalized name similarity in `[0.0, 1.0]` — `1 - levenshtein /
/// max(len)` over the lowercased snake_case-normalized names. Distinct
/// names below the threshold return a low score; identical names return
/// `1.0` (but the caller filters exact matches — two features can share
/// a `Config` resource verbatim and we don't want to nag on that).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::feature_cohesion_002::name_similarity;
///
/// // Near-identical names (singular/plural, one differing token) clear
/// // the bar; genuinely different concepts do not.
/// assert!(name_similarity("WebhookEvent", "WebhookEvents") >= 0.7);
/// assert!(name_similarity("Order", "LegalDoc") < 0.7);
/// ```
pub fn name_similarity(a: &str, b: &str) -> f64 {
    let na = normalize(a);
    let nb = normalize(b);
    if na.is_empty() && nb.is_empty() {
        return 1.0;
    }
    let max_len = na.chars().count().max(nb.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein(&na, &nb);
    1.0 - (dist as f64 / max_len as f64)
}

/// Lowercase + strip underscores/dashes so `Account_Config` and
/// `accountconfig` compare equal at the token level.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '_' && *c != '-')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Plain Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }

    #[test]
    fn disconnected_three_resources_fires_with_three_clusters() {
        let src = r#"
feature platform
  resource LegalDoc
    body: Text required
  resource PlatformConfig
    key: Text required
  resource DataRequest
    email: Text required
"#;
        let feature = lower(src);
        let findings = check(&[LoweredFeature::new(
            "features/platform/platform.lzi",
            &feature,
            src,
        )]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].clusters.len(), 3);
        let msg = findings[0].message();
        assert!(msg.contains("Cluster 1"));
        assert!(msg.contains("Cluster 3"));
        assert!(msg.contains("split the feature"));
    }

    #[test]
    fn connected_feature_is_silent() {
        let src = r#"
feature shop
  resource Order
    customer: Customer required
  resource Customer
    name: Text required
"#;
        let feature = lower(src);
        assert!(check(&[LoweredFeature::new("features/shop/shop.lzi", &feature, src)]).is_empty());
    }

    #[test]
    fn doctor_allow_suppresses() {
        let src = r#"
feature platform
  # doctor:allow LZI-FEATURE-COHESION-002
  resource LegalDoc
    body: Text required
  resource PlatformConfig
    key: Text required
"#;
        let feature = lower(src);
        assert!(
            check(&[LoweredFeature::new(
                "features/platform/platform.lzi",
                &feature,
                src
            )])
            .is_empty()
        );
    }

    #[test]
    fn uses_fanout_emits_info() {
        let src = r#"
feature hub
  uses billing
  uses catalog
  uses account
  uses payments
  resource Thing
    label: Text required
"#;
        let feature = lower(src);
        let infos = check_info(&[LoweredFeature::new("features/hub/hub.lzi", &feature, src)]);
        assert!(
            infos
                .iter()
                .any(|i| matches!(i.kind, InfoKind::UsesFanout { count } if count >= 4)),
            "{infos:?}"
        );
    }

    #[test]
    fn similar_resource_names_across_features_emit_info() {
        // Near-identical resource names across two features (singular vs
        // plural) cross the ≥0.7 similarity bar → duplicated-concern hint.
        let src_a = "feature account\n  resource WebhookEvent\n    kind: Text required\n";
        let src_b = "feature billing\n  resource WebhookEvents\n    kind: Text required\n";
        let fa = lower(src_a);
        let fb = lower(src_b);
        let infos = check_info(&[
            LoweredFeature::new("features/account/account.lzi", &fa, src_a),
            LoweredFeature::new("features/billing/billing.lzi", &fb, src_b),
        ]);
        assert!(
            infos
                .iter()
                .any(|i| matches!(&i.kind, InfoKind::NameCollision { .. })),
            "{infos:?}"
        );
    }

    #[test]
    fn name_similarity_basic() {
        assert!(name_similarity("WebhookEvent", "WebhookEvents") >= 0.7);
        assert!(name_similarity("Order", "LegalDoc") < 0.7);
    }
}
