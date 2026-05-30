//! REF-POLYMORPHIC-TARGET-001 — a `polymorphic_ref <type> <id> targets
//! [A, B, C]` declaration names a target resource that doesn't resolve.
//!
//! GAP-13. A target resolves when it's a resource declared in the same
//! feature, OR a resource in a feature listed in the declaring feature's
//! `uses` (Dependencies). Reuses the GAP-12 cross-feature resolution
//! model (`uses`-as-Dependencies). Fires once per unresolved target.
//!
//! Severity: `error`. An unresolved polymorphic target means the
//! discriminator CHECK admits a value the runtime can never join to —
//! the FK silently dangles for that arm. Rule Zero.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Minimal per-feature view the [`check`] needs. Decoupled from the full
/// `ir::Feature` so the CLI can build it from its Tier-3 fact bundle.
#[derive(Debug, Clone)]
pub struct FeaturePolymorphicView {
    /// Declaring feature name.
    pub feature: String,
    /// Source `.lzi` path hosting the feature.
    pub path: PathBuf,
    /// The feature's `uses` (Dependencies) list.
    pub uses: Vec<String>,
    /// Resource names declared in this feature.
    pub resources: Vec<String>,
    /// Every `polymorphic_ref` site in this feature.
    pub polymorphic_refs: Vec<PolymorphicRefSite>,
}

/// One `polymorphic_ref <type> <id> targets [...]` site.
#[derive(Debug, Clone)]
pub struct PolymorphicRefSite {
    /// Resource declaring the polymorphic ref.
    pub resource: String,
    /// Discriminator field name.
    pub type_field: String,
    /// Declared target resource names.
    pub targets: Vec<String>,
}

/// One REF-POLYMORPHIC-TARGET-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the declaring resource.
    pub path: PathBuf,
    /// Declaring feature.
    pub feature: String,
    /// Declaring resource.
    pub resource: String,
    /// Discriminator field naming the polymorphic ref.
    pub type_field: String,
    /// Target resource name that did not resolve.
    pub unresolved_target: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "REF-POLYMORPHIC-TARGET-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::cross_feature::polymorphic_target_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("ops.lzi"),
    ///     feature: "ops".into(),
    ///     resource: "Note".into(),
    ///     type_field: "entity_type".into(),
    ///     unresolved_target: "Ghost".into(),
    /// };
    /// assert!(f.message().contains("Ghost"));
    /// assert!(f.message().contains("entity_type"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "polymorphic_ref `{}` on resource `{}` lists target `{}`, but no resource named \
             `{}` is declared in feature `{}` or any feature it `uses`. Declare the resource, \
             add the owning feature to Dependencies, or remove the target.",
            self.type_field,
            self.resource,
            self.unresolved_target,
            self.unresolved_target,
            self.feature,
        )
    }
}

/// Run REF-POLYMORPHIC-TARGET-001 across a set of feature views.
///
/// A target resolves when it's a resource in the same feature or in a
/// `uses`-listed feature.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::polymorphic_target_001::check;
///
/// let findings = check(&views);
/// ```
pub fn check(features: &[FeaturePolymorphicView]) -> Vec<Finding> {
    // Index: feature name -> its declared resource names.
    let mut resources_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in features {
        let set = resources_by_feature.entry(f.feature.as_str()).or_default();
        for r in &f.resources {
            set.insert(r.as_str());
        }
    }

    let mut out = Vec::new();
    for f in features {
        // Reachable resources = own feature + every `uses` dependency.
        let mut reachable: BTreeSet<&str> = BTreeSet::new();
        if let Some(set) = resources_by_feature.get(f.feature.as_str()) {
            reachable.extend(set.iter().copied());
        }
        for dep in &f.uses {
            if let Some(set) = resources_by_feature.get(dep.as_str()) {
                reachable.extend(set.iter().copied());
            }
        }
        for site in &f.polymorphic_refs {
            for target in &site.targets {
                if !reachable.contains(target.as_str()) {
                    out.push(Finding {
                        path: f.path.clone(),
                        feature: f.feature.clone(),
                        resource: site.resource.clone(),
                        type_field: site.type_field.clone(),
                        unresolved_target: target.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Convenience: build the per-feature views from `ir::Feature`s. Used by
/// callers that hold lowered IR (LSP / tests).
pub(crate) fn views_from_features(
    features: &[lazuli_ir::Feature],
    path: &Path,
) -> Vec<FeaturePolymorphicView> {
    features
        .iter()
        .map(|feature| {
            let mut sites = Vec::new();
            for resource in &feature.resources {
                for pref in &resource.polymorphic_refs {
                    sites.push(PolymorphicRefSite {
                        resource: resource.name.clone(),
                        type_field: pref.type_field.clone(),
                        targets: pref.targets.clone(),
                    });
                }
            }
            FeaturePolymorphicView {
                feature: feature.name.clone(),
                path: path.to_path_buf(),
                uses: feature.uses.clone(),
                resources: feature.resources.iter().map(|r| r.name.clone()).collect(),
                polymorphic_refs: sites,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(
        feature: &str,
        uses: &[&str],
        resources: &[&str],
        sites: &[(&str, &str, &[&str])],
    ) -> FeaturePolymorphicView {
        FeaturePolymorphicView {
            feature: feature.into(),
            path: PathBuf::from(format!("{feature}.lzi")),
            uses: uses.iter().map(|s| (*s).into()).collect(),
            resources: resources.iter().map(|s| (*s).into()).collect(),
            polymorphic_refs: sites
                .iter()
                .map(|(r, tf, ts)| PolymorphicRefSite {
                    resource: (*r).into(),
                    type_field: (*tf).into(),
                    targets: ts.iter().map(|s| (*s).into()).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn positive_all_targets_resolve_passes() {
        // Note's polymorphic_ref targets Job (same feature) + Customer
        // (in the `crm` dependency).
        let ops = view(
            "ops",
            &["crm"],
            &["Note", "Job"],
            &[("Note", "entity_type", &["Job", "Customer"])],
        );
        let crm = view("crm", &[], &["Customer"], &[]);
        assert!(check(&[ops, crm]).is_empty());
    }

    #[test]
    fn negative_unknown_target_fires() {
        let ops = view(
            "ops",
            &["crm"],
            &["Note", "Job"],
            &[("Note", "entity_type", &["Job", "Ghost"])],
        );
        let crm = view("crm", &[], &["Customer"], &[]);
        let findings = check(&[ops, crm]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_target, "Ghost");
        assert_eq!(Finding::CODE, "REF-POLYMORPHIC-TARGET-001");
    }

    #[test]
    fn negative_target_in_unlisted_feature_fires() {
        // `Customer` exists in `crm` but `ops` does not `uses crm`.
        let ops = view(
            "ops",
            &[],
            &["Note", "Job"],
            &[("Note", "entity_type", &["Job", "Customer"])],
        );
        let crm = view("crm", &[], &["Customer"], &[]);
        let findings = check(&[ops, crm]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_target, "Customer");
    }
}
