//! TEST-RESTATES-POLICY-001 — authored `as @role.X` tests shadow generated matrix.
//!
//! Per `docs/proposals/test-completeness-lints.md` §TEST-RESTATES-POLICY-001.
//! Fires when a `tests` block contains an actor-only assertion (`AllowsAs`,
//! `DeniesAs`, `AllowsFromAs`, or `DeniesFromAs`) and the construct's `policy`
//! resolves to a feature-local policy category — meaning the actor-matrix
//! generator owns this assertion shape. Assertions paired with a `when <expr>`
//! predicate are exempt because the generator cannot emit predicate-gated rows.
//!
//! Resolution is conservative — fire only when the policy resolves to a
//! declared local category (`Policies::categories`). External / atom-only
//! policies short-circuit (no shadow risk).
//!
//! Severity: `warning` (strict + production). Escalates to `error` once the
//! Phase 2 actor-matrix generator ships.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PolicyRef, SpanRef, TestAssertion, TestBlock};

/// One TEST-RESTATES-POLICY-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` source path that hosts the construct.
    pub path: PathBuf,
    /// Feature containing the construct.
    pub feature: String,
    /// Carrier kind (`command`, `workflow_transition`, …).
    pub construct_kind: String,
    /// Construct name.
    pub construct: String,
    /// Actor literal (e.g. `@role.admin`) that the shadow assertion targets.
    pub actor: String,
    /// Optional span pointer for editor jumps.
    pub span: Option<SpanRef>,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-RESTATES-POLICY-001";

    /// Render the user-facing diagnostic body — names the shadow actor
    /// and points at the auto-generated permits/forbids matrix.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_restates_policy_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     construct_kind: "command".into(),
    ///     construct: "delete".into(),
    ///     actor: "@role.admin".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("@role.admin"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} `{}` actor-only test (`as {}`) shadows the generated permits/forbids \
             matrix derived from the construct's `policy`. Delete the shadow \
             assertion, or pair it with a `when <expr>` predicate that the \
             generator cannot emit.",
            self.construct_kind, self.construct, self.actor
        )
    }
}

/// Run TEST-RESTATES-POLICY-001 over a feature. Fires when an
/// actor-only assertion shadows a generator-emitted permits/forbids
/// matrix; only carriers with a local policy are inspected.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_restates_policy_001::check;
///
/// let findings = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Commands ----------------------------------------------------------------
    for cmd in &feature.commands {
        if !resolves_to_local_policy(&cmd.policy, feature) {
            continue;
        }
        if let Some(tests) = &cmd.tests {
            for assertion in &tests.assertions {
                if let Some(actor) = actor_only_target(assertion) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        construct_kind: "command".to_owned(),
                        construct: cmd.name.clone(),
                        actor,
                        span: tests.span_ref.clone(),
                    });
                }
            }
        }
    }

    // Workflow transitions ----------------------------------------------------
    for workflow in &feature.workflows {
        // Workflow.transitions[*] uses `requires` (Option<String>) for overrides;
        // default policy comes from workflow.default_policy.
        for transition in &workflow.transitions {
            let policy = transition
                .requires
                .as_ref()
                .map(|s| PolicyRef::Local(s.clone()))
                .or_else(|| workflow.default_policy.clone());
            let Some(policy) = policy else {
                continue;
            };
            if !resolves_to_local_policy(&policy, feature) {
                continue;
            }
            if let Some(tests) = &transition.tests {
                visit(
                    tests,
                    feature,
                    path,
                    "workflow_transition",
                    &format!("{}.{}", workflow.name, transition.name),
                    &mut findings,
                );
            }
        }
    }

    // Lifecycle transitions ---------------------------------------------------
    for resource in &feature.resources {
        if let Some(lifecycle) = &resource.lifecycle {
            for transition in &lifecycle.transitions {
                let Some(policy) = transition.policy.as_ref().or(transition.requires.as_ref())
                else {
                    continue;
                };
                if !resolves_to_local_policy(policy, feature) {
                    continue;
                }
                if let Some(tests) = &transition.tests {
                    visit(
                        tests,
                        feature,
                        path,
                        "lifecycle_transition",
                        &format!("{}.{}", resource.name, transition.name),
                        &mut findings,
                    );
                }
            }
        }
    }

    findings
}

fn visit(
    tests: &TestBlock,
    feature: &Feature,
    path: &Path,
    construct_kind: &str,
    construct: &str,
    out: &mut Vec<Finding>,
) {
    for assertion in &tests.assertions {
        if let Some(actor) = actor_only_target(assertion) {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                construct_kind: construct_kind.to_owned(),
                construct: construct.to_owned(),
                actor,
                span: tests.span_ref.clone(),
            });
        }
    }
}

/// Returns the actor name when the assertion is actor-only (no `when` clause).
/// `AllowsFromAs`/`DeniesFromAs` are still actor-only (state + actor; no predicate).
fn actor_only_target(assertion: &TestAssertion) -> Option<String> {
    match assertion {
        TestAssertion::AllowsAs { actor } => Some(actor.clone()),
        TestAssertion::DeniesAs { actor } => Some(actor.clone()),
        TestAssertion::AllowsFromAs { actor, .. } => Some(actor.clone()),
        TestAssertion::DeniesFromAs { actor, .. } => Some(actor.clone()),
        _ => None,
    }
}

fn resolves_to_local_policy(policy: &PolicyRef, feature: &Feature) -> bool {
    let name = match policy {
        PolicyRef::Local(name) => name,
        _ => return false,
    };
    feature.policies.categories.iter().any(|c| &c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Policies, PolicyCategory,
    };

    fn mk_feature_with_command(cmd: Command, has_policy: bool) -> Feature {
        let mut policies = Policies::default();
        if has_policy {
            policies.categories.push(PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@role.admin".to_owned()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            });
        }
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies,
            errors: None,
            commands: vec![cmd],
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

    fn mk_command(policy: PolicyRef, tests: Option<TestBlock>) -> Command {
        Command {
            name: "archive".to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy,
            policy_expr: None,
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
            derived_from: None,
        }
    }

    #[test]
    fn allows_as_with_local_policy_fires() {
        let cmd = mk_command(
            PolicyRef::Local("update".to_owned()),
            Some(TestBlock {
                assertions: vec![TestAssertion::AllowsAs {
                    actor: "@role.admin".to_owned(),
                }],
                span_ref: None,
            }),
        );
        let feature = mk_feature_with_command(cmd, true);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].actor, "@role.admin");
    }

    #[test]
    fn no_policy_resolution_does_not_fire() {
        let cmd = mk_command(
            PolicyRef::Local("update".to_owned()),
            Some(TestBlock {
                assertions: vec![TestAssertion::AllowsAs {
                    actor: "@role.admin".to_owned(),
                }],
                span_ref: None,
            }),
        );
        // Feature has no policy categories — short-circuit.
        let feature = mk_feature_with_command(cmd, false);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn predicate_assertion_does_not_fire() {
        let cmd = mk_command(
            PolicyRef::Local("update".to_owned()),
            Some(TestBlock {
                assertions: vec![TestAssertion::AllowsWhen {
                    predicate: lazuli_ir::Predicate::And(vec![]),
                }],
                span_ref: None,
            }),
        );
        let feature = mk_feature_with_command(cmd, true);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
