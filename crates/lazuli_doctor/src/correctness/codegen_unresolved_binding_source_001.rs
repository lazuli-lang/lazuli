//! CODEGEN-UNRESOLVED-BINDING-SOURCE-001 — a command-effect binding RHS
//! resolves to none of the known binding-source kinds.
//!
//! **Severity:** error. This is a correctness gap, not style — the
//! unrecognized path would otherwise *silently* lower to a constant
//! string (`lazuli.FromConst("<raw source text>")`) in the Go emitter
//! (`crates/lazuli_codegen_go/src/emitter/command/effects_format.rs`),
//! producing garbage SQL instead of failing the build. This rule is the
//! promised "Cell I4" diagnostic that upgrades that silent fallback into
//! a hard, build-failing error before codegen runs.
//!
//! **Fires when** a `creates` / `updates` / `deletes` binding's RHS (a
//! SET assignment OR an authored `where <col> = <expr>` row) is a
//! `Path` whose head is none of the recognized source kinds
//! (`input` / `ctx` / `target` / `route`) and is not a bare `let`-bound
//! name on the command. Example fixtures that fire: `tenant_id =
//! foo.bar` (unknown head `foo`), `tenant_id = @fn.placeholder` lowered
//! as a *path* rather than a call (head `@fn` — no parens), or a stray
//! `where = …` column whose RHS path is unrecognized.
//!
//! **Does not fire** for a real literal (`role = "MEMBER"` →
//! `FromConst("MEMBER")` is correct), an `@fn.x()` call (lowers to
//! `FromFn`), a recognized path (`input.tier`, `ctx.actor.id`,
//! `route.id`, `target.x`), or a single-segment path naming a `let`
//! binding declared on the same command.

use std::path::{Path, PathBuf};

use lazuli_ir::{Assignment, Command, CommandEffect, Expr, Feature};

// output

/// One CODEGEN-UNRESOLVED-BINDING-SOURCE-001 finding: a command binding
/// whose RHS path resolves to no known source kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub command: String,
    /// The binding column (the assignment `field`, lowercased the same
    /// way codegen lowercases it for the emitted `Bindings` key).
    pub column: String,
    /// The unrecognized expression as the author would read it
    /// (`foo.bar`, `@fn.x` without parens, …).
    pub expr: String,
    /// `set` for a SET assignment, `where` for an authored where-clause
    /// binding — so the message names which axis tripped.
    pub axis: &'static str,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "CODEGEN-UNRESOLVED-BINDING-SOURCE-001";

    /// Render the "unresolved binding source" message — name the command,
    /// the column, the rendered expression, and the recognized kinds so
    /// the author knows what shapes are legal.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::codegen_unresolved_binding_source_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     command: "signup".into(),
    ///     column: "tenant_id".into(),
    ///     expr: "@fn.placeholder_tenant_id".into(),
    ///     axis: "set",
    /// };
    /// assert!(f.message().contains("none of"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command `{}` {} binding for column `{}` resolves to none of \
             {{input, ctx, target, route, @fn(), literal, let}}: `{}` would \
             silently lower to a constant string in codegen. Did you mean a \
             fn call (`@fn.{}()`), an input slot (`input.{}`), or a ctx path \
             (`ctx.{}`)?",
            self.command, self.axis, self.column, self.expr, self.expr, self.expr, self.expr,
        )
    }
}

// detection

/// Run CODEGEN-UNRESOLVED-BINDING-SOURCE-001 for all commands in one
/// feature.
///
/// `path` anchors findings; no I/O is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::codegen_unresolved_binding_source_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with commands");
/// let _ = check(&feature, Path::new("users.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();

    for command in &feature.commands {
        // Single-segment paths matching a `let` name are legitimate (the
        // emitter resolves them through `let_bindings`). Collect the
        // declared `let` names so we don't false-positive on them.
        let let_names: Vec<&str> = command.lets.iter().map(|l| l.name.as_str()).collect();

        match &command.effect {
            CommandEffect::Creates(create) => {
                check_assignments(command, &create.assignments, "set", &let_names, path, &mut out);
            }
            CommandEffect::Updates(update) => {
                check_assignments(command, &update.assignments, "set", &let_names, path, &mut out);
                check_assignments(
                    command,
                    &update.where_clause,
                    "where",
                    &let_names,
                    path,
                    &mut out,
                );
            }
            CommandEffect::Deletes(delete) => {
                check_assignments(
                    command,
                    &delete.where_clause,
                    "where",
                    &let_names,
                    path,
                    &mut out,
                );
            }
            CommandEffect::Reorders(_) | CommandEffect::Returns(_) | CommandEffect::None => {}
        }
    }

    out
}

// internals

fn check_assignments(
    command: &Command,
    assignments: &[Assignment],
    axis: &'static str,
    let_names: &[&str],
    path: &Path,
    out: &mut Vec<Finding>,
) {
    for assignment in assignments {
        if let Some(expr_repr) = unresolved_source(&assignment.value, let_names) {
            out.push(Finding {
                path: path.to_path_buf(),
                command: command.name.clone(),
                column: assignment.field.to_ascii_lowercase(),
                expr: expr_repr,
                axis,
            });
        }
    }
}

/// Classify a binding RHS. Returns `Some(rendered)` when the expression
/// is an unrecognized path/expr that codegen would silently lower to a
/// `FromConst("<raw>")` string; `None` when it resolves to a known
/// source kind (recognized path, fn call, or real literal).
///
/// Mirrors the dispatch in
/// `lazuli_codegen_go::emitter::command::effects_format::format_binding_source`
/// / `format_path_source` — the SAME classification the emitter uses, so
/// "the rule passes" ⟺ "the emitter never hits its fallback arm".
fn unresolved_source(expr: &Expr, let_names: &[&str]) -> Option<String> {
    match expr {
        // Real literals are legitimate `FromConst` sources — never a bug.
        Expr::String(_)
        | Expr::Integer(_)
        | Expr::Boolean(_)
        | Expr::Enum(_)
        | Expr::Nil
        // `@fn.x(...)` lowers to `FromFn` (recognized). Argument
        // resolution is the FnCall's own concern; the binding source is
        // resolved.
        | Expr::FnCall(_) => None,
        Expr::Path(p) => {
            // Single-segment path naming a `let` binding → resolved
            // (emitter renders it via `let_bindings`).
            if let [name] = p.segments.as_slice()
                && let_names.contains(&name.as_str())
            {
                return None;
            }
            let head = p.segments.first().map(|s| s.as_str()).unwrap_or("");
            match head {
                "input" | "ctx" | "target" | "route" => None,
                _ => Some(p.segments.join(".")),
            }
        }
    }
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        DeleteEffect, EnumLiteral, FnCallExpr, LetBinding, Path as IrPath, Policies, PolicyRef,
        QualifiedName, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn path_expr(segments: &[&str]) -> Expr {
        Expr::Path(IrPath::from_segments(segments.iter().copied()))
    }

    fn assign(field: &str, value: Expr) -> Assignment {
        Assignment {
            field: field.to_owned(),
            value,
        }
    }

    fn mk_cmd(name: &str, effect: CommandEffect) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            _ => CommandKind::Returns,
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
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(command: Command) -> Feature {
        Feature {
            name: "billing".into(),
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
            policies: Policies::default(),
            errors: None,
            commands: vec![command],
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

    fn creates(assignments: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Creates(CreateEffect {
            resource: qn("Account"),
            from_input: false,
            assignments,
        })
    }

    fn updates(assignments: Vec<Assignment>, where_clause: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Account"),
            assignments,
            where_clause,
        })
    }

    fn deletes(where_clause: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Deletes(DeleteEffect {
            resource: qn("Account"),
            where_clause,
        })
    }

    // (a) unrecognized path/expr → ERRORS, naming column + expr.

    #[test]
    fn positive_unknown_head_path_fires() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign("tenant_id", path_expr(&["foo", "bar"]))]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "CODEGEN-UNRESOLVED-BINDING-SOURCE-001");
        assert_eq!(findings[0].command, "create_account");
        assert_eq!(findings[0].column, "tenant_id");
        assert_eq!(findings[0].expr, "foo.bar");
        assert_eq!(findings[0].axis, "set");
        assert!(findings[0].message().contains("tenant_id"));
        assert!(findings[0].message().contains("foo.bar"));
    }

    #[test]
    fn positive_fn_path_without_parens_fires() {
        // Bug #2 shape: `@fn.placeholder_tenant_id` lowered as a PATH
        // (head `@fn`) rather than a FnCall — would silently become
        // FromConst("@fn.placeholder_tenant_id").
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign(
                "tenant_id",
                path_expr(&["@fn", "placeholder_tenant_id"]),
            )]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].expr, "@fn.placeholder_tenant_id");
    }

    #[test]
    fn positive_unresolved_where_rhs_fires() {
        let cmd = mk_cmd(
            "deactivate",
            updates(
                vec![assign("status", Expr::String("inactive".into()))],
                vec![assign("id", path_expr(&["bogus", "id"]))],
            ),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].axis, "where");
        assert_eq!(findings[0].column, "id");
        assert_eq!(findings[0].expr, "bogus.id");
    }

    #[test]
    fn positive_unresolved_delete_where_fires() {
        let cmd = mk_cmd("purge", deletes(vec![assign("id", path_expr(&["nope"]))]));
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].axis, "where");
        assert_eq!(findings[0].expr, "nope");
    }

    // (b) real literals → no error.

    #[test]
    fn negative_string_literal_does_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign("role", Expr::String("MEMBER".into()))]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_other_literals_do_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![
                assign("count", Expr::Integer(0)),
                assign("active", Expr::Boolean(true)),
                assign("deleted_at", Expr::Nil),
                assign(
                    "tier",
                    Expr::Enum(EnumLiteral {
                        type_name: Some(qn("Tier")),
                        variant: "Gold".into(),
                    }),
                ),
            ]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    // (c) recognized sources (input/route/ctx/target/@fn/let) → unaffected.

    #[test]
    fn negative_recognized_heads_do_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![
                assign("name", path_expr(&["input", "name"])),
                assign("owner_id", path_expr(&["ctx", "actor", "id"])),
                assign("prev", path_expr(&["target", "status"])),
                assign("id", path_expr(&["route", "id"])),
            ]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_fn_call_does_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign(
                "password_hash",
                Expr::FnCall(FnCallExpr {
                    name: qn("hash_password"),
                    args: vec![path_expr(&["input", "password"])],
                }),
            )]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_let_bound_single_segment_does_not_fire() {
        let mut cmd = mk_cmd(
            "create_account",
            creates(vec![assign("tenant_id", path_expr(&["tid"]))]),
        );
        cmd.lets = vec![LetBinding {
            name: "tid".into(),
            value: path_expr(&["ctx", "org", "id"]),
        }];
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_returns_command_does_not_fire() {
        // Returns/None/Reorders carry no binding RHS — never inspected.
        let cmd = mk_cmd(
            "noop",
            CommandEffect::Returns(lazuli_ir::ReturnsEffect {
                return_type: lazuli_ir::TypeRef::Builtin(BuiltinType::Boolean),
            }),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }
}
