//! TEST-PREDICATE-UNCOVERED-001 — `tests` block has one-side coverage only.
//!
//! Per `docs/proposals/test-completeness-lints.md` §TEST-PREDICATE-UNCOVERED-001,
//! a construct's `tests` block should carry both `allows when` AND `denies when`
//! predicate assertions to cover both sides of the boundary. v0.1 implements a
//! conservative shape check: fire when the block has predicate assertions but
//! ZERO assertions on one side (allows-only or denies-only).
//!
//! Full atom-level coverage (each predicate atom proven both true and false)
//! is deferred to v0.2 once we calibrate against ≥2 pilots.
//!
//! Severity: `info` (strict + production). Stays informational until calibrated.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, SpanRef, TestAssertion, TestBlock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub construct_kind: String,
    pub construct: String,
    /// One of `allows_only` or `denies_only`.
    pub side: &'static str,
    pub span: Option<SpanRef>,
}

impl Finding {
    pub const CODE: &'static str = "TEST-PREDICATE-UNCOVERED-001";

    pub fn message(&self) -> String {
        format!(
            "{} `{}` predicate tests carry {} coverage — add the matching boundary \
             assertion so both `allows when` and `denies when` cover the predicate.",
            self.construct_kind, self.construct, self.side
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for cmd in &feature.commands {
        if let Some(tests) = &cmd.tests {
            if let Some(side) = predicate_one_side(tests) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    construct_kind: "command".to_owned(),
                    construct: cmd.name.clone(),
                    side,
                    span: tests.span_ref.clone(),
                });
            }
        }
    }

    for rule in &feature.rules {
        if let Some(tests) = &rule.tests {
            if let Some(side) = predicate_one_side(tests) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    construct_kind: "rule".to_owned(),
                    construct: rule.title.clone(),
                    side,
                    span: tests.span_ref.clone(),
                });
            }
        }
    }

    for workflow in &feature.workflows {
        for transition in &workflow.transitions {
            if let Some(tests) = &transition.tests {
                if let Some(side) = predicate_one_side(tests) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        construct_kind: "workflow_transition".to_owned(),
                        construct: format!("{}.{}", workflow.name, transition.name),
                        side,
                        span: tests.span_ref.clone(),
                    });
                }
            }
        }
    }

    for resource in &feature.resources {
        if let Some(lifecycle) = &resource.lifecycle {
            for transition in &lifecycle.transitions {
                if let Some(tests) = &transition.tests {
                    if let Some(side) = predicate_one_side(tests) {
                        findings.push(Finding {
                            path: path.to_path_buf(),
                            feature: feature.name.clone(),
                            construct_kind: "lifecycle_transition".to_owned(),
                            construct: format!("{}.{}", resource.name, transition.name),
                            side,
                            span: tests.span_ref.clone(),
                        });
                    }
                }
            }
        }
    }

    findings
}

/// Returns `Some("allows_only" | "denies_only")` when the block has `*When`
/// predicate assertions on only one side, `None` otherwise.
fn predicate_one_side(tests: &TestBlock) -> Option<&'static str> {
    let mut has_allows = false;
    let mut has_denies = false;
    for assertion in &tests.assertions {
        match assertion {
            TestAssertion::AllowsWhen { .. } => has_allows = true,
            TestAssertion::DeniesWhen { .. } => has_denies = true,
            _ => {}
        }
    }
    match (has_allows, has_denies) {
        (true, false) => Some("allows_only"),
        (false, true) => Some("denies_only"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        CompareOp, Defaults, Expr, OperationKind, OperationRef, Policies, Predicate,
        QualifiedName, Rule,
    };

    fn mk_allows_predicate() -> TestAssertion {
        TestAssertion::AllowsWhen {
            predicate: Predicate::Comparison {
                left: Expr::Path(lazuli_ir::Path::from_segments(["self", "x"])),
                op: CompareOp::Eq,
                right: Expr::Nil,
            },
        }
    }

    fn mk_denies_predicate() -> TestAssertion {
        TestAssertion::DeniesWhen {
            predicate: Predicate::Comparison {
                left: Expr::Path(lazuli_ir::Path::from_segments(["self", "x"])),
                op: CompareOp::Ne,
                right: Expr::Nil,
            },
        }
    }

    fn mk_rule_with_tests(assertions: Vec<TestAssertion>) -> Rule {
        Rule {
            title: "rule_a".to_owned(),
            denies: OperationRef {
                resource: QualifiedName {
                    feature: None,
                    name: "X".to_owned(),
                },
                op_name: "noop".to_owned(),
                kind: OperationKind::Unresolved,
            },
            when: Predicate::And(vec![]),
            message: String::new(),
            message_ref: None,
            tests: Some(TestBlock {
                assertions,
                span_ref: None,
            }),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(rules: Vec<Rule>) -> Feature {
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules,
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
    fn allows_only_fires() {
        let feature = mk_feature(vec![mk_rule_with_tests(vec![mk_allows_predicate()])]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].side, "allows_only");
    }

    #[test]
    fn denies_only_fires() {
        let feature = mk_feature(vec![mk_rule_with_tests(vec![mk_denies_predicate()])]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].side, "denies_only");
    }

    #[test]
    fn both_sides_does_not_fire() {
        let feature = mk_feature(vec![mk_rule_with_tests(vec![
            mk_allows_predicate(),
            mk_denies_predicate(),
        ])]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn non_predicate_assertions_alone_do_not_fire() {
        // E.g. allows-as / denies-as actor-only tests
        let feature = mk_feature(vec![mk_rule_with_tests(vec![TestAssertion::AllowsAs {
            actor: "@role.admin".to_owned(),
        }])]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
