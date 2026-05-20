//! VOCAB-HANDLER-HEAVY-001 — feature with a high handler-heavy command ratio.
//!
//! Fires when a feature has at least three commands and at least 70% of those
//! commands route through `@fn.<name>` handlers instead of staying declarative.
//!
//! Handler-heavy detection is IR-only:
//!   - `Command.effect == CommandEffect::None`
//!     (crates/lazuli_ir/src/lib.rs:1136; legacy / pure-handler path)
//!   - `Command.external_calls` is non-empty
//!     (crates/lazuli_ir/src/lib.rs:940; typed `calls <slot>.<op>` sites)
//!   - any command `let` binding or declarative `creates`/`updates`
//!     assignment has an `Expr::Path` whose joined segments contain `@fn.`
//!     (crates/lazuli_ir/src/lib.rs:1130 and :1440; analyzer lowers raw
//!     expressions into path segments until the typed expression pass lands)
//!
//! Severity: `warning`.
//! Reference: docs/next-checklist.md §VOCAB-HANDLER-HEAVY-001

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, Expr, Feature};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-HANDLER-HEAVY-001 finding: a feature whose commands mostly route
/// through imperative handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Feature name.
    pub feature: String,
    /// Number of commands classified as handler-heavy.
    pub handler_count: usize,
    /// Total command count in this feature.
    pub total_commands: usize,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-HANDLER-HEAVY-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` has {}/{} commands routed through `@fn.<name>` handlers (>70%). \
             Consider converting commands that just assign input fields to a resource into \
             `updates X {{ field = input.field }}` declarative form. Keep `@fn` for \
             cross-resource transactions, OAuth, OTP, or other irreducibly imperative work. \
             See docs/next-checklist.md.",
            self.feature, self.handler_count, self.total_commands
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-HANDLER-HEAVY-001 for one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here. Returns at most one finding per feature.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let total_commands = feature.commands.len();
    if total_commands < 3 {
        return Vec::new();
    }

    let handler_count = feature
        .commands
        .iter()
        .filter(|cmd| is_handler_heavy(cmd))
        .count();

    if handler_count * 100 >= total_commands * 70 {
        vec![Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            handler_count,
            total_commands,
        }]
    } else {
        Vec::new()
    }
}

// ── internals ─────────────────────────────────────────────────────────────────

fn is_handler_heavy(cmd: &Command) -> bool {
    matches!(cmd.effect, CommandEffect::None)
        || !cmd.external_calls.is_empty()
        || cmd
            .lets
            .iter()
            .any(|binding| expr_contains_fn_ref(&binding.value))
        || effect_contains_fn_ref(&cmd.effect)
}

fn effect_contains_fn_ref(effect: &CommandEffect) -> bool {
    match effect {
        CommandEffect::Creates(create) => create
            .assignments
            .iter()
            .any(|assignment| expr_contains_fn_ref(&assignment.value)),
        CommandEffect::Updates(update) => update
            .assignments
            .iter()
            .any(|assignment| expr_contains_fn_ref(&assignment.value)),
        CommandEffect::Deletes(_) | CommandEffect::Returns(_) | CommandEffect::None => false,
    }
}

fn expr_contains_fn_ref(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => {
            path.segments
                .first()
                .map_or(false, |segment| segment == "@fn")
                || path.segments.join(".").contains("@fn.")
        }
        Expr::FnCall(_) => true,
        Expr::String(_) | Expr::Integer(_) | Expr::Boolean(_) | Expr::Enum(_) | Expr::Nil => false,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Assignment, BuiltinType, CommandInput, CommandKind, CreateEffect, Defaults,
        ExternalCallRef, LetBinding, Policies, PolicyRef, QualifiedName, ReturnsEffect, TypeRef,
        UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_feature(commands: Vec<Command>) -> Feature {
        Feature {
            name: "alpha".into(),
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
            span_ref: None,
        }
    }

    fn mk_cmd(name: &str, effect: CommandEffect) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            CommandEffect::Returns(_) | CommandEffect::None => CommandKind::Returns,
        };
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
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
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn declarative_cmd(name: &str) -> Command {
        mk_cmd(
            name,
            CommandEffect::Updates(UpdateEffect {
                resource: qn("Alpha"),
                assignments: vec![Assignment {
                    field: "status".into(),
                    value: Expr::Path(lazuli_ir::Path::from_segments(["input", "status"])),
                }],
            }),
        )
    }

    fn handler_cmd(name: &str) -> Command {
        let mut cmd = mk_cmd(name, CommandEffect::None);
        cmd.lets.push(LetBinding {
            name: "resolved".into(),
            value: Expr::Path(lazuli_ir::Path::from_segments([
                "@fn",
                "resolve_alpha(input)",
            ])),
        });
        cmd
    }

    fn external_call_cmd(name: &str) -> Command {
        let mut cmd = mk_cmd(
            name,
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
        );
        cmd.external_calls.push(ExternalCallRef {
            slot: "crm".into(),
            op: "sync".into(),
            args: vec![],
            span_ref: None,
        });
        cmd
    }

    fn assignment_fn_cmd(name: &str) -> Command {
        mk_cmd(
            name,
            CommandEffect::Creates(CreateEffect {
                resource: qn("Alpha"),
                from_input: false,
                assignments: vec![Assignment {
                    field: "score".into(),
                    value: Expr::Path(lazuli_ir::Path::from_segments([
                        "@fn",
                        "score_alpha(input)",
                    ])),
                }],
            }),
        )
    }

    #[test]
    fn declarative_commands_do_not_fire() {
        let feature = mk_feature(vec![
            declarative_cmd("update_a"),
            declarative_cmd("update_b"),
            declarative_cmd("update_c"),
            declarative_cmd("update_d"),
            declarative_cmd("update_e"),
        ]);
        assert!(check(&feature, Path::new("features/alpha/host.lzi")).is_empty());
    }

    #[test]
    fn eighty_percent_handler_heavy_fires() {
        let feature = mk_feature(vec![
            declarative_cmd("update_a"),
            handler_cmd("resolve_b"),
            handler_cmd("resolve_c"),
            external_call_cmd("sync_d"),
            assignment_fn_cmd("score_e"),
        ]);
        let findings = check(&feature, Path::new("features/alpha/host.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].handler_count, 4);
        assert_eq!(findings[0].total_commands, 5);
        assert_eq!(Finding::CODE, "VOCAB-HANDLER-HEAVY-001");
        assert!(findings[0].message().contains("alpha"));
    }

    #[test]
    fn fifty_percent_handler_heavy_does_not_fire() {
        let feature = mk_feature(vec![
            declarative_cmd("update_a"),
            declarative_cmd("update_b"),
            handler_cmd("resolve_c"),
            handler_cmd("resolve_d"),
        ]);
        assert!(check(&feature, Path::new("features/alpha/host.lzi")).is_empty());
    }

    #[test]
    fn below_three_command_threshold_does_not_fire() {
        let feature = mk_feature(vec![handler_cmd("resolve_a"), declarative_cmd("update_b")]);
        assert!(check(&feature, Path::new("features/alpha/host.lzi")).is_empty());
    }

    #[test]
    fn zero_commands_do_not_fire() {
        let feature = mk_feature(vec![]);
        assert!(check(&feature, Path::new("features/alpha/host.lzi")).is_empty());
    }
}
