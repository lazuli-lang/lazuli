//! TEST-MISSING-AUTHORED-001 — construct has authored predicate gate but no `tests` block.
//!
//! Fires per-construct (command / rule / workflow.transition / lifecycle.transition)
//! whose declared `requires` predicate is more than a bare `@policy.<X>` reference
//! AND whose `tests` slot is absent. Bare policy references are excluded because
//! the actor matrix is generated from `policy @policy.*` (permits/forbids) and
//! authored coverage adds no information beyond what the generator emits.
//!
//! Severity: `warning` (strict + production). Prototype profile skips the rule.
//!
//! Reference: `docs/proposals/test-completeness-lints.md` §TEST-MISSING-AUTHORED-001
//! and `docs/proposals/tdd-bdd-first-2026-05-23.md` Wave 1.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PolicyRef, SpanRef};

/// One TEST-MISSING-AUTHORED-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` source path that hosts the construct.
    pub path: PathBuf,
    /// Feature containing the construct.
    pub feature: String,
    /// One of `command`, `rule`, `workflow_transition`, `lifecycle_transition`.
    pub construct_kind: String,
    /// Construct name (e.g. command name, transition name, rule title).
    pub construct: String,
    /// Optional span pointer for editor jumps.
    pub span: Option<SpanRef>,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-MISSING-AUTHORED-001";

    /// Render the user-facing diagnostic body — names the construct
    /// and the missing inline `tests` block plus the override escape.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_missing_authored_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     construct_kind: "command".into(),
    ///     construct: "create_invoice".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("create_invoice"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} `{}` declares a predicate gate but has no `tests` block — add an \
             inline `tests` block covering the authored predicate boundary. If \
             the omission is intentional, override via `[doctor.test_discipline]` \
             with a `reason`.",
            self.construct_kind, self.construct
        )
    }
}

/// Run TEST-MISSING-AUTHORED-001 over every predicate-bearing carrier
/// in a feature. Fires when the carrier has an authored predicate
/// (`policy_expr`, rule `when`, transition guard) and no `tests` block.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_missing_authored_001::check;
///
/// let findings = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Commands: predicate gate = non-policy `requires`. Today commands carry
    // `policy` + `policy_expr` (predicate-shaped policies) — we treat the
    // construct as authored-predicate when `policy_expr` is set and `tests`
    // is empty.
    for cmd in &feature.commands {
        if cmd.tests.is_some() {
            continue;
        }
        if cmd.policy_expr.is_some() {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                construct_kind: "command".to_owned(),
                construct: cmd.name.clone(),
                span: cmd.span_ref.clone(),
            });
        }
    }

    // Rules always carry a closed-predicate `when` clause (Predicate AST).
    // Every rule has an authored predicate; `tests` is the only legitimate
    // coverage surface.
    for rule in &feature.rules {
        if rule.tests.is_some() {
            continue;
        }
        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            construct_kind: "rule".to_owned(),
            construct: rule.title.clone(),
            span: rule.span_ref.clone(),
        });
    }

    // Workflow transitions: predicate gate = `requires @policy.<override>`
    // (today the IR types this as `Option<String>`). When `requires` is set,
    // the transition raises the policy bar above the workflow default; that
    // is the predicate authoring surface this rule is named for.
    for workflow in &feature.workflows {
        for transition in &workflow.transitions {
            if transition.tests.is_some() {
                continue;
            }
            if transition.requires.is_some() {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    construct_kind: "workflow_transition".to_owned(),
                    construct: format!("{}.{}", workflow.name, transition.name),
                    span: transition.span_ref.clone(),
                });
            }
        }
    }

    // Lifecycle transitions: predicate gate = `requires @policy.<override>`
    // (typed `Option<PolicyRef>` here). Same predicate-authoring story as
    // workflow transitions.
    for resource in &feature.resources {
        if let Some(lifecycle) = &resource.lifecycle {
            for transition in &lifecycle.transitions {
                if transition.tests.is_some() {
                    continue;
                }
                if matches!(&transition.requires, Some(p) if !matches!(p, PolicyRef::None)) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        construct_kind: "lifecycle_transition".to_owned(),
                        construct: format!("{}.{}", resource.name, transition.name),
                        span: transition.span_ref.clone(),
                    });
                }
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, OperationKind, OperationRef,
        Policies, PolicyExpr, PolicyRef, Predicate, QualifiedName, Rule, TestBlock,
    };

    fn mk_command(name: &str, policy_expr: Option<PolicyExpr>, tests: Option<TestBlock>) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_rule(title: &str, tests: Option<TestBlock>) -> Rule {
        Rule {
            title: title.to_owned(),
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
            tests,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(commands: Vec<Command>, rules: Vec<Rule>) -> Feature {
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
            commands,
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
    fn command_with_policy_expr_no_tests_fires() {
        let cmd = mk_command(
            "create",
            Some(PolicyExpr::Authenticated),
            None,
        );
        let feature = mk_feature(vec![cmd], vec![]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct_kind, "command");
        assert_eq!(findings[0].construct, "create");
    }

    #[test]
    fn command_without_policy_expr_does_not_fire() {
        let cmd = mk_command("create", None, None);
        let feature = mk_feature(vec![cmd], vec![]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn command_with_tests_does_not_fire() {
        let cmd = mk_command(
            "create",
            Some(PolicyExpr::Authenticated),
            Some(TestBlock {
                assertions: vec![],
                span_ref: None,
            }),
        );
        let feature = mk_feature(vec![cmd], vec![]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn rule_without_tests_fires() {
        let rule = mk_rule("archived customers immutable", None);
        let feature = mk_feature(vec![], vec![rule]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct_kind, "rule");
    }

    #[test]
    fn rule_with_tests_does_not_fire() {
        let rule = mk_rule(
            "archived customers immutable",
            Some(TestBlock {
                assertions: vec![],
                span_ref: None,
            }),
        );
        let feature = mk_feature(vec![], vec![rule]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
