//! TEST-RESTATES-EFFECT-001 — `allows when` assertion mirrors construct's own effect.
//!
//! Per `docs/proposals/test-completeness-lints.md` §TEST-RESTATES-EFFECT-001.
//! Fires when an `allows when self.<field> <op> <literal>` assertion's LHS
//! field is in the construct's WRITES set AND the literal matches a written
//! value. For lifecycle transitions the WRITES set is built from
//! `timestamps <field>`. For commands it includes every LHS field in
//! `creates`/`updates` assignments.
//!
//! `denies when` shape is exempt — denies often legitimately asserts the
//! pre-image had a specific shape.
//!
//! Severity: `warning` (strict + production).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Assignment, CommandEffect, Expr, Feature, Predicate, SpanRef, TestAssertion};

/// One TEST-RESTATES-EFFECT-001 finding.
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
    /// Field name being restated.
    pub field: String,
    /// Optional span pointer for editor jumps.
    pub span: Option<SpanRef>,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-RESTATES-EFFECT-001";

    /// Render the user-facing diagnostic body — names the restated
    /// field and points at the runtime effect that already guarantees it.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_restates_effect_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     construct_kind: "command".into(),
    ///     construct: "publish".into(),
    ///     field: "status".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("status"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} `{}` `allows when` assertion restates the construct's own effect on \
             field `{}` — the runtime guarantees the effect; `tests` should cover \
             inference beyond it. Delete the shadow assertion or replace with a \
             predicate beyond the construct's WRITES set.",
            self.construct_kind, self.construct, self.field
        )
    }
}

/// Run TEST-RESTATES-EFFECT-001 over a feature. Fires when an
/// `allows when` assertion's predicate touches a field the construct
/// is already known to write — the assertion adds no inference value.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_restates_effect_001::check;
///
/// let findings = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Commands: WRITES set = LHS field names in creates/updates assignments.
    for cmd in &feature.commands {
        let writes = command_writes(&cmd.effect);
        if writes.is_empty() {
            continue;
        }
        if let Some(tests) = &cmd.tests {
            for assertion in &tests.assertions {
                if let TestAssertion::AllowsWhen { predicate } = assertion {
                    for field in restated_fields(predicate, &writes) {
                        findings.push(Finding {
                            path: path.to_path_buf(),
                            feature: feature.name.clone(),
                            construct_kind: "command".to_owned(),
                            construct: cmd.name.clone(),
                            field,
                            span: tests.span_ref,
                        });
                    }
                }
            }
        }
    }

    // Lifecycle transitions: WRITES set = `timestamps <field>` slot.
    for resource in &feature.resources {
        if let Some(lifecycle) = &resource.lifecycle {
            for transition in &lifecycle.transitions {
                let mut writes = BTreeSet::new();
                if let Some(ts) = &transition.timestamps {
                    writes.insert(ts.clone());
                }
                if writes.is_empty() {
                    continue;
                }
                if let Some(tests) = &transition.tests {
                    for assertion in &tests.assertions {
                        if let TestAssertion::AllowsWhen { predicate } = assertion {
                            for field in restated_fields(predicate, &writes) {
                                findings.push(Finding {
                                    path: path.to_path_buf(),
                                    feature: feature.name.clone(),
                                    construct_kind: "lifecycle_transition".to_owned(),
                                    construct: format!("{}.{}", resource.name, transition.name),
                                    field,
                                    span: tests.span_ref,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    findings
}

fn command_writes(effect: &CommandEffect) -> BTreeSet<String> {
    let mut writes = BTreeSet::new();
    match effect {
        CommandEffect::Creates(c) => {
            for Assignment { field, .. } in &c.assignments {
                writes.insert(field.clone());
            }
        }
        CommandEffect::Updates(u) => {
            for Assignment { field, .. } in &u.assignments {
                writes.insert(field.clone());
            }
        }
        CommandEffect::Deletes(_)
        | CommandEffect::Reorders(_)
        | CommandEffect::Returns(_)
        | CommandEffect::None => {}
    }
    writes
}

/// Collects field names in the predicate's leaf comparisons whose LHS path
/// resolves to `self.<field>` or `target.<field>` AND whose RHS is a literal
/// (string / integer / nil / boolean / enum). Returns the subset that is in
/// `writes`.
fn restated_fields(predicate: &Predicate, writes: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect(predicate, writes, &mut out);
    out
}

fn collect(predicate: &Predicate, writes: &BTreeSet<String>, out: &mut Vec<String>) {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            if let Some(field) = leaf_self_field(left)
                && writes.contains(&field)
                && is_literal(right)
            {
                out.push(field);
            }
        }
        Predicate::And(parts) | Predicate::Or(parts) => {
            for p in parts {
                collect(p, writes, out);
            }
        }
        Predicate::Has { .. } => {}
    }
}

fn leaf_self_field(expr: &Expr) -> Option<String> {
    if let Expr::Path(p) = expr {
        let segments = &p.segments;
        if segments.len() == 2 && (segments[0] == "self" || segments[0] == "target") {
            return Some(segments[1].clone());
        }
    }
    None
}

fn is_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::String(_) | Expr::Integer(_) | Expr::Boolean(_) | Expr::Enum(_) | Expr::Nil,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CompareOp, Defaults, Field, Lifecycle, LifecycleState, LifecycleStateKind,
        LifecycleTransition, Policies, Resource, TestBlock, TypeRef,
    };

    fn mk_resource_with_transition(transition: LifecycleTransition) -> Resource {
        Resource {
            name: "Post".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: vec![Field {
                name: "published_at".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::DateTime),
                required: false,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: Default::default(),
                full_text: false,
                previous_names: vec![],
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            }],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: Some(Lifecycle {
                discriminator_field: "status".to_owned(),
                generated_enum: "PostStatus".to_owned(),
                states: vec![
                    LifecycleState {
                        name: "draft".to_owned(),
                        kind: LifecycleStateKind::Initial,
                        span_ref: None,
                    },
                    LifecycleState {
                        name: "published".to_owned(),
                        kind: LifecycleStateKind::Terminal,
                        span_ref: None,
                    },
                ],
                transitions: vec![transition],
                invariants: vec![],
                invariant_handlers: vec![],
                previous_names: vec![],
                span_ref: None,
            }),
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

    fn mk_feature(resource: Resource) -> Feature {
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
            resources: vec![resource],
            events: vec![],
            rules: vec![],
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
    fn allows_when_published_at_not_nil_after_timestamps_fires() {
        // transition stamps `published_at`; test re-asserts the effect.
        let transition = LifecycleTransition {
            name: "publish".to_owned(),
            from: vec!["draft".to_owned()],
            to: "published".to_owned(),
            policy: None,
            audit: None,
            timestamps: Some("published_at".to_owned()),
            emits: vec![],
            requires: None,
            tests: Some(TestBlock {
                assertions: vec![TestAssertion::AllowsWhen {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["self", "published_at"])),
                        op: CompareOp::Ne,
                        right: Expr::Nil,
                    },
                }],
                span_ref: None,
            }),
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(mk_resource_with_transition(transition));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "published_at");
    }

    #[test]
    fn denies_when_does_not_fire() {
        // denies-side pre-image checks are exempt.
        let transition = LifecycleTransition {
            name: "publish".to_owned(),
            from: vec!["draft".to_owned()],
            to: "published".to_owned(),
            policy: None,
            audit: None,
            timestamps: Some("published_at".to_owned()),
            emits: vec![],
            requires: None,
            tests: Some(TestBlock {
                assertions: vec![TestAssertion::DeniesWhen {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["self", "published_at"])),
                        op: CompareOp::Ne,
                        right: Expr::Nil,
                    },
                }],
                span_ref: None,
            }),
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(mk_resource_with_transition(transition));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn predicate_on_different_field_does_not_fire() {
        let transition = LifecycleTransition {
            name: "publish".to_owned(),
            from: vec!["draft".to_owned()],
            to: "published".to_owned(),
            policy: None,
            audit: None,
            timestamps: Some("published_at".to_owned()),
            emits: vec![],
            requires: None,
            tests: Some(TestBlock {
                assertions: vec![TestAssertion::AllowsWhen {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["self", "error_reason"])),
                        op: CompareOp::Eq,
                        right: Expr::Nil,
                    },
                }],
                span_ref: None,
            }),
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(mk_resource_with_transition(transition));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
