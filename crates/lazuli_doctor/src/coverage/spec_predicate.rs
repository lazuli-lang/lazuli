//! `spec_predicate` coverage layer.
//!
//! Walks IR predicates on `command.requires`-style constructs and
//! counts branches; the numerator is branches with ≥1 authored
//! coverage on both sides (`allows when ...` AND `denies when ...`)
//! inside the construct's `tests` block.
//!
//! Branch enumeration (Wave 6.2 line "Number of predicate branches
//! across all `requires`/`rule`/`when`-clauses"):
//!
//! - A `Comparison` or `Has` predicate counts as 1 atomic branch.
//! - An `And(...)` / `Or(...)` node multiplies; each leaf is a branch.
//!   (Closed predicate language: no nested booleans, no NOT.) For v1
//!   we count the **leaf count** as the branch count — the simplest
//!   honest measure that does not overstate truth-table cardinality.
//!
//! A branch is **covered** when the construct's `tests` block carries
//! at least one `AllowsWhen` AND one `DeniesWhen` assertion whose
//! predicates touch the same root identifier (`self`/`target`/etc.).
//! This is intentionally lenient: v1 measures "the boundary was
//! exercised on both sides", not "every literal was probed". Tighter
//! semantics are a follow-up; honest reporting beats false precision.

use lazuli_ir::{
    Command, Expr, Feature, LifecycleTransition, Path, Predicate, Rule, TestAssertion, TestBlock,
};

use super::LayerCoverage;

/// Compute the `spec_predicate` layer for `features`. Each leaf inside
/// `requires`/`rule`/lifecycle `when` clauses counts as one branch; a
/// branch is covered when the construct's tests carry at least one
/// `AllowsWhen` AND one `DeniesWhen` touching the same root identifier.
/// Always emits `source = "ir-walk"`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::coverage::spec_predicate::compute;
///
/// let layer = compute(&[]);
/// assert_eq!(layer.total, 0);
/// ```
pub fn compute(features: &[Feature]) -> LayerCoverage {
    let mut total = 0usize;
    let mut covered = 0usize;
    for feature in features {
        for cmd in &feature.commands {
            count_command(cmd, &mut total, &mut covered);
        }
        for rule in &feature.rules {
            count_rule(rule, &mut total, &mut covered);
        }
        for workflow in &feature.workflows {
            for t in &workflow.transitions {
                count_transition_requires(t, &mut total, &mut covered);
            }
        }
        for resource in &feature.resources {
            if let Some(lifecycle) = &resource.lifecycle {
                for t in &lifecycle.transitions {
                    count_lifecycle_transition_requires(t, &mut total, &mut covered);
                }
            }
        }
    }
    LayerCoverage::new(covered, total).with_source("ir-walk")
}

fn count_command(cmd: &Command, total: &mut usize, covered: &mut usize) {
    // `Command` does not carry an authored `requires` predicate slot
    // (it's expressed through the policy / target / lets shape). The
    // `tests` block can still carry `AllowsWhen`/`DeniesWhen` rows
    // covering implicit truth-table branches over `target.<field>`.
    // We approximate by treating each unique `target.<path>` reference
    // inside the tests block as one branch.
    let Some(tests) = &cmd.tests else {
        return;
    };
    let touched = touched_paths(tests);
    for path in &touched {
        *total += 1;
        if has_both_sides_for_path(tests, path) {
            *covered += 1;
        }
    }
}

fn count_rule(rule: &Rule, total: &mut usize, covered: &mut usize) {
    let branches = enumerate_branches(&rule.when);
    let branch_paths: Vec<Path> = branches.iter().filter_map(extract_path).collect();
    let tests = rule.tests.as_ref();
    for path in &branch_paths {
        *total += 1;
        if let Some(tb) = tests {
            if has_both_sides_for_path(tb, path) {
                *covered += 1;
            }
        }
    }
}

fn count_transition_requires(
    t: &lazuli_ir::Transition,
    total: &mut usize,
    covered: &mut usize,
) {
    // `Transition.requires: Option<String>` is the policy-bar form, not
    // a predicate. Its branch space is captured by `spec_actor_matrix`.
    // The `tests` block under a transition may still carry
    // `AllowsWhen`/`DeniesWhen` (which the closed catalog permits via
    // §6413 "Inline declarative assertions about IR shape"). Count
    // those the same way as `count_command`.
    let Some(tests) = &t.tests else {
        return;
    };
    let touched = touched_paths(tests);
    for path in &touched {
        *total += 1;
        if has_both_sides_for_path(tests, path) {
            *covered += 1;
        }
    }
}

fn count_lifecycle_transition_requires(
    t: &LifecycleTransition,
    total: &mut usize,
    covered: &mut usize,
) {
    let Some(tests) = &t.tests else {
        return;
    };
    let touched = touched_paths(tests);
    for path in &touched {
        *total += 1;
        if has_both_sides_for_path(tests, path) {
            *covered += 1;
        }
    }
}

/// Flatten an arbitrary `Predicate` into a list of leaf predicates.
/// `And` / `Or` enumeration is leaf-count (one per atomic
/// `Comparison`/`Has`), not truth-table cardinality.
fn enumerate_branches(p: &Predicate) -> Vec<Predicate> {
    match p {
        Predicate::Comparison { .. } | Predicate::Has { .. } => vec![p.clone()],
        Predicate::And(parts) | Predicate::Or(parts) => {
            parts.iter().flat_map(enumerate_branches).collect()
        }
    }
}

fn extract_path(p: &Predicate) -> Option<Path> {
    match p {
        Predicate::Comparison { left, .. } => path_of(left),
        Predicate::Has { collection, .. } => path_of(collection),
        Predicate::And(_) | Predicate::Or(_) => None,
    }
}

fn path_of(e: &Expr) -> Option<Path> {
    match e {
        Expr::Path(p) => Some(p.clone()),
        _ => None,
    }
}

/// Collect unique `Path`s mentioned by `allows when ...` /
/// `denies when ...` assertions inside the given tests block.
fn touched_paths(tests: &TestBlock) -> Vec<Path> {
    let mut seen: Vec<Path> = Vec::new();
    for a in &tests.assertions {
        let pred = match a {
            TestAssertion::AllowsWhen { predicate } | TestAssertion::DeniesWhen { predicate } => {
                predicate
            }
            _ => continue,
        };
        for leaf in enumerate_branches(pred) {
            if let Some(p) = extract_path(&leaf) {
                if !seen.iter().any(|existing| paths_eq(existing, &p)) {
                    seen.push(p);
                }
            }
        }
    }
    seen
}

fn has_both_sides_for_path(tests: &TestBlock, target: &Path) -> bool {
    let mut has_allows = false;
    let mut has_denies = false;
    for a in &tests.assertions {
        match a {
            TestAssertion::AllowsWhen { predicate } => {
                if predicate_touches_path(predicate, target) {
                    has_allows = true;
                }
            }
            TestAssertion::DeniesWhen { predicate } => {
                if predicate_touches_path(predicate, target) {
                    has_denies = true;
                }
            }
            _ => {}
        }
        if has_allows && has_denies {
            return true;
        }
    }
    has_allows && has_denies
}

fn predicate_touches_path(p: &Predicate, target: &Path) -> bool {
    for leaf in enumerate_branches(p) {
        if let Some(leaf_path) = extract_path(&leaf) {
            if paths_eq(&leaf_path, target) {
                return true;
            }
        }
    }
    false
}

fn paths_eq(a: &Path, b: &Path) -> bool {
    a.segments == b.segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{CompareOp, TestAssertion, TestBlock};

    fn path(seg: &[&str]) -> Path {
        Path {
            segments: seg.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn comparison(left: Path, right_lit: &str) -> Predicate {
        Predicate::Comparison {
            left: Expr::Path(left),
            op: CompareOp::Eq,
            right: Expr::String(right_lit.to_string()),
        }
    }

    fn feature_with_rule(when: Predicate, tests: Option<TestBlock>) -> Feature {
        let mut f = crate::coverage::test_support::empty_feature("f");
        f.rules.push(Rule {
            title: "r".to_string(),
            denies: lazuli_ir::OperationRef {
                resource: lazuli_ir::QualifiedName {
                    feature: None,
                    name: "X".to_string(),
                },
                op_name: "y".to_string(),
                kind: lazuli_ir::OperationKind::Command,
            },
            when,
            message: "m".to_string(),
            message_ref: None,
            tests,
            previous_names: Vec::new(),
            span_ref: None,
        });
        f
    }

    #[test]
    fn rule_with_no_tests_counts_uncovered() {
        let when = comparison(path(&["self", "status"]), "active");
        let f = feature_with_rule(when, None);
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn rule_with_both_sides_counts_covered() {
        let when = comparison(path(&["self", "status"]), "active");
        let tests = TestBlock {
            assertions: vec![
                TestAssertion::AllowsWhen {
                    predicate: comparison(path(&["self", "status"]), "active"),
                },
                TestAssertion::DeniesWhen {
                    predicate: comparison(path(&["self", "status"]), "archived"),
                },
            ],
            span_ref: None,
        };
        let f = feature_with_rule(when, Some(tests));
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }

    #[test]
    fn rule_with_only_one_side_counts_uncovered() {
        let when = comparison(path(&["self", "status"]), "active");
        let tests = TestBlock {
            assertions: vec![TestAssertion::AllowsWhen {
                predicate: comparison(path(&["self", "status"]), "active"),
            }],
            span_ref: None,
        };
        let f = feature_with_rule(when, Some(tests));
        let l = compute(&[f]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }
}
