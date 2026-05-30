//! AUDIT-MATERIALIZE-TARGET-001 — a command's `audit materialize
//! @feature.<feature>.<Resource>` clause names a target that either does
//! not resolve to a reachable resource, OR resolves to a resource that is
//! not declared `append_only`.
//!
//! GAP-AUDIT-01. A target resolves when it's a resource declared in the
//! same feature, OR a resource in a feature listed in the declaring
//! feature's `uses` (Dependencies). Reuses the GAP-12 cross-feature
//! resolution model (`uses`-as-Dependencies), composed with the W4
//! `append_only` resource modifier: an OperationLog written to by audit
//! materialization MUST be append-only (insert-only ledger discipline) so
//! the trail cannot be mutated after the fact.
//!
//! Severity: `error`. A non-append_only or unresolved materialize target
//! means the runtime would either write to a table that admits
//! update/delete (corruptible audit trail) or to a table that does not
//! exist. Rule Zero.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Minimal per-feature view the [`check`] needs. Decoupled from the full
/// `ir::Feature` so the CLI can build it from its Tier-3 fact bundle.
#[derive(Debug, Clone)]
pub struct FeatureAuditMaterializeView {
    /// Declaring feature name.
    pub feature: String,
    /// Source `.lzi` path hosting the feature.
    pub path: PathBuf,
    /// The feature's `uses` (Dependencies) list.
    pub uses: Vec<String>,
    /// Resource names declared in this feature, paired with their
    /// `append_only` flag. Used both to resolve same-feature targets and
    /// (across the package index) to enforce the append_only invariant.
    pub resources: Vec<ResourceAppendOnly>,
    /// Every `audit materialize` site in this feature.
    pub materialize_sites: Vec<AuditMaterializeSite>,
}

/// One resource name + its `append_only` flag.
#[derive(Debug, Clone)]
pub struct ResourceAppendOnly {
    pub name: String,
    pub append_only: bool,
}

/// One `command <name>` ... `audit materialize @feature.<f>.<R>` site.
#[derive(Debug, Clone)]
pub struct AuditMaterializeSite {
    /// Command declaring the `audit materialize` clause.
    pub command: String,
    /// Target feature segment (after `@feature.`).
    pub target_feature: String,
    /// Target resource name.
    pub target_resource: String,
}

/// The reason an AUDIT-MATERIALIZE-TARGET-001 finding fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// The named resource does not resolve to a reachable resource (same
    /// feature or a `uses`-listed dependency).
    Unresolved,
    /// The named resource resolves, but is not declared `append_only`.
    NotAppendOnly,
}

/// One AUDIT-MATERIALIZE-TARGET-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the declaring command.
    pub path: PathBuf,
    /// Declaring feature.
    pub feature: String,
    /// Declaring command.
    pub command: String,
    /// Target feature segment.
    pub target_feature: String,
    /// Target resource name.
    pub target_resource: String,
    /// Why the rule fired.
    pub kind: FindingKind,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "AUDIT-MATERIALIZE-TARGET-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::cross_feature::audit_materialize_target_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("ops.lzi"),
    ///     feature: "ops".into(),
    ///     command: "create_job".into(),
    ///     target_feature: "audit".into(),
    ///     target_resource: "OperationLog".into(),
    ///     kind: FindingKind::NotAppendOnly,
    /// };
    /// assert!(f.message().contains("OperationLog"));
    /// assert!(f.message().contains("append_only"));
    /// ```
    pub fn message(&self) -> String {
        let target = format!("@feature.{}.{}", self.target_feature, self.target_resource);
        match self.kind {
            FindingKind::Unresolved => format!(
                "command `{}` declares `audit materialize {target}`, but no resource named `{}` \
                 is declared in feature `{}` or any feature it `uses`. Declare the resource, add \
                 the owning feature to Dependencies, or remove the `materialize` clause.",
                self.command, self.target_resource, self.feature,
            ),
            FindingKind::NotAppendOnly => format!(
                "command `{}` declares `audit materialize {target}`, but resource `{}` is not \
                 `append_only`. An audit OperationLog must be append-only so the materialized \
                 trail cannot be mutated — mark the resource `append_only`.",
                self.command, self.target_resource,
            ),
        }
    }
}

/// Run AUDIT-MATERIALIZE-TARGET-001 across a set of feature views.
///
/// A target resolves when it's a resource in the same feature or in a
/// `uses`-listed feature; once resolved, the resource MUST be
/// `append_only`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::audit_materialize_target_001::check;
///
/// let findings = check(&views);
/// ```
pub fn check(features: &[FeatureAuditMaterializeView]) -> Vec<Finding> {
    // Index: feature name -> { resource name -> append_only }.
    let mut resources_by_feature: BTreeMap<&str, BTreeMap<&str, bool>> = BTreeMap::new();
    for f in features {
        let set = resources_by_feature.entry(f.feature.as_str()).or_default();
        for r in &f.resources {
            set.insert(r.name.as_str(), r.append_only);
        }
    }

    let mut out = Vec::new();
    for f in features {
        // Reachable feature names = own feature + every `uses` dependency.
        let mut reachable: BTreeSet<&str> = BTreeSet::new();
        reachable.insert(f.feature.as_str());
        for dep in &f.uses {
            reachable.insert(dep.as_str());
        }
        for site in &f.materialize_sites {
            // The target carries its own owning-feature segment; it only
            // resolves when that feature is reachable AND declares the
            // resource. Cross-feature segments not in `uses` are
            // unresolved (matches GAP-12 `uses`-as-Dependencies).
            let resolved = reachable.contains(site.target_feature.as_str()).then(|| {
                resources_by_feature
                    .get(site.target_feature.as_str())
                    .and_then(|set| set.get(site.target_resource.as_str()).copied())
            });
            match resolved {
                Some(Some(true)) => { /* reachable + append_only — OK. */ }
                Some(Some(false)) => out.push(Finding {
                    path: f.path.clone(),
                    feature: f.feature.clone(),
                    command: site.command.clone(),
                    target_feature: site.target_feature.clone(),
                    target_resource: site.target_resource.clone(),
                    kind: FindingKind::NotAppendOnly,
                }),
                // Resource missing in the (reachable) target feature, or
                // the target feature itself is not reachable.
                Some(None) | None => out.push(Finding {
                    path: f.path.clone(),
                    feature: f.feature.clone(),
                    command: site.command.clone(),
                    target_feature: site.target_feature.clone(),
                    target_resource: site.target_resource.clone(),
                    kind: FindingKind::Unresolved,
                }),
            }
        }
    }
    out
}

/// File-local LSP entry point — checks only the `audit materialize`
/// sites whose target feature is *this* feature (same-feature
/// resolution). Cross-feature targets are deferred to the package-wide
/// [`check`] in the CLI doctor, which can see sibling features' resources
/// + their `append_only` flags; a per-file LSP pass cannot. This mirrors
/// how the package-wide cross-feature rules (REF-CROSS-FEATURE-UNKNOWN-001,
/// REF-POLYMORPHIC-TARGET-001) only run in the CLI while same-feature
/// invariants surface live in the editor.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::audit_materialize_target_001::check_local;
/// let findings = check_local(&feature, path);
/// ```
pub fn check_local(feature: &lazuli_ir::Feature, path: &Path) -> Vec<Finding> {
    let view = views_from_features(std::slice::from_ref(feature), path)
        .into_iter()
        .next();
    let Some(mut view) = view else {
        return Vec::new();
    };
    // Retain only same-feature targets; cross-feature ones can't be
    // resolved from a single feature view (the sibling's resources +
    // append_only flag live in another file).
    view.materialize_sites
        .retain(|site| site.target_feature == view.feature);
    check(&[view])
}

/// Convenience: build the per-feature views from `ir::Feature`s. Used by
/// callers that hold lowered IR (LSP / tests).
pub fn views_from_features(
    features: &[lazuli_ir::Feature],
    path: &Path,
) -> Vec<FeatureAuditMaterializeView> {
    features
        .iter()
        .map(|feature| {
            let mut sites = Vec::new();
            for command in &feature.commands {
                if let Some(audit) = &command.audit
                    && let Some(m) = &audit.materialize
                {
                    sites.push(AuditMaterializeSite {
                        command: command.name.clone(),
                        target_feature: m.feature.clone(),
                        target_resource: m.resource.clone(),
                    });
                }
            }
            FeatureAuditMaterializeView {
                feature: feature.name.clone(),
                path: path.to_path_buf(),
                uses: feature.uses.clone(),
                resources: feature
                    .resources
                    .iter()
                    .map(|r| ResourceAppendOnly {
                        name: r.name.clone(),
                        append_only: r.append_only,
                    })
                    .collect(),
                materialize_sites: sites,
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
        resources: &[(&str, bool)],
        sites: &[(&str, &str, &str)],
    ) -> FeatureAuditMaterializeView {
        FeatureAuditMaterializeView {
            feature: feature.into(),
            path: PathBuf::from(format!("{feature}.lzi")),
            uses: uses.iter().map(|s| (*s).into()).collect(),
            resources: resources
                .iter()
                .map(|(n, ao)| ResourceAppendOnly {
                    name: (*n).into(),
                    append_only: *ao,
                })
                .collect(),
            materialize_sites: sites
                .iter()
                .map(|(c, tf, tr)| AuditMaterializeSite {
                    command: (*c).into(),
                    target_feature: (*tf).into(),
                    target_resource: (*tr).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn positive_append_only_target_resolves_passes() {
        // `jobs.create_job` materializes into `audit.OperationLog`, an
        // append_only resource in the `audit` dependency.
        let jobs = view(
            "jobs",
            &["audit"],
            &[("Job", false)],
            &[("create_job", "audit", "OperationLog")],
        );
        let audit = view("audit", &[], &[("OperationLog", true)], &[]);
        assert!(check(&[jobs, audit]).is_empty());
    }

    #[test]
    fn positive_same_feature_append_only_passes() {
        let audit = view(
            "audit",
            &[],
            &[("OperationLog", true)],
            &[("record", "audit", "OperationLog")],
        );
        assert!(check(&[audit]).is_empty());
    }

    #[test]
    fn negative_target_resource_missing_fires_unresolved() {
        let jobs = view(
            "jobs",
            &["audit"],
            &[("Job", false)],
            &[("create_job", "audit", "Ghost")],
        );
        let audit = view("audit", &[], &[("OperationLog", true)], &[]);
        let findings = check(&[jobs, audit]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::Unresolved);
        assert_eq!(findings[0].target_resource, "Ghost");
        assert_eq!(Finding::CODE, "AUDIT-MATERIALIZE-TARGET-001");
    }

    #[test]
    fn negative_target_feature_unlisted_fires_unresolved() {
        // `audit` is not in `jobs.uses` — even though OperationLog exists.
        let jobs = view(
            "jobs",
            &[],
            &[("Job", false)],
            &[("create_job", "audit", "OperationLog")],
        );
        let audit = view("audit", &[], &[("OperationLog", true)], &[]);
        let findings = check(&[jobs, audit]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::Unresolved);
    }

    #[test]
    fn negative_target_not_append_only_fires() {
        // OperationLog resolves but is NOT append_only.
        let jobs = view(
            "jobs",
            &["audit"],
            &[("Job", false)],
            &[("create_job", "audit", "OperationLog")],
        );
        let audit = view("audit", &[], &[("OperationLog", false)], &[]);
        let findings = check(&[jobs, audit]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::NotAppendOnly);
        assert!(findings[0].message().contains("append_only"));
    }
}
