//! VOCAB-EVENT-PRODUCER-001 — mutating command without IR-visible emits.
//!
//! Fires when a `command` mutates a resource with matching feature-level events
//! but declares no `emits` clause. The likely symptom is handler-side event
//! emission that the IR cannot see, so audit and projection tooling drift from
//! runtime behavior.

use std::path::{Path, PathBuf};

use lazuli_ir::{AuditSpec, Command, CommandEffect, Feature};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-EVENT-PRODUCER-001 finding: a mutating command omits `emits` even
/// though the feature declares plausible events for the affected resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending command.
    pub command: String,
    /// Matching feature-level event names.
    pub candidate_events: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-EVENT-PRODUCER-001";

    pub fn message(&self) -> String {
        format!(
            "command `{}` mutates a resource for which event(s) {:?} exist, \
             but declares no `emits` clause. If the handler emits events \
             out-of-band, the IR can't see them and audit / projections drift. \
             Add `emits <event>` from creates/updates/deletes.",
            self.command, self.candidate_events
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-EVENT-PRODUCER-001 for all commands in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here. The caller maps each `Finding` into a `DoctorDiagnostic` and
/// supplies exact source locations from the syntax facts.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let event_names: Vec<&str> = feature.events.iter().map(|ev| ev.name.as_str()).collect();

    feature
        .commands
        .iter()
        .filter(|cmd| cmd.emits.is_empty())
        .filter(|cmd| !is_read_named(cmd))
        .filter_map(|cmd| {
            let resource = mutated_resource(cmd)?;

            if is_audit_none_create(cmd) {
                return None;
            }

            let resource_lower = resource.to_ascii_lowercase();
            let command_lower = cmd.name.to_ascii_lowercase();
            let candidates: Vec<String> = event_names
                .iter()
                .copied()
                .filter(|event_name| {
                    name_matches_prefix(event_name, &resource_lower)
                        || name_matches_prefix(event_name, &command_lower)
                })
                .map(str::to_owned)
                .collect();

            (!candidates.is_empty()).then_some((cmd, candidates))
        })
        .map(|(cmd, candidate_events)| Finding {
            path: path.to_path_buf(),
            command: cmd.name.clone(),
            candidate_events,
        })
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

fn mutated_resource(cmd: &Command) -> Option<&str> {
    match &cmd.effect {
        CommandEffect::Creates(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Updates(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Deletes(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

fn name_matches_prefix(event_name: &str, prefix: &str) -> bool {
    let event_lower = event_name.to_ascii_lowercase();
    event_lower == prefix || event_lower.starts_with(&format!("{prefix}."))
}

fn is_read_named(cmd: &Command) -> bool {
    let name = cmd.name.to_ascii_lowercase();
    name.starts_with("get_") || name.starts_with("find_") || name.starts_with("list_")
}

fn is_audit_none_create(cmd: &Command) -> bool {
    matches!(cmd.effect, CommandEffect::Creates(_))
        && cmd.audit.as_ref().map(is_audit_none).unwrap_or(false)
}

fn is_audit_none(audit: &AuditSpec) -> bool {
    audit.subjects.len() == 1 && audit.subjects[0] == "none"
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AuditSpec, BuiltinType, Command, CommandInput, CommandKind, CreateEffect, Defaults, Event,
        EventField, EventKind, Feature, Policies, PolicyRef, QualifiedName, ReturnsEffect, TypeRef,
        UpdateEffect,
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
        emits: Vec<&str>,
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
            kind,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect,
            policy: PolicyRef::None,
            policy_expr: None,
            emits: emits.into_iter().map(str::to_owned).collect(),
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
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_event(name: &str) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload: vec![EventField {
                name: "post".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                optional: false,
            }],
            payload_none: false,
            level: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(commands: Vec<Command>, events: Vec<Event>) -> Feature {
        Feature {
            name: "post".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events,
            rules: vec![],
            policies: Policies::default(),
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn updates_post() -> CommandEffect {
        updates_resource("Post")
    }

    fn updates_resource(resource: &str) -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn(resource),
            assignments: vec![],
        })
    }

    fn creates_post() -> CommandEffect {
        CommandEffect::Creates(CreateEffect {
            resource: qn("Post"),
            from_input: false,
            assignments: vec![],
        })
    }

    fn returns_bool() -> CommandEffect {
        CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Builtin(BuiltinType::Boolean),
        })
    }

    fn audit_none() -> Option<AuditSpec> {
        Some(AuditSpec {
            subjects: vec!["none".to_owned()],
            emit_to: None,
        })
    }

    #[test]
    fn positive_update_command_no_emits_with_candidate_event_fires() {
        let cmd = mk_cmd("archive_post", updates_post(), vec![], None);
        let feature = mk_feature(vec![cmd], vec![mk_event("post.archived")]);

        let findings = check(&feature, Path::new("features/post/post.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].command, "archive_post");
        assert_eq!(findings[0].candidate_events, vec!["post.archived"]);
        assert_eq!(Finding::CODE, "VOCAB-EVENT-PRODUCER-001");
        assert!(findings[0].message().contains("archive_post"));
    }

    #[test]
    fn negative_command_with_emits_does_not_fire() {
        let cmd = mk_cmd("archive_post", updates_post(), vec!["post.archived"], None);
        let feature = mk_feature(vec![cmd], vec![mk_event("post.archived")]);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_resource_without_events_does_not_fire() {
        let cmd = mk_cmd("archive_note", updates_resource("Note"), vec![], None);
        let feature = mk_feature(vec![cmd], vec![mk_event("comment.archived")]);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_returns_command_does_not_fire() {
        let cmd = mk_cmd("archive_post", returns_bool(), vec![], None);
        let feature = mk_feature(vec![cmd], vec![mk_event("post.archived")]);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_audit_none_command_does_not_fire() {
        let cmd = mk_cmd("create_post", creates_post(), vec![], audit_none());
        let feature = mk_feature(vec![cmd], vec![mk_event("post.created")]);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
