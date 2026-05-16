//! VOCAB-TESTS-MISSING-001 — feature with resources or commands but no tests.
//!
//! Fires when a feature declares at least one `resource` or `command.*` block
//! but declares zero inline `test` blocks anywhere in the feature.
//!
//! The current IR has no feature-level `tests` field.  Test blocks are attached
//! to commands, rules, workflow/lifecycle transitions, and view constructs, so
//! this lint treats any of those as evidence that the feature has authored test
//! vocabulary.
//!
//! Planned opt-out:
//!   `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."`
//!
//! v0 does not parse comments yet; the opt-out walker lands in a follow-up
//! cell.  v0 also does not implement the planned
//! feature-touched-in-last-N-commits filter, so downstream callers may suppress
//! legacy untouched features until that false-positive defense lands.
//!
//! Severity: `warning` (strict-profile), `warning` (production-profile).
//! Reference: docs/next-checklist.md §VOCAB-TESTS-MISSING-001

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-TESTS-MISSING-001 finding: a non-empty feature with no tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending feature.
    pub feature: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-TESTS-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares resources or commands but has no inline `test` blocks \
             — add at least one `test` block to make expected behavior visible to doctor, \
             codegen, and review tooling. If the omission is intentional, add \
             `# doctor:allow VOCAB-TESTS-MISSING-001 — reason \"...\"` near the feature.",
            self.feature
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-TESTS-MISSING-001 for one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.  The caller (doctor walker) maps each `Finding` into a
/// `DoctorDiagnostic` and supplies the exact source line from feature facts.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // TODO: opt-out wiring lands in a follow-up cell.
    if !has_subjects(feature) || has_any_test_block(feature) {
        return Vec::new();
    }

    vec![Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
    }]
}

// ── internals ─────────────────────────────────────────────────────────────────

fn has_subjects(feature: &Feature) -> bool {
    !feature.resources.is_empty() || !feature.commands.is_empty()
}

fn has_any_test_block(feature: &Feature) -> bool {
    feature.commands.iter().any(|cmd| cmd.tests.is_some())
        || feature.rules.iter().any(|rule| rule.tests.is_some())
        || feature.workflows.iter().any(|workflow| {
            workflow
                .transitions
                .iter()
                .any(|transition| transition.tests.is_some())
        })
        || feature.resources.iter().any(|resource| {
            resource.lifecycle.as_ref().map_or(false, |lifecycle| {
                lifecycle
                    .transitions
                    .iter()
                    .any(|transition| transition.tests.is_some())
            })
        })
    // Note: modern `Surface`/`Audience`/`View` (lzx grammar) carry no
    // `tests` field — view kinds (List/Detail/Create) deliberately leave
    // tests to commands. Legacy `LegacySurface`/`LegacyView` carry tests
    // but are no longer reachable from `Feature` (Feature.surfaces is the
    // modern path only). No surface-walk branch needed here.
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Policies, PolicyRef, Resource,
        TestBlock,
    };

    fn mk_cmd(name: &str, tests: Option<TestBlock>) -> Command {
        Command {
            name: name.to_owned(),
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
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
            tests,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
        }
    }

    fn mk_test_block() -> TestBlock {
        TestBlock {
            assertions: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn resource_only_without_tests_fires() {
        let feature = mk_feature(vec![mk_resource("Post")], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "test_feat");
        assert_eq!(Finding::CODE, "VOCAB-TESTS-MISSING-001");
        assert!(
            findings[0].message().contains("doctor:allow"),
            "message should mention the planned opt-out"
        );
    }

    #[test]
    fn command_only_without_tests_fires() {
        let feature = mk_feature(vec![], vec![mk_cmd("publish", None)]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn resource_and_command_with_one_test_does_not_fire() {
        let feature = mk_feature(
            vec![mk_resource("Post")],
            vec![mk_cmd("publish", Some(mk_test_block()))],
        );
        assert!(
            check(&feature, Path::new("features/post/post.lzi")).is_empty(),
            "one declared command test block satisfies the feature-level lint"
        );
    }

    #[test]
    fn empty_feature_does_not_fire() {
        let feature = mk_feature(vec![], vec![]);
        assert!(
            check(&feature, Path::new("features/empty/empty.lzi")).is_empty(),
            "empty features are not subject to VOCAB-TESTS-MISSING-001"
        );
    }

    #[test]
    fn many_subjects_without_tests_emit_one_feature_finding() {
        let resources = (0..5)
            .map(|i| mk_resource(&format!("Resource{i}")))
            .collect();
        let commands = (0..3)
            .map(|i| mk_cmd(&format!("command_{i}"), None))
            .collect();
        let feature = mk_feature(resources, commands);
        let findings = check(&feature, Path::new("features/bulk/bulk.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "lint is per-feature, not per resource or command"
        );
    }
}
