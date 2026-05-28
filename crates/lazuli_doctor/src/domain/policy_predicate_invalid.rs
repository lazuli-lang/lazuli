//! POLICY-PREDICATE-001 — an input-value-predicate policy atom
//! (`@policy.admin when input.scope = "production"`) references an
//! `input.*` field the consuming command does not declare, OR the
//! guarded atom is not a known `@policy` / `@role` / `@scope` / `@actor`
//! atom.
//!
//! GAP-09. The `when` predicate is a closed predicate over the consuming
//! command's `input.*` fields. Fires when:
//!  - a `when` predicate path head (after the `input.` prefix) is not a
//!    declared input slot of any command that references the category, OR
//!  - the predicate could not be lowered to a closed comparison
//!    (`Unparsed`), OR
//!  - the guarded atom's namespace is outside the closed catalog
//!    (`policy` / `role` / `scope` / `actor`).
//!
//! Severity: `error`. A predicate that can't bind would silently never
//! apply (Rule Zero — vocabulary, not silent no-op). The predicate
//! field-reference walk mirrors `constraint_unique_when_invalid` so the
//! two checks stay aligned.
//!
//! Resolution model: a policy category named `<name>` is consumed by any
//! command whose `policy` resolves to `@policy.<name>`. The predicate
//! `input.*` fields are checked against that command's declared input.
//! A category with conditional atoms that no command references surfaces
//! the predicate fields against the empty input set (every ref is then
//! unresolved) so a dangling predicate is never silently dropped.
//!
//! Reference: GAP-09 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    Command, CommandInput, ConditionalPolicyAtom, EvalPredicate, Expr, Feature, Path as IrPath,
    PolicyRef, Predicate,
};

/// Closed catalog of admissible namespaces for a predicate-gated atom.
/// Mirrors the legacy OR-atom catalog the runtime evaluator understands.
const KNOWN_ATOM_NAMESPACES: &[&str] = &["policy", "role", "scope", "actor"];

/// One POLICY-PREDICATE-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the feature.
    pub path: PathBuf,
    /// Feature containing the policy category.
    pub feature: String,
    /// Policy category whose conditional atom is invalid.
    pub category: String,
    /// The guarded atom literal.
    pub atom: String,
    /// What's wrong — either an unresolved `input.*` field or a
    /// disallowed atom namespace.
    pub kind: FindingKind,
}

/// The two POLICY-PREDICATE-001 failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// The `when` predicate references an `input.<field>` that no command
    /// consuming the category declares (or the predicate is unparseable).
    UnknownInputField(String),
    /// The guarded atom's namespace is outside the closed catalog.
    UnknownAtom,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "POLICY-PREDICATE-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::policy_predicate_invalid::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("account.lzi"),
    ///     feature: "account".into(),
    ///     category: "create".into(),
    ///     atom: "@policy.admin".into(),
    ///     kind: FindingKind::UnknownInputField("ghost".into()),
    /// };
    /// assert!(f.message().contains("create"));
    /// assert!(f.message().contains("ghost"));
    /// ```
    pub fn message(&self) -> String {
        match &self.kind {
            FindingKind::UnknownInputField(field) => format!(
                "policy `{}` atom `{}` has a `when` predicate referencing unknown \
                 input field `{}` (not declared on any command using this policy). \
                 Reference a declared `input.*` field.",
                self.category, self.atom, field
            ),
            FindingKind::UnknownAtom => format!(
                "policy `{}` predicate-gated atom `{}` is not a known \
                 `@policy` / `@role` / `@scope` / `@actor` atom.",
                self.category, self.atom
            ),
        }
    }
}

/// Run POLICY-PREDICATE-001 over one feature.
///
/// For each policy category carrying conditional (`when`) atoms, resolve
/// the union of input fields across every command that references the
/// category, then walk each atom's `when` predicate paths against that
/// set with the same closed-predicate collector as
/// `constraint_unique_when_invalid`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::policy_predicate_invalid::check;
///
/// let findings = check(&feature, Path::new("account.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for category in &feature.policies.categories {
        if category.conditional_atoms.is_empty() {
            continue;
        }
        // Union of input fields across all commands that use this category.
        let known_inputs = input_fields_for_category(feature, &category.name);

        for ca in &category.conditional_atoms {
            check_atom(
                ca,
                &category.name,
                &known_inputs,
                feature,
                path,
                &mut findings,
            );
        }
    }

    findings
}

/// Validate one conditional atom: namespace catalog + predicate refs.
fn check_atom(
    ca: &ConditionalPolicyAtom,
    category: &str,
    known_inputs: &HashSet<String>,
    feature: &Feature,
    path: &Path,
    out: &mut Vec<Finding>,
) {
    // 1. Atom namespace must be in the closed catalog.
    if !atom_namespace_known(&ca.atom) {
        out.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            category: category.to_owned(),
            atom: ca.atom.clone(),
            kind: FindingKind::UnknownAtom,
        });
    }

    // 2. Predicate `input.<field>` heads must resolve to a declared input.
    for field in unresolved_input_fields(&ca.when, known_inputs) {
        out.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            category: category.to_owned(),
            atom: ca.atom.clone(),
            kind: FindingKind::UnknownInputField(field),
        });
    }
}

/// `@policy.x` / `@role.y` / `@scope.z` / `@actor.w` → known. Strips the
/// leading `@`, takes the namespace segment before the first `.`.
fn atom_namespace_known(atom: &str) -> bool {
    let stripped = atom.strip_prefix('@').unwrap_or(atom);
    let ns = stripped.split('.').next().unwrap_or("");
    KNOWN_ATOM_NAMESPACES.contains(&ns)
}

/// Collect the declared input field names across every command that
/// references the named policy category. Commands resolve a category via
/// `PolicyRef::Local("<name>")` or `PolicyRef::Atom("policy.<name>")`.
fn input_fields_for_category(feature: &Feature, category: &str) -> HashSet<String> {
    let mut fields = HashSet::new();
    for command in &feature.commands {
        if command_uses_category(command, category) {
            collect_input_fields(&command.input, &mut fields);
        }
    }
    fields
}

/// Whether a command's resolved policy points at the named feature-local
/// category.
fn command_uses_category(command: &Command, category: &str) -> bool {
    match &command.policy {
        PolicyRef::Local(name) => name == category,
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            stripped
                .strip_prefix("policy.")
                .is_some_and(|local| local == category)
        }
        _ => false,
    }
}

/// Push every declared input slot name onto `out`. `Short` lists and
/// `Typed` blocks both contribute names; `Empty` contributes nothing.
fn collect_input_fields(input: &CommandInput, out: &mut HashSet<String>) {
    match input {
        CommandInput::Short(names) => out.extend(names.iter().cloned()),
        CommandInput::Typed(slots) => out.extend(slots.iter().map(|s| s.name.clone())),
        CommandInput::Empty => {}
    }
}

/// Collect predicate path heads that don't resolve to a declared input.
/// The path's leading `input.` segment is stripped before the head is
/// compared; an `Unparsed` predicate surfaces a `<unparseable: ...>`
/// sentinel so the author gets a hint instead of a silent skip. Mirrors
/// `constraint_unique_when_invalid::unresolved_top_fields`.
fn unresolved_input_fields(pred: &EvalPredicate, known: &HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    match pred {
        EvalPredicate::Closed(inner) => collect_pred_paths(inner, known, &mut out),
        EvalPredicate::Contains { lhs, .. } => check_path(lhs, known, &mut out),
        EvalPredicate::ToolsCalls { .. } => {}
        EvalPredicate::Unparsed(text) => out.push(format!("<unparseable: {}>", text.trim())),
    }
    out
}

fn collect_pred_paths(pred: &Predicate, known: &HashSet<String>, out: &mut Vec<String>) {
    match pred {
        Predicate::Comparison { left, right, .. } => {
            check_expr(left, known, out);
            check_expr(right, known, out);
        }
        Predicate::Has {
            collection,
            element,
        } => {
            check_expr(collection, known, out);
            check_expr(element, known, out);
        }
        Predicate::And(parts) | Predicate::Or(parts) => {
            for part in parts {
                collect_pred_paths(part, known, out);
            }
        }
    }
}

fn check_expr(expr: &Expr, known: &HashSet<String>, out: &mut Vec<String>) {
    if let Expr::Path(p) = expr {
        check_path(p, known, out);
    }
}

/// A predicate path is `input.<field>[...]`. The head we validate is the
/// first segment after `input`. A bare path with no `input.` prefix (a
/// literal-looking path the parser kept as a path, e.g. a misspelled
/// keyword) is treated as a reference to that head directly. String /
/// integer / boolean RHS literals never reach here (they aren't paths).
fn check_path(p: &IrPath, known: &HashSet<String>, out: &mut Vec<String>) {
    let mut segs = p.segments.iter();
    let Some(first) = segs.next() else {
        return;
    };
    // The field head is the segment after `input`, else the first segment.
    let head = if first == "input" {
        match segs.next() {
            Some(field) => field.as_str(),
            // Bare `input` with no field — nothing to validate.
            None => return,
        }
    } else {
        first.as_str()
    };
    if known.contains(head) {
        return;
    }
    out.push(head.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CommandEffect, CommandInput, CommandKind, CompareOp, ConditionalPolicyAtom,
        Defaults, EvalPredicate, Expr, Feature, Path as IrPath, Policies, PolicyCategory, PolicyRef,
        Predicate, TypeRef, TypedSlot,
    };

    fn scope_pred(value: &str) -> EvalPredicate {
        EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments(["input", "scope"])),
            op: CompareOp::Eq,
            right: Expr::String(value.into()),
        })
    }

    fn typed_slot(name: &str) -> TypedSlot {
        TypedSlot {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            constraints: Default::default(),
            validate_skip: false,
        }
    }

    fn mk_command(name: &str, policy: PolicyRef, inputs: &[&str]) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Typed(inputs.iter().map(|n| typed_slot(n)).collect()),
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
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(categories: Vec<PolicyCategory>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "account".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies {
                categories,
                fields: vec![],
                span_ref: None,
            },
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

    fn cat(name: &str, conditional: Vec<ConditionalPolicyAtom>) -> PolicyCategory {
        PolicyCategory {
            name: name.into(),
            atoms: vec![],
            conditional_atoms: conditional,
            previous_names: vec![],
            when_denied: None,
            when_denied_route: None,
        }
    }

    #[test]
    fn negative_known_input_field_passes() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_unknown_input_field_fires() {
        // Predicate references `input.ghost`; command declares only `scope`.
        let ghost_pred = EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments(["input", "ghost"])),
            op: CompareOp::Eq,
            right: Expr::String("x".into()),
        });
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: ghost_pred,
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].kind,
            FindingKind::UnknownInputField("ghost".into())
        );
        assert_eq!(Finding::CODE, "POLICY-PREDICATE-001");
    }

    #[test]
    fn positive_unknown_atom_namespace_fires() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@bogus.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::UnknownAtom);
    }

    #[test]
    fn unparseable_predicate_surfaces_sentinel() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: EvalPredicate::Unparsed("scope and weird".into()),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        match &findings[0].kind {
            FindingKind::UnknownInputField(f) => assert!(f.contains("unparseable")),
            other => panic!("expected unparseable sentinel, got {other:?}"),
        }
    }

    #[test]
    fn unconditional_category_is_ignored() {
        // No conditional atoms → not in scope, even with no command.
        let feature = mk_feature(
            vec![PolicyCategory {
                name: "create".into(),
                atoms: vec!["@role.admin".into()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            vec![],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn atom_resolves_via_policy_dot_atom_ref() {
        // Command references the category via PolicyRef::Atom("policy.create").
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@role.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Atom("policy.create".into()),
                &["scope"],
            )],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
