//! VOCAB-AUDIT-001 — mutating command without an explicit `audit` child.
//!
//! Fires when a `command.*` block carries a write effect (`creates`, `updates`,
//! `deletes`) or emits events but does NOT declare any of the three audit forms:
//!   `audit default`           — log all default fields
//!   `audit <field>, <field>`  — log specific fields
//!   `audit none`              — explicit opt-out (with a reason in comments)
//!
//! Severity: `warning` (strict-profile), `error` (production-profile).
//! Reference: docs/proposals/doctor-vocabulary-lints.md §VOCAB-AUDIT-001
//! Invariant: docs/invariants.md:93-97

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, Feature};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-AUDIT-001 finding: a mutating command with no `audit` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending command.
    pub command: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-AUDIT-001";

    /// Render the user-facing diagnostic body prompting the author to
    /// add one of the three `audit` forms (default / fields / none).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_audit_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("f.lzi"), command: "update_x".into() };
    /// assert!(f.message().contains("audit default"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command `{}` has a write effect or emits events but declares no `audit` child \
             — add `audit default`, `audit <fields>`, or `audit none` (with a reason). \
             Audit declarations give compliance tooling a typed contract instead of \
             relying on event-name conventions. See docs/invariants.md:93-97.",
            self.command
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-AUDIT-001 for all commands in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.  The caller (doctor walker) maps each `Finding` into a
/// `DoctorDiagnostic` and supplies the exact source line from
/// `Tier3FeatureFacts.command_lines`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_audit_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with mutating commands");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .commands
        .iter()
        .filter(|cmd| is_mutating(cmd) && cmd.audit.is_none())
        .map(|cmd| Finding {
            path: path.to_path_buf(),
            command: cmd.name.clone(),
        })
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

/// A command is mutating when its effect is `creates`, `updates`, or `deletes`,
/// or when it declares at least one `emits` target.  Pure `returns` commands are
/// reads and are not subject to this rule.  `CommandEffect::None` (legacy path)
/// is treated as non-mutating unless `emits` is populated.
fn is_mutating(cmd: &Command) -> bool {
    matches!(
        cmd.effect,
        CommandEffect::Creates(_) | CommandEffect::Updates(_) | CommandEffect::Deletes(_)
    ) || !cmd.emits.is_empty()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AuditSpec, BuiltinType, Command, CommandEffect, CommandInput, CommandKind,
        CreateEffect, Defaults, DeleteEffect, Feature, Policies, PolicyRef,
        QualifiedName, ReturnsEffect, TypeRef, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_cmd(
        name: &str,
        effect: CommandEffect,
        emits: Vec<String>,
        audit: Option<AuditSpec>,
    ) -> Command {
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
            emits,
            rate_limit: None,
            audit,
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
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
            derived_from: None,
        }
    }

    fn mk_feature(commands: Vec<Command>) -> Feature {
        Feature {
            name: "test_feat".into(),
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
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn updates_effect() -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Publication"),
            assignments: vec![],
        })
    }

    fn creates_effect() -> CommandEffect {
        CommandEffect::Creates(CreateEffect {
            resource: qn("Publication"),
            from_input: false,
            assignments: vec![],
        })
    }

    fn deletes_effect() -> CommandEffect {
        CommandEffect::Deletes(DeleteEffect {
            resource: qn("Publication"),
        })
    }

    fn returns_effect() -> CommandEffect {
        CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Builtin(BuiltinType::Boolean),
        })
    }

    /// `audit default` — subjects: ["default"]
    fn audit_default() -> Option<AuditSpec> {
        Some(AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        })
    }

    /// `audit actor, input.status` — specific field list
    fn audit_fields() -> Option<AuditSpec> {
        Some(AuditSpec {
            subjects: vec!["actor".to_owned(), "input.status".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        })
    }

    /// `audit none` — explicit opt-out; subjects: ["none"]
    fn audit_none() -> Option<AuditSpec> {
        Some(AuditSpec {
            subjects: vec!["none".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        })
    }

    // ── positive ──────────────────────────────────────────────────────────────

    /// `updates` effect with no `audit` child — MUST fire.
    #[test]
    fn positive_updates_no_audit_fires() {
        let cmd = mk_cmd("update_status", updates_effect(), vec![], None);
        let feature = mk_feature(vec![cmd]);
        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));
        assert_eq!(findings.len(), 1, "expected one finding for updates+no-audit");
        assert_eq!(findings[0].command, "update_status");
        assert_eq!(Finding::CODE, "VOCAB-AUDIT-001");
        assert!(
            findings[0].message().contains("update_status"),
            "message should name the command"
        );
    }

    // ── negative (i): `audit default` — must NOT fire ─────────────────────────

    #[test]
    fn negative_i_audit_default_does_not_fire() {
        let cmd = mk_cmd("update_status", updates_effect(), vec![], audit_default());
        let feature = mk_feature(vec![cmd]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "command with `audit default` must not trigger VOCAB-AUDIT-001"
        );
    }

    // ── negative (ii): `audit <fields>` — must NOT fire ─────────────────────

    #[test]
    fn negative_ii_audit_fields_does_not_fire() {
        let cmd = mk_cmd("update_status", updates_effect(), vec![], audit_fields());
        let feature = mk_feature(vec![cmd]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "command with `audit actor, input.status` must not trigger VOCAB-AUDIT-001"
        );
    }

    // ── negative (iii): `audit none` — must NOT fire ─────────────────────────

    #[test]
    fn negative_iii_audit_none_does_not_fire() {
        let cmd = mk_cmd("archive_post", deletes_effect(), vec![], audit_none());
        let feature = mk_feature(vec![cmd]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "command with `audit none` must not trigger VOCAB-AUDIT-001"
        );
    }

    // ── negative (iv): query block — must NOT fire ────────────────────────────
    //
    // Queries live in `Feature.queries` (not `Feature.commands`) so they are
    // naturally excluded.  The closest in-scope equivalent is a `returns`
    // command with no `emits` — a pure compute / read result.

    #[test]
    fn negative_iv_returns_command_without_emits_does_not_fire() {
        let cmd = mk_cmd("check_eligibility", returns_effect(), vec![], None);
        let feature = mk_feature(vec![cmd]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "pure `returns` command with no emits should not trigger VOCAB-AUDIT-001"
        );
    }

    // ── additional coverage ───────────────────────────────────────────────────

    /// `creates` and `deletes` without audit also fire — one finding each.
    #[test]
    fn positive_creates_and_deletes_fire() {
        let feature = mk_feature(vec![
            mk_cmd("create_publication", creates_effect(), vec![], None),
            mk_cmd("delete_publication", deletes_effect(), vec![], None),
        ]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 2, "both creates and deletes without audit must fire");
    }

    /// A `returns` command that also emits is treated as mutating and fires.
    #[test]
    fn positive_returns_with_emits_fires() {
        let cmd = mk_cmd(
            "notify_admin",
            returns_effect(),
            vec!["admin.notified".to_owned()],
            None,
        );
        let feature = mk_feature(vec![cmd]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "`returns` command with emits and no audit must fire");
        assert_eq!(findings[0].command, "notify_admin");
    }

    /// Mixed bag: one with audit, one without — exactly one finding.
    #[test]
    fn mixed_only_undeclared_fires() {
        let feature = mk_feature(vec![
            mk_cmd("update_ok", updates_effect(), vec![], audit_default()),
            mk_cmd("update_bad", updates_effect(), vec![], None),
        ]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].command, "update_bad");
    }
}
