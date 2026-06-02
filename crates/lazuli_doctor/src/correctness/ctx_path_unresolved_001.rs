//! CTX-PATH-UNRESOLVED-001 — an author-written `ctx.<tail>` binding path
//! whose tail is not a recognized ctx slot.
//!
//! **Severity:** error. This is the "author-tail" gap the codegen↔runtime
//! ctx-path face-parity harness (`runtime/go/lazuli/readctx_parity_test.go`
//! + `crates/lazuli_codegen_go/tests/ctx_path_parity.rs`) explicitly
//! admitted it cannot cover: the codegen's `"ctx" =>` arm
//! (`crates/lazuli_codegen_go/src/emitter/command/effects_format.rs`)
//! lowers *whatever* an author types after `ctx.` to
//! `lazuli.FromCtx("<tail>")`, and the runtime `readCtx`
//! (`runtime/go/lazuli/handle.go`) 500s with `unknown ctx path: <tail>`
//! for any tail it doesn't have a `case` arm for. The parity tests only
//! pin the *framework-emitted* ctx paths (`actor.id`, `actor.org_id`, …);
//! a brand-new author-written `ctx.foo.bar` slips past them and is caught
//! only at runtime. This rule closes that gap at analyze time.
//!
//! **Single source of truth:** `runtime/go/lazuli/ctx_path_catalog.json`
//! — the SAME catalog both parity tests gate against. We embed it at
//! compile time (`include_str!`) and parse the `paths` array
//! dependency-free, exactly like the Rust parity harness does, so this
//! rule can never disagree with the catalog: a path readCtx handles is in
//! the catalog (Go parity test), and the catalog is what this rule
//! accepts. Add a `readCtx` arm + a catalog entry together and this rule
//! recognizes the new path automatically.
//!
//! **Fires when** a `creates` / `updates` / `deletes` binding's RHS (a SET
//! assignment OR an authored `where <col> = <expr>` row), or a command
//! `let` binding's value, is a `Path` whose head is `ctx` and whose tail
//! (the segments after `ctx`) is NOT a catalog entry. Example fixtures
//! that fire: `owner = ctx.actor.bogus`, `created_at = ctx.nonexistent`.
//! FnCall arguments are recursed (`number = @fn.next(ctx.actor.bogus)`
//! fires on the inner ctx path).
//!
//! **Does not fire** for a recognized ctx path (`ctx.now`, `ctx.user.id`,
//! `ctx.actor.id`, `ctx.actor.org_id`, `ctx.actor.tenant_id`, …), or for
//! any non-`ctx` head (`input.x`, `route.id`, `target.y`, a bare `let`
//! name) — those are governed by `CODEGEN-UNRESOLVED-BINDING-SOURCE-001`
//! (head resolution), which is this rule's sibling. This rule governs
//! ONLY the `ctx` namespace tail; the `input`/`route`/`target` namespaces
//! have their own runtime resolvers and are out of scope here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use lazuli_ir::{Assignment, Command, CommandEffect, Expr, Feature};

// catalog

/// The ctx-path catalog JSON, embedded at compile time from the SoT file
/// that both face-parity tests gate against. Embedding (rather than a
/// runtime read) keeps the rule self-contained — the doctor runs against
/// pilot directories, not the framework workspace, so there is no
/// `runtime/go/lazuli/` tree to read at doctor time.
const CTX_PATH_CATALOG_JSON: &str =
    include_str!("../../../../runtime/go/lazuli/ctx_path_catalog.json");

/// Parse the `paths` array out of the embedded catalog JSON. Dependency-
/// free (the crate has no serde in scope here): slice the
/// `"paths": [ ... ]` array and pull every quoted literal. Mirrors the
/// parser in `crates/lazuli_codegen_go/tests/ctx_path_parity.rs` byte for
/// byte so the two faces read the file identically. The `_comment` field
/// is outside the array and never read.
fn parse_catalog(raw: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(start) = raw.find("\"paths\"") else {
        return out;
    };
    let Some(arr_start) = raw[start..].find('[').map(|i| start + i) else {
        return out;
    };
    let Some(arr_end) = raw[arr_start..].find(']').map(|i| arr_start + i) else {
        return out;
    };
    let body = &raw[arr_start + 1..arr_end];
    for piece in body.split(',') {
        let piece = piece.trim();
        if let Some(stripped) = piece.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            out.insert(stripped.to_owned());
        }
    }
    out
}

/// The recognized ctx-path set, parsed once from the embedded catalog.
fn catalog() -> &'static BTreeSet<String> {
    static CATALOG: OnceLock<BTreeSet<String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let set = parse_catalog(CTX_PATH_CATALOG_JSON);
        debug_assert!(
            !set.is_empty(),
            "ctx_path_catalog.json parsed to zero paths — the embedded SoT or the \
             parser is broken"
        );
        set
    })
}

// output

/// One CTX-PATH-UNRESOLVED-001 finding: an author-written `ctx.<tail>`
/// binding path whose tail is not a recognized ctx slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub command: String,
    /// The binding column (the assignment `field`, lowercased the same way
    /// codegen lowercases it). `let <name>` bindings report the `let` name.
    pub column: String,
    /// The unrecognized ctx tail as the author wrote it after `ctx.`
    /// (`actor.bogus`, `nonexistent`, …).
    pub tail: String,
    /// `set` for a SET assignment, `where` for an authored where-clause
    /// binding, `let` for a `let` binding value — so the message names
    /// which axis tripped.
    pub axis: &'static str,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "CTX-PATH-UNRESOLVED-001";

    /// Render the "unresolved ctx path" message — name the command, the
    /// column, the offending tail, and the recognized catalog set so the
    /// author can see exactly which ctx slots resolve.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::ctx_path_unresolved_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     command: "create_account".into(),
    ///     column: "owner".into(),
    ///     tail: "actor.bogus".into(),
    ///     axis: "set",
    /// };
    /// assert!(f.message().contains("actor.bogus"));
    /// assert!(f.message().contains("ctx.now"));
    /// ```
    pub fn message(&self) -> String {
        let known = catalog()
            .iter()
            .map(|p| format!("ctx.{p}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "command `{}` {} binding for `{}` reads `ctx.{}`, but `{}` is not a \
             known ctx path — it would lower to `lazuli.FromCtx(\"{}\")` and 500 at \
             runtime with `unknown ctx path: {}`. Known ctx paths: {}.",
            self.command, self.axis, self.column, self.tail, self.tail, self.tail, self.tail,
            known,
        )
    }
}

// detection

/// Run CTX-PATH-UNRESOLVED-001 for all commands in one feature.
///
/// `path` anchors findings; no I/O is performed here (the catalog is
/// embedded at compile time).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::ctx_path_unresolved_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with commands");
/// let _ = check(&feature, Path::new("users.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();

    for command in &feature.commands {
        // `let <name> = ctx.<tail>` bindings — same author-controlled ctx
        // surface as effect bindings (they lower through the same
        // `format_binding_source` path when referenced).
        for binding in &command.lets {
            collect_unresolved(&binding.value, command, &binding.name, "let", path, &mut out);
        }

        match &command.effect {
            CommandEffect::Creates(create) => {
                check_assignments(command, &create.assignments, "set", path, &mut out);
            }
            CommandEffect::Updates(update) => {
                check_assignments(command, &update.assignments, "set", path, &mut out);
                check_assignments(command, &update.where_clause, "where", path, &mut out);
            }
            CommandEffect::Deletes(delete) => {
                check_assignments(command, &delete.where_clause, "where", path, &mut out);
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
    path: &Path,
    out: &mut Vec<Finding>,
) {
    for assignment in assignments {
        let column = assignment.field.to_ascii_lowercase();
        collect_unresolved(&assignment.value, command, &column, axis, path, out);
    }
}

/// Walk one binding RHS, pushing a finding for every `ctx.<tail>` path
/// whose tail is not a catalog entry. FnCall arguments are recursed so a
/// ctx path buried in an `@fn.x(ctx.actor.bogus)` call is still caught.
fn collect_unresolved(
    expr: &Expr,
    command: &Command,
    column: &str,
    axis: &'static str,
    path: &Path,
    out: &mut Vec<Finding>,
) {
    match expr {
        Expr::Path(p) => {
            if let Some(tail) = unresolved_ctx_tail(&p.segments) {
                out.push(Finding {
                    path: path.to_path_buf(),
                    command: command.name.clone(),
                    column: column.to_owned(),
                    tail,
                    axis,
                });
            }
        }
        Expr::FnCall(call) => {
            for arg in &call.args {
                collect_unresolved(arg, command, column, axis, path, out);
            }
        }
        // Literals carry no ctx path.
        Expr::String(_)
        | Expr::Integer(_)
        | Expr::Boolean(_)
        | Expr::Enum(_)
        | Expr::Nil => {}
    }
}

/// Return `Some(tail)` when `segments` is a `ctx`-headed path whose tail
/// is NOT in the catalog; `None` for a non-`ctx` head, a bare `ctx` with
/// no tail, or a recognized tail.
///
/// Mirrors the codegen `"ctx" =>` arm: the lowered `FromCtx` argument is
/// exactly `segments[1..].join(".")`, so "the rule passes" ⟺ "readCtx has
/// an arm for the emitted path" (both gated by the same catalog).
fn unresolved_ctx_tail(segments: &[String]) -> Option<String> {
    if segments.first().map(|s| s.as_str()) != Some("ctx") {
        return None;
    }
    if segments.len() < 2 {
        // A bare `ctx` with no tail is not a binding-source the codegen
        // lowers; leave it to other passes rather than inventing a tail.
        return None;
    }
    let tail = segments[1..].join(".");
    if catalog().contains(&tail) {
        return None;
    }
    Some(tail)
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        DeleteEffect, FnCallExpr, LetBinding, Path as IrPath, Policies, PolicyRef, QualifiedName,
        UpdateEffect,
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

    // catalog sanity — the embedded SoT really parsed.

    #[test]
    fn catalog_loads_the_known_paths() {
        let cat = catalog();
        for p in ["now", "user.id", "actor.id", "actor.org_id", "actor.tenant_id"] {
            assert!(cat.contains(p), "catalog should contain {p}: {cat:?}");
        }
        assert!(!cat.contains("actor.bogus"));
    }

    // (a) unknown ctx tail → ERRORS, naming column + tail.

    #[test]
    fn positive_unknown_ctx_tail_in_creates_fires() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign("owner", path_expr(&["ctx", "actor", "bogus"]))]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "CTX-PATH-UNRESOLVED-001");
        assert_eq!(findings[0].command, "create_account");
        assert_eq!(findings[0].column, "owner");
        assert_eq!(findings[0].tail, "actor.bogus");
        assert_eq!(findings[0].axis, "set");
        assert!(findings[0].message().contains("actor.bogus"));
        // The message lists the recognized set for the author.
        assert!(findings[0].message().contains("ctx.now"));
    }

    #[test]
    fn positive_single_segment_unknown_tail_fires() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign("created_at", path_expr(&["ctx", "nonexistent"]))]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tail, "nonexistent");
    }

    #[test]
    fn positive_unknown_ctx_tail_in_where_fires() {
        let cmd = mk_cmd(
            "deactivate",
            updates(
                vec![assign("status", Expr::String("inactive".into()))],
                vec![assign("id", path_expr(&["ctx", "actor", "wrong"]))],
            ),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].axis, "where");
        assert_eq!(findings[0].tail, "actor.wrong");
    }

    #[test]
    fn positive_unknown_ctx_tail_in_delete_where_fires() {
        let cmd = mk_cmd(
            "purge",
            deletes(vec![assign("id", path_expr(&["ctx", "nope"]))]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].axis, "where");
        assert_eq!(findings[0].tail, "nope");
    }

    #[test]
    fn positive_unknown_ctx_tail_in_let_fires() {
        let mut cmd = mk_cmd(
            "create_account",
            creates(vec![assign("owner", path_expr(&["ctx", "user", "id"]))]),
        );
        cmd.lets = vec![LetBinding {
            name: "tid".into(),
            value: path_expr(&["ctx", "actor", "missing"]),
        }];
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].axis, "let");
        assert_eq!(findings[0].column, "tid");
        assert_eq!(findings[0].tail, "actor.missing");
    }

    #[test]
    fn positive_unknown_ctx_tail_inside_fn_arg_fires() {
        // `number = @fn.next_invoice_number(ctx.actor.bogus)` — the inner
        // ctx path is still lowered to FromCtx, so it must be caught.
        let cmd = mk_cmd(
            "create_account",
            creates(vec![assign(
                "number",
                Expr::FnCall(FnCallExpr {
                    name: qn("next_invoice_number"),
                    args: vec![path_expr(&["ctx", "actor", "bogus"])],
                }),
            )]),
        );
        let findings = check(&mk_feature(cmd), Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tail, "actor.bogus");
    }

    // (b) recognized ctx paths → no error (no false-positive on pilots).

    #[test]
    fn negative_known_ctx_paths_do_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![
                assign("created_at", path_expr(&["ctx", "now"])),
                assign("owner", path_expr(&["ctx", "user"])),
                assign("owner_id", path_expr(&["ctx", "user", "id"])),
                assign("org", path_expr(&["ctx", "user", "org"])),
                assign("actor_id", path_expr(&["ctx", "actor", "id"])),
                assign("org_id", path_expr(&["ctx", "actor", "org_id"])),
                assign("tenant_id", path_expr(&["ctx", "actor", "tenant_id"])),
            ]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    // (c) non-ctx heads → unaffected (governed by the sibling
    // CODEGEN-UNRESOLVED-BINDING-SOURCE-001 rule, not this one).

    #[test]
    fn negative_non_ctx_heads_do_not_fire() {
        let cmd = mk_cmd(
            "create_account",
            creates(vec![
                assign("name", path_expr(&["input", "name"])),
                assign("id", path_expr(&["route", "id"])),
                assign("prev", path_expr(&["target", "status"])),
                // An unknown non-ctx head (`foo.bar`) is the sibling rule's
                // job; this rule only governs the ctx namespace.
                assign("misc", path_expr(&["foo", "bar"])),
                assign("role", Expr::String("MEMBER".into())),
            ]),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_returns_command_does_not_fire() {
        let cmd = mk_cmd(
            "noop",
            CommandEffect::Returns(lazuli_ir::ReturnsEffect {
                return_type: lazuli_ir::TypeRef::Builtin(BuiltinType::Boolean),
            }),
        );
        assert!(check(&mk_feature(cmd), Path::new("billing.lzi")).is_empty());
    }
}
