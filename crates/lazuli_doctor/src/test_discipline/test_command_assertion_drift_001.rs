//! TEST-COMMAND-ASSERTION-DRIFT-001 — `command tests` declares a `denies
//! when target.<field> = <value>` predicate that no IR-side guard
//! actually enforces.
//!
//! Example: fires when a command tests block declares `denies when
//! target.status = "removed"` but the target resource has no invariant /
//! lifecycle / trigger backing on the `status` field.
//!
//! Wave 4 widening (TDD/BDD-first proposal §7.1). The rule catches bug
//! #11 from the Hostpoint integration-suite pass 2 (`leave_host_reply`
//! handler ignored a `policy denies when target.status = removed`
//! declaration because the handler's WHERE clause only matched
//! `host_reply IS NULL`).
//!
//! Today the `tests` block on a command/rule/transition is
//! documentation-only — codegen does not consume it as a constraint
//! generator. This rule reverses that: every `DeniesWhen` predicate
//! targeting a resource field must be backed by an IR-level guard
//! (resource invariant, lifecycle state gate, or transition policy)
//! that would prevent the operation from running when the predicate
//! evaluates true. When no backing exists the rule fires `error`.
//!
//! ## Why this design (vs. the proposal's "cross-reference with codegen
//! output OR resource-level constraints")
//!
//! The proposal hints at two surfaces. Codegen output is *generated*
//! state — fragile to read from a doctor rule and tightly coupled to
//! emitter implementation details. Resource-level constraints (the
//! second option) live in the IR already, are stable across emitters,
//! and survive future codegen rewrites. Doctor reads only the IR.
//!
//! The trade-off: the rule cannot detect every drift shape (e.g. a
//! handler-level WHERE clause that's missing despite the IR being
//! consistent). What it CAN detect is the most common shape — the
//! lifecycle gate / invariant / policy-expr exists in spec but the
//! `tests` predicate targets a field that has no such backing. That
//! covers the `leave_host_reply` pattern: `target.status = removed` is
//! flagged unless the resource's lifecycle gates `removed` away from
//! the operation or an invariant forbids it.
//!
//! Severity: `error` (strict / production both).

use std::path::{Path, PathBuf};

use lazuli_ir::{
    Command, CommandEffect, CompareOp, Expr, Feature, Predicate, SpanRef, TestAssertion,
};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub command: String,
    /// Field referenced by the `denies when target.<field> = <value>`
    /// predicate (left-hand path's terminal segment).
    pub field: String,
    /// Verbatim debug rendering of the right-hand expression — useful
    /// for messages without recursively walking the closed `Expr`
    /// catalog.
    pub value: String,
    /// Resource that the command targets (drawn from `command.effect`).
    /// `None` when the command is a `Returns` shape (no implicit
    /// WHERE) — in that case the rule does not fire.
    pub resource: Option<String>,
    pub span_ref: Option<SpanRef>,
}

impl Finding {
    pub const CODE: &'static str = "TEST-COMMAND-ASSERTION-DRIFT-001";

    pub fn message(&self) -> String {
        let resource = self
            .resource
            .clone()
            .unwrap_or_else(|| "<unresolved>".to_string());
        format!(
            "command `{}.{}` tests assert `denies when target.{} = {}` but resource `{}` \
             has no IR-level guard (invariant, lifecycle state, or policy predicate) \
             enforcing that filter — the handler's WHERE clause may silently disagree \
             with the declared spec (see TDD/BDD-first proposal §7.1 / leave_host_reply \
             bug pattern). Either add a backing guard or remove the assertion to keep \
             tests honest.",
            self.feature, self.command, self.field, self.value, resource,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-COMMAND-ASSERTION-DRIFT-001 against one `Feature`.
///
/// Walks every command's `tests.assertions`; for each `DeniesWhen`
/// predicate of the shape `target.<field> = <literal>`, resolves the
/// target resource via `command.effect` and checks whether the resource
/// carries a backing guard. No I/O.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for command in &feature.commands {
        let Some(tests) = &command.tests else { continue };
        let target_resource = effect_resource(&command.effect);
        for assertion in &tests.assertions {
            let TestAssertion::DeniesWhen { predicate } = assertion else {
                continue;
            };
            for (field, value) in target_field_eq_value(predicate) {
                if backing_guard_exists(feature, command, target_resource, &field, &value) {
                    continue;
                }
                out.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    command: command.name.clone(),
                    field: field.clone(),
                    value: value.clone(),
                    resource: target_resource.map(str::to_string),
                    span_ref: tests.span_ref,
                });
            }
        }
    }
    out
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Returns the resource name (a `QualifiedName` short form) the command
/// targets via its effect. `None` for `Returns` (read-only) and `None`
/// (legacy) effects — these shapes have no implicit WHERE clause to
/// cross-check.
fn effect_resource(effect: &CommandEffect) -> Option<&str> {
    match effect {
        CommandEffect::Updates(u) => Some(u.resource.name.as_str()),
        CommandEffect::Deletes(d) => Some(d.resource.name.as_str()),
        CommandEffect::Creates(_) | CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// Flattens an `And` predicate tree and extracts every leaf comparison
/// of the shape `target.<field> = <literal-or-enum>`. Returns
/// `(field, value-render)` pairs.
fn target_field_eq_value(predicate: &Predicate) -> Vec<(String, String)> {
    let mut out = Vec::new();
    walk_predicate(predicate, &mut out);
    out
}

fn walk_predicate(predicate: &Predicate, out: &mut Vec<(String, String)>) {
    match predicate {
        Predicate::Comparison { left, op, right } => {
            if !matches!(op, CompareOp::Eq) {
                return;
            }
            let Some(field) = target_field_from_expr(left) else {
                return;
            };
            let value = render_expr(right);
            out.push((field, value));
        }
        Predicate::And(predicates) => {
            for p in predicates {
                walk_predicate(p, out);
            }
        }
        // `Or` is deliberately skipped: even one disjunct being unbacked
        // does not prove drift; the spec language is too soft to flag.
        // `Has` is collection-membership and out of the leave_host_reply
        // pattern; future widening can extend the rule.
        Predicate::Or(_) | Predicate::Has { .. } => {}
    }
}

fn target_field_from_expr(expr: &Expr) -> Option<String> {
    let Expr::Path(path) = expr else { return None };
    let segments = &path.segments;
    if segments.len() != 2 || segments[0] != "target" {
        return None;
    }
    Some(segments[1].clone())
}

fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::String(s) => format!("\"{s}\""),
        Expr::Integer(i) => i.to_string(),
        Expr::Boolean(b) => b.to_string(),
        Expr::Enum(e) => {
            let type_name = e
                .type_name
                .as_ref()
                .map(|q| q.name.as_str())
                .unwrap_or("<enum>");
            format!("{}.{}", type_name, e.variant)
        }
        Expr::Nil => "nil".to_string(),
        Expr::Path(p) => p.segments.join("."),
        Expr::FnCall(c) => format!("{}({} args)", c.name.name, c.args.len()),
    }
}

/// Returns `true` when the resource carries an IR-level guard that
/// could enforce the `target.<field> = <value>` filter. Three kinds of
/// backing are accepted today:
///
/// 1. A resource `Invariant` that references the field — assumed to
///    constrain it (the predicate body itself is `EvalPredicate`, which
///    we don't deeply analyze here; presence is enough to suppress the
///    false-positive while a more rigorous EvalPredicate parser lands).
/// 2. A `Resource.lifecycle` whose state machine could prevent the
///    operation by gating that field as the lifecycle discriminator.
/// 3. The command itself binds `triggers` to a lifecycle transition,
///    which already enforces a state filter on the resource.
fn backing_guard_exists(
    feature: &Feature,
    command: &Command,
    target_resource: Option<&str>,
    field: &str,
    _value: &str,
) -> bool {
    // Commands with triggers fire lifecycle transitions that already
    // enforce state filtering. Conservatively treat any trigger as
    // backing — the lifecycle transition's `from` set is the implicit
    // WHERE clause.
    if !command.triggers.is_empty() {
        return true;
    }

    let Some(resource_name) = target_resource else {
        // Without a target resource we cannot cross-check; do not fire
        // (avoid false positives on `Returns` shapes).
        return true;
    };

    let Some(resource) = feature
        .resources
        .iter()
        .find(|r| r.name == resource_name)
    else {
        // Resource lives in a different feature — out of scope for the
        // v0.1 rule. Future widening can take a `Module` instead of a
        // `Feature`.
        return true;
    };

    // (1) Resource invariant referencing the field.
    if resource
        .invariants
        .iter()
        .any(|inv| invariant_mentions_field(inv, field))
    {
        return true;
    }

    // (2) Lifecycle discriminator matches the asserted field — the
    // state machine is the WHERE clause. The `leave_host_reply` bug
    // had `status` as a non-lifecycle field, so this guard does NOT
    // trigger for that pattern (which is what we want).
    if let Some(lifecycle) = &resource.lifecycle {
        if lifecycle.discriminator_field == field {
            return true;
        }
    }

    false
}

fn invariant_mentions_field(inv: &lazuli_ir::Invariant, field: &str) -> bool {
    // `EvalPredicate` is a deeply-nested structure; we string-match
    // here through the serialized form. Cheap, conservative, and good
    // enough for the v0.1 rule — a deeper EvalPredicate walker lands
    // when Wave 0.5 / Wave 1 ships the shared predicate visitor.
    let serialized =
        serde_json::to_string(&inv.when).unwrap_or_default();
    serialized.contains(&format!("\"{field}\""))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CompareOp, Defaults,
        DeleteEffect, EvalPredicate, Expr, Feature, Field, FieldConstraints, Invariant, Lifecycle,
        LifecycleState, LifecycleStateKind, Path as IrPath, Policies, PolicyRef, Predicate,
        QualifiedName, Resource, TestAssertion, TestBlock, TypeRef, UpdateEffect,
    };

    fn mk_cmd(name: &str, effect: CommandEffect, tests: Option<TestBlock>) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect,
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
        }
    }

    fn mk_qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, lifecycle: Option<Lifecycle>, invariants: Vec<Invariant>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![mk_field("status"), mk_field("host_reply")],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle,
            invariants,
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "trust".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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

    fn denies_when_target_eq(field: &str, value: &str) -> TestBlock {
        TestBlock {
            assertions: vec![TestAssertion::DeniesWhen {
                predicate: Predicate::Comparison {
                    left: Expr::Path(IrPath::from_segments(["target", field])),
                    op: CompareOp::Eq,
                    right: Expr::String(value.to_string()),
                },
            }],
            span_ref: None,
        }
    }

    #[test]
    fn fires_for_leave_host_reply_pattern() {
        // The bug shape: `denies when target.status = removed` declared
        // on the command, but resource has no invariant or lifecycle
        // gate on `status`, so the handler's WHERE clause silently
        // ignores it.
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        let findings = check(&feature, Path::new("trust.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "status");
        assert_eq!(findings[0].command, "leave_host_reply");
        assert_eq!(findings[0].resource.as_deref(), Some("Review"));
        assert!(findings[0].message().contains("§7.1"));
    }

    #[test]
    fn quiet_when_lifecycle_discriminator_matches_field() {
        // If `status` IS the lifecycle field, the state machine is the
        // implicit WHERE clause — no drift.
        let lifecycle = Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "ReviewStatus".into(),
            states: vec![
                LifecycleState {
                    name: "active".into(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "removed".into(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", Some(lifecycle), vec![])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_invariant_mentions_field() {
        // Build an invariant whose serialized form mentions the
        // "status" field. The v0.1 rule string-matches the field name
        // through the JSON projection of `EvalPredicate` — cheap and
        // conservative pending a shared predicate visitor.
        let inv = Invariant {
            name: "host_reply_only_when_active".into(),
            when: EvalPredicate::Closed(Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["target", "status"])),
                op: CompareOp::Eq,
                right: Expr::String("active".to_string()),
            }),
            message: "".into(),
            span_ref: None,
        };
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", None, vec![inv])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_command_has_triggers() {
        // A command bound to lifecycle transitions inherits state
        // filtering — no drift possible.
        let mut cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        cmd.triggers.push("publish".to_string());
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_for_returns_commands() {
        // Read-only commands have no implicit WHERE clause to drift on.
        let cmd = mk_cmd(
            "summarize",
            CommandEffect::None,
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_for_delete_command_with_lifecycle_backing() {
        let lifecycle = Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "ReviewStatus".into(),
            states: vec![LifecycleState {
                name: "active".into(),
                kind: LifecycleStateKind::Initial,
                span_ref: None,
            }],
            transitions: vec![],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "drop",
            CommandEffect::Deletes(DeleteEffect {
                resource: mk_qn("Review"),
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", Some(lifecycle), vec![])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn ignores_non_target_paths() {
        // `input.X = Y` is not a WHERE clause — argument validation.
        // The rule stays silent.
        let tests = TestBlock {
            assertions: vec![TestAssertion::DeniesWhen {
                predicate: Predicate::Comparison {
                    left: Expr::Path(IrPath::from_segments(["input", "owner_id"])),
                    op: CompareOp::Eq,
                    right: Expr::Nil,
                },
            }],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "update_self",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(tests),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }
}
