//! RESOURCE-APPEND-ONLY-001 — a `command` updates or deletes a resource
//! declared `append_only`.
//!
//! W4 GAP-AUDIT-02. `append_only` marks a resource as insert-only (an
//! audit-log / ledger discipline): rows may be created but never mutated or
//! removed. This rule fires when a command's effect is `updates <R>` or
//! `deletes <R>` and `<R>` carries the `append_only` modifier on the same
//! feature.
//!
//! Severity: `error`. An `updates`/`deletes` against an append-only
//! resource is a contradiction the author must resolve (drop the modifier
//! or drop the command) — Rule Zero (vocabulary, not silent override).
//! Cross-construct (command → resource) walk, mirroring the W2 cross-feature
//! checks.
//!
//! Reference: GAP-AUDIT-02 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{CommandEffect, Feature};

/// Which write verb a command applied to the append-only resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// `updates <append_only resource>`.
    Update,
    /// `deletes <append_only resource>`.
    Delete,
}

impl Verb {
    fn as_str(self) -> &'static str {
        match self {
            Verb::Update => "updates",
            Verb::Delete => "deletes",
        }
    }
}

/// One RESOURCE-APPEND-ONLY-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the command.
    pub path: PathBuf,
    /// Feature containing the command + resource.
    pub feature: String,
    /// Command whose effect targets the append-only resource.
    pub command: String,
    /// The append-only resource the command writes against.
    pub resource: String,
    /// Whether the command updates or deletes the resource.
    pub verb: Verb,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "RESOURCE-APPEND-ONLY-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::resource_append_only_invalid::{Finding, Verb};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("ledger.lzi"),
    ///     feature: "ledger".into(),
    ///     command: "edit_entry".into(),
    ///     resource: "Entry".into(),
    ///     verb: Verb::Update,
    /// };
    /// assert!(f.message().contains("Entry"));
    /// assert!(f.message().contains("append_only"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command `{}` {} resource `{}`, which is declared `append_only` \
             (insert-only). Remove the `append_only` modifier or drop the \
             `{}` command — append-only resources accept inserts only.",
            self.command,
            self.verb.as_str(),
            self.resource,
            self.command,
        )
    }
}

/// Run RESOURCE-APPEND-ONLY-001 over one feature.
///
/// Builds the set of same-feature `append_only` resource names, then walks
/// each command's effect: an `updates`/`deletes` whose target resource is in
/// the set produces a finding.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::resource_append_only_invalid::check;
///
/// let findings = check(&feature, Path::new("ledger.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let append_only: HashSet<&str> = feature
        .resources
        .iter()
        .filter(|r| r.append_only)
        .map(|r| r.name.as_str())
        .collect();
    if append_only.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for command in &feature.commands {
        let (target, verb) = match &command.effect {
            CommandEffect::Updates(e) => (&e.resource, Verb::Update),
            CommandEffect::Deletes(e) => (&e.resource, Verb::Delete),
            _ => continue,
        };
        // The effect resource is feature-local (commands write their own
        // feature's resources); an explicit cross-feature `feature` is not
        // the append-only owner here, so only match unqualified / same-feature.
        if let Some(feat) = target.feature.as_deref()
            && feat != feature.name
        {
            continue;
        }
        if append_only.contains(target.name.as_str()) {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                command: command.name.clone(),
                resource: target.name.clone(),
                verb,
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandInput, CommandKind, Defaults, DeleteEffect, Policies, PolicyRef,
        QualifiedName, Resource, UpdateEffect,
    };

    fn mk_resource(name: &str, append_only: bool) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
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
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only,
            many_through: Vec::new(),
        }
    }

    fn mk_command(name: &str, effect: CommandEffect) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Update,
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
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn local(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "ledger".into(),
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
    fn negative_update_on_append_only_fires() {
        let resource = mk_resource("Entry", true);
        let cmd = mk_command(
            "edit_entry",
            CommandEffect::Updates(UpdateEffect {
                resource: local("Entry"),
                assignments: vec![],
            }),
        );
        let feature = mk_feature(vec![resource], vec![cmd]);
        let findings = check(&feature, Path::new("ledger.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verb, Verb::Update);
        assert_eq!(findings[0].resource, "Entry");
        assert_eq!(Finding::CODE, "RESOURCE-APPEND-ONLY-001");
    }

    #[test]
    fn negative_delete_on_append_only_fires() {
        let resource = mk_resource("Entry", true);
        let cmd = mk_command(
            "remove_entry",
            CommandEffect::Deletes(DeleteEffect {
                resource: local("Entry"),
            }),
        );
        let feature = mk_feature(vec![resource], vec![cmd]);
        let findings = check(&feature, Path::new("ledger.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verb, Verb::Delete);
    }

    #[test]
    fn positive_update_on_non_append_only_passes() {
        let resource = mk_resource("Entry", false);
        let cmd = mk_command(
            "edit_entry",
            CommandEffect::Updates(UpdateEffect {
                resource: local("Entry"),
                assignments: vec![],
            }),
        );
        let feature = mk_feature(vec![resource], vec![cmd]);
        assert!(check(&feature, Path::new("ledger.lzi")).is_empty());
    }

    #[test]
    fn positive_create_on_append_only_passes() {
        // A create against an append-only resource is fine — only
        // updates/deletes are rejected. We model "create" here as an
        // effect that is neither Updates nor Deletes.
        let resource = mk_resource("Entry", true);
        let cmd = mk_command("add_entry", CommandEffect::None);
        let feature = mk_feature(vec![resource], vec![cmd]);
        assert!(check(&feature, Path::new("ledger.lzi")).is_empty());
    }
}
