//! REF-CROSS-FEATURE-UNKNOWN-001 — a field-level `target
//! @feature.<feature>.<Resource>` cross-feature FK references a feature
//! not declared in the consumer's Dependencies (`uses`), or a resource
//! that doesn't exist in the named feature.
//!
//! GAP-12. Fires when:
//!  - the named `<feature>` is not in the declaring feature's `uses`
//!    (Dependencies) list, OR
//!  - the named `<Resource>` is not declared in that feature.
//!
//! Severity: `error`. A dangling cross-feature FK silently indexes a
//! column that points nowhere; under `microservices` it also breaks the
//! contract boundary. Reuses the `uses`-as-Dependencies resolution model
//! described in `lazuli-ops/docs/proposals/cross-feature-contracts.md`
//! §5.2 (the consumer declares intent via `uses`).
//!
//! This is a *module-level* check (it must look up the target feature's
//! resources), so the public `check` consumes a flat
//! [`FeatureCrossRefView`] slice the caller assembles from whatever IR /
//! fact bundle it has on hand.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Minimal per-feature view the [`check`] needs. Decoupled from the full
/// `ir::Feature` so the CLI can build it from its Tier-3 fact bundle and
/// tests can construct it inline.
#[derive(Debug, Clone)]
pub struct FeatureCrossRefView {
    /// Declaring feature name.
    pub feature: String,
    /// Source `.lzi` path hosting the feature.
    pub path: PathBuf,
    /// The feature's `uses` (Dependencies) list.
    pub uses: Vec<String>,
    /// Resource names declared in this feature.
    pub resources: Vec<String>,
    /// Every `target @feature.<feature>.<Resource>` annotation in this
    /// feature, as `(resource, field, target_feature, target_resource)`.
    pub cross_feature_targets: Vec<CrossFeatureTargetRef>,
}

/// Where a `target @feature` annotation was declared — a persisted
/// `resource` field or a typed `record` field. GAP-R5 added record-field
/// support; the resolver is identical, but the rendered message names the
/// right container kind ("resource" vs "record").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Declared on a `resource <Name>` field.
    Resource,
    /// Declared on a `record <Name>` field (GAP-R5). The cross-feature ID
    /// is logical-only — nested in JSONB, so no migration index is emitted.
    Record,
}

impl Origin {
    /// Lower-case container noun for diagnostic messages.
    fn noun(self) -> &'static str {
        match self {
            Origin::Resource => "resource",
            Origin::Record => "record",
        }
    }
}

/// One `target @feature.<feature>.<Resource>` annotation site.
#[derive(Debug, Clone)]
pub struct CrossFeatureTargetRef {
    /// Resource or record declaring the FK field.
    pub resource: String,
    /// FK field name.
    pub field: String,
    /// Named target feature (segment after `@feature.`).
    pub target_feature: String,
    /// Named target resource.
    pub target_resource: String,
    /// Whether the declaring container is a `resource` or a `record`.
    pub origin: Origin,
}

/// What about the reference failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// `target_feature` not in the declaring feature's `uses`.
    FeatureNotInDependencies,
    /// `target_feature` is a dependency but declares no such resource.
    ResourceNotInFeature,
}

/// One REF-CROSS-FEATURE-UNKNOWN-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the declaring resource.
    pub path: PathBuf,
    /// Declaring feature.
    pub feature: String,
    /// Declaring resource.
    pub resource: String,
    /// FK field name.
    pub field: String,
    /// Named target feature.
    pub target_feature: String,
    /// Named target resource.
    pub target_resource: String,
    /// Whether the declaring container is a `resource` or a `record`.
    pub origin: Origin,
    /// Why it failed.
    pub reason: Reason,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "REF-CROSS-FEATURE-UNKNOWN-001";

    /// Render the user-facing diagnostic body. Branches on `reason` so
    /// the author sees the right fix (add to `uses` vs. fix the name).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::cross_feature::ref_unknown_001::{Finding, Origin, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("agency.lzi"),
    ///     feature: "agency".into(),
    ///     resource: "Agency".into(),
    ///     field: "default_department_id".into(),
    ///     target_feature: "department".into(),
    ///     target_resource: "Department".into(),
    ///     origin: Origin::Resource,
    ///     reason: Reason::FeatureNotInDependencies,
    /// };
    /// assert!(f.message().contains("department"));
    /// assert!(f.message().contains("uses"));
    /// ```
    pub fn message(&self) -> String {
        let container = self.origin.noun();
        match self.reason {
            Reason::FeatureNotInDependencies => format!(
                "field `{}` of {} `{}` targets `@feature.{}.{}`, but feature `{}` \
                 does not declare `uses {}` — add it to the feature's Dependencies.",
                self.field,
                container,
                self.resource,
                self.target_feature,
                self.target_resource,
                self.feature,
                self.target_feature,
            ),
            Reason::ResourceNotInFeature => format!(
                "field `{}` of {} `{}` targets `@feature.{}.{}`, but feature `{}` \
                 declares no resource named `{}`.",
                self.field,
                container,
                self.resource,
                self.target_feature,
                self.target_resource,
                self.target_feature,
                self.target_resource,
            ),
        }
    }
}

/// Run REF-CROSS-FEATURE-UNKNOWN-001 across a set of feature views.
///
/// For every `target @feature.<feature>.<Resource>` annotation: the named
/// feature must be in the declaring feature's `uses`, and the named
/// resource must exist in that feature.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::ref_unknown_001::check;
///
/// let findings = check(&feature_views);
/// ```
pub fn check(features: &[FeatureCrossRefView]) -> Vec<Finding> {
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
        let deps: BTreeSet<&str> = f.uses.iter().map(String::as_str).collect();
        for target in &f.cross_feature_targets {
            // 1. Target feature must be a declared dependency.
            if !deps.contains(target.target_feature.as_str()) {
                out.push(make_finding(f, target, Reason::FeatureNotInDependencies));
                continue;
            }
            // 2. Target resource must exist in that feature.
            let resource_known = resources_by_feature
                .get(target.target_feature.as_str())
                .map(|set| set.contains(target.target_resource.as_str()))
                .unwrap_or(false);
            if !resource_known {
                out.push(make_finding(f, target, Reason::ResourceNotInFeature));
            }
        }
    }
    out
}

fn make_finding(
    f: &FeatureCrossRefView,
    target: &CrossFeatureTargetRef,
    reason: Reason,
) -> Finding {
    Finding {
        path: f.path.clone(),
        feature: f.feature.clone(),
        resource: target.resource.clone(),
        field: target.field.clone(),
        target_feature: target.target_feature.clone(),
        target_resource: target.target_resource.clone(),
        origin: target.origin,
        reason,
    }
}

/// Convenience: build the per-feature views directly from `ir::Feature`s.
/// Used by callers that already hold lowered IR (LSP / tests). The CLI
/// builds views from its fact bundle instead.
pub(crate) fn views_from_features(
    features: &[lazuli_ir::Feature],
    path: &Path,
) -> Vec<FeatureCrossRefView> {
    features
        .iter()
        .map(|feature| {
            let mut targets = Vec::new();
            for resource in &feature.resources {
                for field in &resource.fields {
                    if let Some(t) = &field.cross_feature_target {
                        targets.push(CrossFeatureTargetRef {
                            resource: resource.name.clone(),
                            field: field.name.clone(),
                            target_feature: t.feature.clone(),
                            target_resource: t.resource.clone(),
                            origin: Origin::Resource,
                        });
                    }
                }
            }
            // GAP-R5 — a `target @feature.<f>.<R>` annotation may also sit
            // on a `record <Name>` field. The cross-feature ID is nested in
            // JSONB (logical-only, no migration index), but the reference
            // must still resolve: same `uses` + resource-exists check.
            for record in &feature.records {
                for field in &record.fields {
                    if let Some(t) = &field.cross_feature_target {
                        targets.push(CrossFeatureTargetRef {
                            resource: record.name.clone(),
                            field: field.name.clone(),
                            target_feature: t.feature.clone(),
                            target_resource: t.resource.clone(),
                            origin: Origin::Record,
                        });
                    }
                }
            }
            FeatureCrossRefView {
                feature: feature.name.clone(),
                path: path.to_path_buf(),
                uses: feature.uses.clone(),
                resources: feature.resources.iter().map(|r| r.name.clone()).collect(),
                cross_feature_targets: targets,
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
        targets: &[(&str, &str, &str, &str)],
    ) -> FeatureCrossRefView {
        view_with_origin(feature, uses, resources, targets, Origin::Resource)
    }

    fn view_with_origin(
        feature: &str,
        uses: &[&str],
        resources: &[&str],
        targets: &[(&str, &str, &str, &str)],
        origin: Origin,
    ) -> FeatureCrossRefView {
        FeatureCrossRefView {
            feature: feature.into(),
            path: PathBuf::from(format!("{feature}.lzi")),
            uses: uses.iter().map(|s| (*s).into()).collect(),
            resources: resources.iter().map(|s| (*s).into()).collect(),
            cross_feature_targets: targets
                .iter()
                .map(|(r, f, tf, tr)| CrossFeatureTargetRef {
                    resource: (*r).into(),
                    field: (*f).into(),
                    target_feature: (*tf).into(),
                    target_resource: (*tr).into(),
                    origin,
                })
                .collect(),
        }
    }

    #[test]
    fn positive_feature_in_deps_resource_exists_passes() {
        let agency = view(
            "agency",
            &["department"],
            &["Agency"],
            &[(
                "Agency",
                "default_department_id",
                "department",
                "Department",
            )],
        );
        let department = view("department", &[], &["Department"], &[]);
        assert!(check(&[agency, department]).is_empty());
    }

    #[test]
    fn negative_feature_not_in_dependencies_fires() {
        // `agency` references `@feature.department.Department` but never
        // declares `uses department`.
        let agency = view(
            "agency",
            &[],
            &["Agency"],
            &[(
                "Agency",
                "default_department_id",
                "department",
                "Department",
            )],
        );
        let department = view("department", &[], &["Department"], &[]);
        let findings = check(&[agency, department]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FeatureNotInDependencies);
        assert_eq!(findings[0].target_feature, "department");
        assert_eq!(Finding::CODE, "REF-CROSS-FEATURE-UNKNOWN-001");
        assert!(findings[0].message().contains("uses department"));
    }

    #[test]
    fn negative_unknown_resource_in_dependency_fires() {
        let agency = view(
            "agency",
            &["department"],
            &["Agency"],
            &[("Agency", "ghost_id", "department", "Ghost")],
        );
        let department = view("department", &[], &["Department"], &[]);
        let findings = check(&[agency, department]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::ResourceNotInFeature);
        assert_eq!(findings[0].target_resource, "Ghost");
    }

    // ---- GAP-R5: `target @feature.<f>.<R>` on a RECORD field ----

    #[test]
    fn record_positive_feature_in_deps_resource_exists_passes() {
        // `agency` has a record (snapshot bag) whose field targets
        // `@feature.department.Department`, declares `uses department`,
        // and the resource exists → clean.
        let agency = view_with_origin(
            "agency",
            &["department"],
            &["Agency"],
            &[(
                "AgencySnapshot",
                "department_id",
                "department",
                "Department",
            )],
            Origin::Record,
        );
        let department = view("department", &[], &["Department"], &[]);
        assert!(check(&[agency, department]).is_empty());
    }

    #[test]
    fn record_negative_feature_not_in_dependencies_fires() {
        // Record field references `@feature.department.Department` but the
        // declaring feature never declares `uses department`.
        let agency = view_with_origin(
            "agency",
            &[],
            &["Agency"],
            &[(
                "AgencySnapshot",
                "department_id",
                "department",
                "Department",
            )],
            Origin::Record,
        );
        let department = view("department", &[], &["Department"], &[]);
        let findings = check(&[agency, department]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FeatureNotInDependencies);
        assert_eq!(findings[0].origin, Origin::Record);
        assert_eq!(findings[0].resource, "AgencySnapshot");
        assert!(findings[0].message().contains("record `AgencySnapshot`"));
        assert!(findings[0].message().contains("uses department"));
    }

    #[test]
    fn record_negative_unknown_resource_in_dependency_fires() {
        // Feature is a declared dependency, but the named resource does
        // not exist in it.
        let agency = view_with_origin(
            "agency",
            &["department"],
            &["Agency"],
            &[("AgencySnapshot", "ghost_id", "department", "Ghost")],
            Origin::Record,
        );
        let department = view("department", &[], &["Department"], &[]);
        let findings = check(&[agency, department]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::ResourceNotInFeature);
        assert_eq!(findings[0].origin, Origin::Record);
        assert_eq!(findings[0].target_resource, "Ghost");
    }
}
