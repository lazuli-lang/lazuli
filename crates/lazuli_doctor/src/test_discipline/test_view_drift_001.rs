//! TEST-VIEW-DRIFT-001 — view test assertion fails cross-feature resolution.
//!
//! Example: fires when a `.lzx` view authors `allows extension tags` but the
//! `tags` experience does not declare `extends @anchor.<host_anchor>`.
//!
//! Walks every `allows extension <feature>` / `denies extension <feature>` assertion
//! on `.lzx` views and verifies, for `allows extension`, that the named feature
//! actually declares `extends @anchor.<X>` matching the host view's anchor.
//!
//! Two finding shapes share the rule:
//!
//! - `MissingFeature` — `allows extension <feature>` names an experience that
//!   does not exist in the module.
//! - `MissingAnchorExtension` — the experience exists but does not extend
//!   the host view's anchor.
//!
//! `denies extension` assertions deliberately do NOT fire either finding here:
//! they are existence-tolerant (the feature may not even exist yet; the
//! point is to pre-commit a forbidden surface). A future
//! `TEST-VIEW-REJECTED-DRIFT-001` could flag the inverse — `denies extension
//! <feature>` while `<feature>` actually carries an `extends` clause.
//! Out of scope for Wave 4.
//!
//! Severity: `error` (strict / production both). Reference:
//! TDD/BDD-first proposal §Wave 4.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, SpanRef, ViewTestAssertion};

// ── output ────────────────────────────────────────────────────────────────────

/// One TEST-VIEW-DRIFT-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzx` source path that hosts the view.
    pub path: PathBuf,
    /// Host experience containing the view.
    pub experience: String,
    /// Host view name.
    pub view: String,
    /// Host view's anchor token.
    pub anchor: String,
    /// Feature named by the `allows extension <feature>` assertion.
    pub target_feature: String,
    /// Specific drift detected — feature unknown vs feature known but
    /// missing the required `extends <anchor>` declaration.
    pub kind: FindingKind,
    /// Optional span pointer for editor jumps.
    pub span_ref: Option<SpanRef>,
}

/// Distinguishes the two `TEST-VIEW-DRIFT-001` shapes. See variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// `allows extension <feature>` names a feature that does not appear in
    /// the module's experiences.
    MissingFeature,
    /// `allows extension <feature>` resolves to an experience, but that
    /// experience does not declare `extends <anchor>` for the host view's
    /// anchor.
    MissingAnchorExtension,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-VIEW-DRIFT-001";

    /// Render the user-facing diagnostic body. Wording branches on
    /// [`FindingKind`] so authors see whether the feature is unknown
    /// or merely missing the matching `extends <anchor>` declaration.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_view_drift_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("shop.lzx"),
    ///     experience: "customer".into(),
    ///     view: "dashboard".into(),
    ///     anchor: "account-card".into(),
    ///     target_feature: "billing".into(),
    ///     kind: FindingKind::MissingFeature,
    ///     span_ref: None,
    /// };
    /// assert!(f.message().contains("billing"));
    /// ```
    pub fn message(&self) -> String {
        match self.kind {
            FindingKind::MissingFeature => format!(
                "view `{}.{}` asserts `allows extension {}` but no `experience {}` declaration \
                 exists in the module — declare the experience, drop the assertion, or fix \
                 the typo.",
                self.experience, self.view, self.target_feature, self.target_feature,
            ),
            FindingKind::MissingAnchorExtension => format!(
                "view `{}.{}` (anchor `{}`) asserts `allows extension {}` but experience \
                 `{}` does not declare `extends {}` — either add the extension or remove \
                 the assertion to keep view tests honest.",
                self.experience,
                self.view,
                self.anchor,
                self.target_feature,
                self.target_feature,
                self.anchor,
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-VIEW-DRIFT-001 across a single `ExperienceModule`.
///
/// Cross-feature resolution stays inside the module: extensions named on
/// view tests must resolve to a sibling `Experience` in the same `.lzx`
/// module. (Cross-`.lzx` extension drift, if it ever lands, gets a
/// dedicated rule — out of scope here.)
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_drift_001::check;
///
/// let findings = check(&module, Path::new("shop.lzx"));
/// for f in findings {
///     eprintln!("{}.{}: drift on allows extension {}", f.experience, f.view, f.target_feature);
/// }
/// ```
pub fn check(module: &ExperienceModule, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for experience in &module.experiences {
        for view in &experience.views {
            let Some(view_anchor) = view.anchor.as_ref() else {
                // No anchor on the host view — nothing to cross-check
                // against. `allows extension` on a non-anchored view is
                // already a parse-time arguable shape; doctor stays
                // silent here.
                continue;
            };
            for assertion in &view.tests {
                let ViewTestAssertion::AllowsExtension { feature, span_ref } = assertion else {
                    // `DeniesExtension` is existence-tolerant by design.
                    continue;
                };

                let Some(target) = module
                    .experiences
                    .iter()
                    .find(|other| &other.name == feature)
                else {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        experience: experience.name.clone(),
                        view: view.name.clone(),
                        anchor: view_anchor.clone(),
                        target_feature: feature.clone(),
                        kind: FindingKind::MissingFeature,
                        span_ref: *span_ref,
                    });
                    continue;
                };

                let matches_anchor = target
                    .extensions
                    .iter()
                    .any(|ext| &ext.anchor == view_anchor);

                if !matches_anchor {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        experience: experience.name.clone(),
                        view: view.name.clone(),
                        anchor: view_anchor.clone(),
                        target_feature: feature.clone(),
                        kind: FindingKind::MissingAnchorExtension,
                        span_ref: *span_ref,
                    });
                }
            }
        }
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Experience, ExperienceModule, ExperienceView, ViewExtension, ViewTestAssertion,
    };

    fn mk_view(name: &str, anchor: Option<&str>, tests: Vec<ViewTestAssertion>) -> ExperienceView {
        ExperienceView {
            name: name.into(),
            anchor: anchor.map(Into::into),
            routes: vec![],
            extensible_by: vec![],
            source: None,
            submit: None,
            blocks: vec![],
            actions: vec![],
            opens: vec![],
            tests,
            guard: None,
            resolved_guard_policy: None,
            resolved_lifecycle_gate: None,
            span_ref: None,
        }
    }

    fn mk_experience(
        name: &str,
        views: Vec<ExperienceView>,
        extension_anchors: Vec<&str>,
    ) -> Experience {
        Experience {
            name: name.into(),
            imports: vec![],
            views,
            resume_routers: vec![],
            extensions: extension_anchors
                .into_iter()
                .map(|a| ViewExtension {
                    anchor: a.into(),
                    blocks: vec![],
                    slots: vec![],
                    span_ref: None,
                })
                .collect(),
            span_ref: None,
        }
    }

    fn mk_module(experiences: Vec<Experience>) -> ExperienceModule {
        ExperienceModule {
            app: None,
            routes: vec![],
            experiences,
            surfaces: vec![],
        }
    }

    #[test]
    fn quiet_when_target_feature_extends_anchor() {
        let host_view = mk_view(
            "detail",
            Some("@anchor.customer_detail"),
            vec![ViewTestAssertion::AllowsExtension {
                feature: "tags".into(),
                span_ref: None,
            }],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let target = mk_experience("tags", vec![], vec!["@anchor.customer_detail"]);

        let module = mk_module(vec![host, target]);
        assert!(check(&module, Path::new("c.lzx")).is_empty());
    }

    #[test]
    fn fires_when_target_feature_missing() {
        let host_view = mk_view(
            "detail",
            Some("@anchor.customer_detail"),
            vec![ViewTestAssertion::AllowsExtension {
                feature: "tags".into(),
                span_ref: None,
            }],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let module = mk_module(vec![host]);
        let findings = check(&module, Path::new("c.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::MissingFeature);
        assert!(findings[0].message().contains("no `experience tags`"));
    }

    #[test]
    fn fires_when_target_feature_extends_a_different_anchor() {
        let host_view = mk_view(
            "detail",
            Some("@anchor.customer_detail"),
            vec![ViewTestAssertion::AllowsExtension {
                feature: "tags".into(),
                span_ref: None,
            }],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let target = mk_experience("tags", vec![], vec!["@anchor.OTHER"]);

        let module = mk_module(vec![host, target]);
        let findings = check(&module, Path::new("c.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::MissingAnchorExtension);
        assert!(findings[0].message().contains("does not declare"));
    }

    #[test]
    fn rejected_by_does_not_fire_either_finding() {
        // `denies extension` is existence-tolerant; it is meant to pre-commit
        // a forbidden surface even before the would-be extender exists.
        let host_view = mk_view(
            "detail",
            Some("@anchor.customer_detail"),
            vec![ViewTestAssertion::DeniesExtension {
                feature: "billing".into(),
                span_ref: None,
            }],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let module = mk_module(vec![host]);
        assert!(check(&module, Path::new("c.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_host_view_has_no_anchor() {
        let host_view = mk_view(
            "detail",
            None,
            vec![ViewTestAssertion::AllowsExtension {
                feature: "tags".into(),
                span_ref: None,
            }],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let module = mk_module(vec![host]);
        // Without an anchor on the host view, no cross-check is possible.
        // The rule stays silent and a future surface (e.g. a parser-time
        // error) handles the malformed shape.
        assert!(check(&module, Path::new("c.lzx")).is_empty());
    }

    #[test]
    fn multiple_assertions_each_evaluated_independently() {
        let host_view = mk_view(
            "detail",
            Some("@anchor.customer_detail"),
            vec![
                ViewTestAssertion::AllowsExtension {
                    feature: "tags".into(),
                    span_ref: None,
                },
                ViewTestAssertion::AllowsExtension {
                    feature: "imports".into(),
                    span_ref: None,
                },
            ],
        );
        let host = mk_experience("customer", vec![host_view], vec![]);
        let tags = mk_experience("tags", vec![], vec!["@anchor.customer_detail"]);
        // `imports` is missing entirely.

        let module = mk_module(vec![host, tags]);
        let findings = check(&module, Path::new("c.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].target_feature, "imports");
        assert_eq!(findings[0].kind, FindingKind::MissingFeature);
    }
}
