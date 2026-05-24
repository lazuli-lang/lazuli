//! TEST-VIEW-EXTENSIBILITY-001 — extensible view without test assertions.
//!
//! Example: fires when a `.lzx` view declares `extensible_by customer_tags`
//! but has no `accepted by` / `rejected by` assertions under its
//! `tests` block.
//!
//! Fires when an `.lzx` `view` declares one or more `extensible_by` targets
//! but authors no `accepted by` / `rejected by` assertions. The intent of
//! the rule is to keep extensibility surface visible: a view that opens
//! its anchor to other features must declare which features are admitted
//! (and, optionally, which are denied) so downstream extensions can be
//! cross-checked by `TEST-VIEW-DRIFT-001`.
//!
//! Wave 4 (TDD/BDD-first proposal §Wave 4). Sibling rule:
//! `TEST-VIEW-DRIFT-001`. Severity: `warning` (strict / production both).

use std::path::{Path, PathBuf};

use lazuli_ir::{Experience, ExperienceModule, ExperienceView, SpanRef};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzx` file.
    pub path: PathBuf,
    /// Owning experience (`experience <name>` block in the `.lzx`).
    pub experience: String,
    /// View name (`view <name>` child).
    pub view: String,
    /// Verbatim `extensible_by` list — surfaces in the message so the
    /// author sees which features they pledged to constrain.
    pub extensible_by: Vec<String>,
    /// Source location of the view header, when known.
    pub span_ref: Option<SpanRef>,
}

impl Finding {
    pub const CODE: &'static str = "TEST-VIEW-EXTENSIBILITY-001";

    pub fn message(&self) -> String {
        format!(
            "view `{}.{}` declares `extensible_by {}` but has no `accepted by` / `rejected by` \
             assertions — add at least one assertion under a `tests` block so doctor can \
             cross-check that each declared extension actually exists and targets the right \
             anchor (see TEST-VIEW-DRIFT-001).",
            self.experience,
            self.view,
            self.extensible_by.join(", "),
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-VIEW-EXTENSIBILITY-001 against one `ExperienceModule`.
///
/// `path` is the source `.lzx` file. The rule does no I/O; the caller
/// turns each `Finding` into a `DoctorDiagnostic` and anchors it via the
/// returned `span_ref`.
pub fn check(module: &ExperienceModule, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for experience in &module.experiences {
        for view in &experience.views {
            if view_needs_assertion(view) && view.tests.is_empty() {
                out.push(Finding {
                    path: path.to_path_buf(),
                    experience: experience.name.clone(),
                    view: view.name.clone(),
                    extensible_by: view.extensible_by.clone(),
                    span_ref: view.span_ref,
                });
            }
        }
    }
    out
}

/// Helper for callers that already iterate experiences elsewhere (LSP
/// single-view squiggle path). Returns one finding when the view triggers
/// the rule, `None` otherwise.
pub fn check_view(
    experience: &Experience,
    view: &ExperienceView,
    path: &Path,
) -> Option<Finding> {
    if !view_needs_assertion(view) || !view.tests.is_empty() {
        return None;
    }
    Some(Finding {
        path: path.to_path_buf(),
        experience: experience.name.clone(),
        view: view.name.clone(),
        extensible_by: view.extensible_by.clone(),
        span_ref: view.span_ref,
    })
}

// ── internals ─────────────────────────────────────────────────────────────────

fn view_needs_assertion(view: &ExperienceView) -> bool {
    !view.extensible_by.is_empty()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{Experience, ExperienceModule, ExperienceView, ViewTestAssertion};

    fn mk_view(name: &str, extensible_by: Vec<String>, tests: Vec<ViewTestAssertion>) -> ExperienceView {
        ExperienceView {
            name: name.into(),
            anchor: Some(format!("{name}_anchor")),
            routes: vec![],
            extensible_by,
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

    fn mk_module(view: ExperienceView) -> ExperienceModule {
        ExperienceModule {
            app: None,
            routes: vec![],
            experiences: vec![Experience {
                name: "customer".into(),
                imports: vec![],
                views: vec![view],
                resume_routers: vec![],
                extensions: vec![],
                span_ref: None,
            }],
            surfaces: vec![],
        }
    }

    #[test]
    fn fires_when_extensible_view_has_no_assertions() {
        let view = mk_view("detail", vec!["tags".into(), "imports".into()], vec![]);
        let module = mk_module(view);
        let findings = check(&module, Path::new("customer.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "detail");
        assert_eq!(Finding::CODE, "TEST-VIEW-EXTENSIBILITY-001");
        assert!(findings[0].message().contains("tags, imports"));
    }

    #[test]
    fn quiet_when_extensible_view_has_at_least_one_assertion() {
        let view = mk_view(
            "detail",
            vec!["tags".into()],
            vec![ViewTestAssertion::AcceptedBy {
                feature: "tags".into(),
                span_ref: None,
            }],
        );
        let module = mk_module(view);
        assert!(check(&module, Path::new("customer.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_view_is_not_extensible() {
        // No `extensible_by` declared — the rule is silent regardless of
        // missing assertions because the view does not promise an
        // extension surface.
        let view = mk_view("readonly_summary", vec![], vec![]);
        let module = mk_module(view);
        assert!(check(&module, Path::new("customer.lzx")).is_empty());
    }

    #[test]
    fn rejected_by_alone_satisfies_the_rule() {
        // `rejected by` is also a closure: it pins which feature is
        // explicitly NOT admitted at the anchor.
        let view = mk_view(
            "detail",
            vec!["tags".into()],
            vec![ViewTestAssertion::RejectedBy {
                feature: "billing".into(),
                span_ref: None,
            }],
        );
        let module = mk_module(view);
        assert!(check(&module, Path::new("customer.lzx")).is_empty());
    }

    #[test]
    fn multiple_views_each_evaluated_independently() {
        let mut module = mk_module(mk_view("detail", vec!["tags".into()], vec![]));
        module.experiences[0].views.push(mk_view(
            "summary",
            vec!["tags".into()],
            vec![ViewTestAssertion::AcceptedBy {
                feature: "tags".into(),
                span_ref: None,
            }],
        ));
        module.experiences[0].views.push(mk_view("plain", vec![], vec![]));

        let findings = check(&module, Path::new("customer.lzx"));
        assert_eq!(findings.len(), 1, "only `detail` should fire");
        assert_eq!(findings[0].view, "detail");
    }

    #[test]
    fn check_view_returns_some_when_finding_present() {
        let exp = Experience {
            name: "customer".into(),
            imports: vec![],
            views: vec![],
            resume_routers: vec![],
            extensions: vec![],
            span_ref: None,
        };
        let view = mk_view("detail", vec!["tags".into()], vec![]);
        let result = check_view(&exp, &view, Path::new("customer.lzx"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().view, "detail");
    }
}
