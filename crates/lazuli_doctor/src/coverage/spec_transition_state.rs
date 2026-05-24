//! `spec_transition_state` coverage layer.
//!
//! Walks every `from <state>` declaration on `workflow.transition` and
//! `lifecycle.transition` across the package and counts. A `from`
//! source is **covered** when the transition's `tests` block carries
//! at least one `AllowsFrom`/`AllowsFromAs` assertion naming that
//! state. (`DeniesFrom` alone is NOT sufficient — coverage measures
//! that the happy path was asserted; deny coverage is a follow-up
//! tightening per Wave 6 acceptance bar.)
//!
//! `Transition.from` is a single string; `LifecycleTransition.from`
//! is a Vec<String> (fan-in) — each element counts as one source.

use lazuli_ir::{Feature, TestAssertion, TestBlock};

use super::LayerCoverage;

pub fn compute(features: &[Feature]) -> LayerCoverage {
    let mut total = 0usize;
    let mut covered = 0usize;
    for feature in features {
        for workflow in &feature.workflows {
            for t in &workflow.transitions {
                total += 1;
                if covers_from(&t.tests, &t.from) {
                    covered += 1;
                }
            }
        }
        for resource in &feature.resources {
            if let Some(lifecycle) = &resource.lifecycle {
                for t in &lifecycle.transitions {
                    for from_state in &t.from {
                        total += 1;
                        if covers_from(&t.tests, from_state) {
                            covered += 1;
                        }
                    }
                }
            }
        }
    }
    LayerCoverage::new(covered, total).with_source("ir-walk")
}

fn covers_from(tests: &Option<TestBlock>, state: &str) -> bool {
    let Some(tb) = tests else { return false };
    for a in &tb.assertions {
        match a {
            TestAssertion::AllowsFrom { state: s } => {
                if s == state {
                    return true;
                }
            }
            TestAssertion::AllowsFromAs { state: s, .. } => {
                if s == state {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::test_support::empty_feature;
    use lazuli_ir::{FieldRef, TestAssertion, TestBlock, Transition, Workflow};

    fn transition(name: &str, from: &str, tests: Option<TestBlock>) -> Transition {
        Transition {
            name: name.to_string(),
            from: from.to_string(),
            to: "next".to_string(),
            requires: None,
            emits: Vec::new(),
            tests,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn workflow_with(transitions: Vec<Transition>) -> Workflow {
        Workflow {
            name: "wf".to_string(),
            on: FieldRef {
                resource: lazuli_ir::QualifiedName {
                    feature: None,
                    name: "Post".to_string(),
                },
                field: "status".to_string(),
            },
            default_policy: None,
            default_emits: Vec::new(),
            transitions,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn workflow_from_state_no_tests_is_uncovered() {
        let mut f = empty_feature("f");
        f.workflows
            .push(workflow_with(vec![transition("publish", "draft", None)]));
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn workflow_with_allows_from_is_covered() {
        let mut f = empty_feature("f");
        let tests = TestBlock {
            assertions: vec![TestAssertion::AllowsFrom {
                state: "draft".to_string(),
            }],
            span_ref: None,
        };
        f.workflows
            .push(workflow_with(vec![transition("publish", "draft", Some(tests))]));
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }

    #[test]
    fn denies_from_alone_is_not_sufficient() {
        let mut f = empty_feature("f");
        let tests = TestBlock {
            assertions: vec![TestAssertion::DeniesFrom {
                state: "draft".to_string(),
            }],
            span_ref: None,
        };
        f.workflows
            .push(workflow_with(vec![transition("publish", "draft", Some(tests))]));
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }
}
