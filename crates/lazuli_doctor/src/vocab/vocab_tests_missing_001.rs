//! VOCAB-TESTS-MISSING-001 — feature with resources or commands but no tests.
//!
//! Fires when a feature declares at least one `resource` or `command.*` block
//! but carries zero **substantive** inline `tests` blocks anywhere in the
//! feature.
//!
//! ## What counts as "has tests"
//!
//! The IR has no feature-level `tests` field. Authored `tests { }` blocks hang
//! off commands, rules, and workflow/lifecycle transitions, and the analyzer
//! lowers each one into [`lazuli_ir::TestBlock`] via the shared
//! `lazuli_analyzer::test_lowering` path. This lint treats a lowered block with
//! at least one assertion (see [`block_has_substance`]) on ANY of those
//! constructs as evidence the feature has authored test vocabulary.
//!
//! Critically, this includes the dominant pilot form `allows when <pred>` /
//! `denies when <pred>`: the typed closed-`Predicate` parser is not yet wired,
//! so those lines lower to the sanctioned `TestAssertion::Raw` fallback
//! (non-empty) rather than being dropped. An authored command whose only tests
//! are `when`-predicate lines therefore satisfies this rule. (This was NOT true
//! before spec 0012 — command `tests` were hardcoded to `None` and `when` lines
//! were dropped, which is why both pilots once carried legitimate
//! `@doctor.allow(VOCAB-TESTS-MISSING-001, ...)` waivers reading "captured as
//! raw lines and do not lower". Those waivers are now retireable for any feature
//! that does author inline tests.)
//!
//! An empty `tests { }` body (zero assertions) is NOT coverage — see the
//! theater-promotion note on [`block_has_substance`].
//!
//! ## Opt-out
//!
//! `@doctor.allow(VOCAB-TESTS-MISSING-001, reason: "...")` (or the legacy
//! `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` comment) near the
//! feature suppresses the finding — wired in [`check`] below. Reserve this for
//! features that genuinely author no inline tests (behavior lives only in
//! `handlers/*_test.go`); features that DO author inline tests no longer need
//! it.
//!
//! v0 does not implement the planned feature-touched-in-last-N-commits filter,
//! so downstream callers may suppress legacy untouched features until that
//! false-positive defense lands.
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
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-TESTS-MISSING-001";

    /// Render the "feature has no inline test blocks" message and
    /// point at the canonical `# doctor:allow` escape hatch.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_tests_missing_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("f.lzi"), feature: "billing".into() };
    /// assert!(f.message().contains("inline `test` blocks"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_tests_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources/commands");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if !has_subjects(feature) || has_any_test_block(feature) {
        return Vec::new();
    }
    // 2026-05-27 — opt-out wiring: honor the `# doctor:allow VOCAB-TESTS-
    // MISSING-001 — reason "..."` comment that the rule's own message
    // tells authors to use. Without this, the message advertises an
    // escape hatch that doesn't exist (rule TODO was dormant).
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
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
    feature
        .commands
        .iter()
        .any(|cmd| block_has_substance(cmd.tests.as_ref()))
        || feature
            .rules
            .iter()
            .any(|rule| block_has_substance(rule.tests.as_ref()))
        || feature.workflows.iter().any(|workflow| {
            workflow
                .transitions
                .iter()
                .any(|transition| block_has_substance(transition.tests.as_ref()))
        })
        || feature.resources.iter().any(|resource| {
            resource.lifecycle.as_ref().is_some_and(|lifecycle| {
                lifecycle
                    .transitions
                    .iter()
                    .any(|transition| block_has_substance(transition.tests.as_ref()))
            })
        })
    // Note: modern `Surface`/`Audience`/`View` (lzx grammar) carry no
    // `tests` field; view kinds (List/Detail/Create) deliberately leave
    // tests to commands. No surface-walk branch needed here.
}

/// Theater promotion (2026-05-27): a `tests { }` block on a command
/// or transition is only "real" coverage when it carries at least
/// one assertion. An empty `tests { }` body declares intent without
/// content; we treat it as if no block were present so the
/// feature-level lint still fires and the author wires real
/// `allows when` / `denies when` / `permits` assertions.
fn block_has_substance(block: Option<&lazuli_ir::TestBlock>) -> bool {
    match block {
        Some(b) => !b.assertions.is_empty(),
        None => false,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Policies, PolicyRef, Resource,
        TestAssertion, TestBlock,
    };

    fn mk_cmd(name: &str, tests: Option<TestBlock>) -> Command {
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

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
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
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_test_block() -> TestBlock {
        // Substance-bearing block — one real assertion. After the
        // 2026-05-27 hardening, an empty `tests { }` no longer clears
        // the rule, so the "good citizen" fixture has to carry at
        // least one entry.
        TestBlock {
            assertions: vec![TestAssertion::PolicyAllow {
                actors: vec!["@role.admin".into()],
            }],
            span_ref: None,
        }
    }

    fn mk_empty_test_block() -> TestBlock {
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
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
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
    fn empty_test_block_still_fires() {
        // Theater regression guard (2026-05-27): a `tests { }` block
        // with zero assertions does NOT clear the rule. Authors who
        // type the keyword without filling in `permits` / `allows
        // when` get the same finding as if they had no block.
        let feature = mk_feature(
            vec![mk_resource("Post")],
            vec![mk_cmd("publish", Some(mk_empty_test_block()))],
        );
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].feature, "test_feat");
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
