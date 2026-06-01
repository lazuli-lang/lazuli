//! `LZI-FILE-SIZE-001` — flag a feature whose declarative surface area
//! is large, keyed off distinct `(resource × effect)` pairs rather than
//! raw line count.
//!
//! ## Why `(resource × effect)`, not LOC (spec 0008 re-key)
//!
//! LOC turned out to be uncorrelated with cohesion: legit-large
//! cohesive files (`account.lzi` 591, `media_price_tables.lzi` 686)
//! tripped the old LOC threshold while a tiny grab-bag (`platform.lzi`
//! 170) sailed under it. The honest surface-area proxy is "how many
//! distinct resources are touched by how many distinct effect kinds"
//! (command / query.list / query.lookup / query.sql / job / webhook) —
//! invariant to comments and formatting. LOC survives only as
//! informative metadata in the diagnostic body, never as the trigger.
//!
//! Cohesion (does the feature do one thing?) is now
//! `LZI-FEATURE-COHESION-002`'s job; this rule is the demoted,
//! re-keyed cold-read-cost nudge.
//!
//! ## Default severity
//!
//! `Warning` (demoted from the old preset-escalated posture). Under
//! `tdd-iron-hand`: `Error` via preset. Legitimately-large-but-cohesive
//! features can still waive it with `# doctor:allow LZI-FILE-SIZE-001`.
//!
//! ## Example fires
//!
//! - A feature with 9 resources each touched by create+list+lookup
//!   (high distinct `(resource × effect)` count) — fires regardless of
//!   how tersely it's written.
//!
//! ## Example silent
//!
//! - A 700-LOC feature that is one resource with a handful of effects:
//!   low `(resource × effect)` count → silent, even though the old LOC
//!   rule would have fired at tier 2.

use std::collections::BTreeSet;

use lazuli_ir::{CommandEffect, Feature, Query, TypeRef};

use crate::allow_comment::source_contains_doctor_allow;
use crate::lzi_hygiene::feature_cohesion_002::LoweredFeature;

/// Surface-area threshold: a feature with strictly more than this many
/// distinct `(resource, effect-kind)` pairs fires. Tuned against the
/// pilot corpus so legit-large-cohesive features
/// (`account` / `payments` / `operations` / `media_price_tables`) stay
/// silent while genuinely sprawling features fire.
pub const RESOURCE_EFFECT_THRESHOLD: usize = 20;

/// One feature above the `(resource × effect)` surface-area threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: std::path::PathBuf,
    /// Feature name.
    pub feature: String,
    /// Count of distinct `(resource, effect-kind)` pairs — the trigger.
    pub resource_effect_pairs: usize,
    /// Total line count — informative metadata only, never the trigger.
    pub loc_count: usize,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "LZI-FILE-SIZE-001";

    /// Render the doctor-formatted diagnostic message. The trigger is the
    /// `(resource × effect)` count; LOC rides along as metadata.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::file_size_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/billing/billing.lzi"),
    ///     feature: "billing".to_string(),
    ///     resource_effect_pairs: 28,
    ///     loc_count: 720,
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("28"));
    /// assert!(msg.contains("720 LOC"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "feature `{}` ({}) has a large declarative surface: {} distinct \
             (resource × effect) pairs (limit {}; {} LOC). Cold-read cost is \
             real for LLM + human authors. If the resources form one cohesive \
             capability, consider extracting a sub-feature into a sibling \
             `.lzi`; if they don't, `LZI-FEATURE-COHESION-002` will say so. \
             Waive with `# doctor:allow LZI-FILE-SIZE-001` only when the size \
             is genuine cohesion. See CLAUDE.md `.lzi` hygiene section.",
            self.feature,
            self.path.display(),
            self.resource_effect_pairs,
            RESOURCE_EFFECT_THRESHOLD,
            self.loc_count,
        )
    }
}

/// Count of distinct `(target, effect-kind)` pairs in a feature.
///
/// Effect kinds and their resource axis (what the IR actually carries):
/// - `command` → the resource named by its `creates`/`updates`/
///   `deletes`/`reorders` effect (pure `returns` commands carry no
///   resource axis and are skipped).
/// - `query.list` / `query.lookup` → the query's own name (list/lookup
///   queries don't carry a resolved resource in IR; each distinct query
///   is a distinct surface effect).
/// - `query.sql` → the resource/record named by its `returns` type.
/// - `job` / `webhook` → the declaration's own name (their target is a
///   handler/route, not a resolved resource in IR).
///
/// Deterministic from the IR (formatting-invariant): the same feature
/// always yields the same count regardless of comments / whitespace.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::lzi_hygiene::file_size_001::resource_effect_pairs;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let n = resource_effect_pairs(&feature);
/// ```
pub fn resource_effect_pairs(feature: &Feature) -> usize {
    let mut pairs: BTreeSet<(String, &'static str)> = BTreeSet::new();

    for cmd in &feature.commands {
        if let Some(resource) = command_effect_resource(&cmd.effect) {
            pairs.insert((resource, "command"));
        }
    }
    for query in &feature.queries {
        match query {
            Query::List(q) => {
                pairs.insert((q.name.clone(), "query.list"));
            }
            Query::Lookup(q) => {
                pairs.insert((q.name.clone(), "query.lookup"));
            }
            Query::Sql(q) => {
                if let Some(resource) = type_ref_resource(&q.returns) {
                    pairs.insert((resource, "query.sql"));
                } else {
                    pairs.insert((q.name.clone(), "query.sql"));
                }
            }
        }
    }
    for job in &feature.jobs {
        pairs.insert((job.name.clone(), "job"));
    }
    for webhook in &feature.webhooks {
        pairs.insert((webhook.name.clone(), "webhook"));
    }

    pairs.len()
}

/// The resource name a command's write effect targets, or `None` for
/// pure `returns` commands (which have no resource axis).
fn command_effect_resource(effect: &CommandEffect) -> Option<String> {
    match effect {
        CommandEffect::Creates(e) => Some(e.resource.name.clone()),
        CommandEffect::Updates(e) => Some(e.resource.name.clone()),
        CommandEffect::Deletes(e) => Some(e.resource.name.clone()),
        CommandEffect::Reorders(e) => Some(e.resource.name.clone()),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// The resource/record name a `query.sql` `returns` type points at, if
/// it is a user-defined type (possibly wrapped in `Many`).
fn type_ref_resource(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::UserDefined(q) => Some(q.name.clone()),
        TypeRef::Many(inner) => type_ref_resource(inner),
        _ => None,
    }
}

/// Run the rule against the pre-lowered features. Returns one finding
/// per feature whose distinct `(resource × effect)` count exceeds the
/// threshold, honoring the `# doctor:allow` opt-out.
pub fn check(features: &[LoweredFeature<'_>]) -> Vec<Finding> {
    let mut out = Vec::new();
    for lf in features {
        if source_contains_doctor_allow(lf.source, Finding::CODE) {
            continue;
        }
        let pairs = resource_effect_pairs(lf.feature);
        if pairs > RESOURCE_EFFECT_THRESHOLD {
            out.push(Finding {
                path: lf.path.clone(),
                feature: lf.feature.name.clone(),
                resource_effect_pairs: pairs,
                loc_count: lf.source.lines().count(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }

    /// Build a feature whose body declares `n` resources, each with a
    /// create command + list + lookup query, yielding `3*n` distinct
    /// `(resource × effect)` pairs. `pad_loc` inflates LOC with comment
    /// lines to prove LOC is NOT the trigger.
    fn many_effects_source(n: usize, pad_loc: usize) -> String {
        // Declarations are grouped (all resources, then commands, then
        // queries): the `.lzi` grammar silently drops a `resource`
        // declared after a `query` at the same indent.
        let mut s = String::from("feature billing\n");
        for i in 0..n {
            s.push_str(&format!("  resource Res{i}\n    label: Text required\n"));
        }
        for i in 0..n {
            s.push_str(&format!("  command create_res{i}\n    creates Res{i}\n"));
        }
        for i in 0..n {
            s.push_str(&format!("  query.list list_res{i}\n"));
        }
        for i in 0..n {
            s.push_str(&format!("  query.lookup get_res{i} by id: ID\n"));
        }
        for _ in 0..pad_loc {
            s.push_str("  # padding comment line\n");
        }
        s
    }

    #[test]
    fn high_resource_effect_count_fires() {
        // 8 resources × 3 effects = 24 distinct pairs > 20 → fires.
        let src = many_effects_source(8, 0);
        let feature = lower(&src);
        let findings = check(&[LoweredFeature::new(
            "features/billing/billing.lzi",
            &feature,
            &src,
        )]);
        assert_eq!(findings.len(), 1, "expected fire, pairs > threshold");
        assert!(findings[0].resource_effect_pairs > RESOURCE_EFFECT_THRESHOLD);
    }

    #[test]
    fn high_loc_but_low_surface_is_silent() {
        // 3 resources × 3 effects = 9 pairs (< 20), padded to 700 LOC.
        let src = many_effects_source(3, 700);
        assert!(src.lines().count() > 600, "fixture must be LOC-large");
        let feature = lower(&src);
        assert!(
            check(&[LoweredFeature::new(
                "features/billing/billing.lzi",
                &feature,
                &src
            )])
            .is_empty(),
            "LOC-large but low (resource × effect) must stay silent"
        );
    }

    #[test]
    fn doctor_allow_suppresses() {
        let mut src = many_effects_source(8, 0);
        src.insert_str(0, "# doctor:allow LZI-FILE-SIZE-001\n");
        let feature = lower(&src);
        assert!(
            check(&[LoweredFeature::new(
                "features/billing/billing.lzi",
                &feature,
                &src
            )])
            .is_empty()
        );
    }

    #[test]
    fn message_reports_pairs_and_loc() {
        let src = many_effects_source(8, 50);
        let feature = lower(&src);
        let lf = LoweredFeature::new("features/billing/billing.lzi", &feature, &src);
        let findings = check(&[lf]);
        let msg = findings[0].message();
        assert!(msg.contains("(resource × effect)"));
        // LOC is the source line count carried into the body.
        assert!(msg.contains(&format!("{} LOC", src.lines().count())));
    }
}
