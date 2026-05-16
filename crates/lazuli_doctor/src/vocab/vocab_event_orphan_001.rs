//! VOCAB-EVENT-ORPHAN-001 - event declared but no command emits it.
//!
//! Inverse of VOCAB-EVENT-PAYLOAD-001: finds feature-level event declarations
//! that never materialise as emissions. Either the declaration is dead code OR
//! the command/job is missing an `emits` clause.
//!
//! Cross-feature emissions are intentionally out of scope for v1. A sibling
//! feature may emit this event, but this rule only checks the current feature
//! and lets the user disable the warning or move the emit when that is intended.
//!
//! Severity: `warning` (strict-profile), `warning` (production-profile).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{EventKind, Feature};

// -- output -------------------------------------------------------------------

/// One VOCAB-EVENT-ORPHAN-001 finding: a feature-level event declaration with
/// a typed payload and no same-feature command/job emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Event name as declared at feature level.
    pub event_name: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-EVENT-ORPHAN-001";

    pub fn message(&self) -> String {
        format!(
            "event `{}` is declared but no command or job in this feature \
             emits it - either remove the orphan declaration or attach \
             `emits {}` to the relevant command. Orphan events leak abstraction.",
            self.event_name, self.event_name
        )
    }
}

// -- detection ----------------------------------------------------------------

/// Run VOCAB-EVENT-ORPHAN-001 for one feature.
///
/// `payload none` is an explicit opt-out, so those events are silent. Trace
/// events are also silent because the IR documents them as outside the feature
/// reaction graph, for logs, audit streams, and external observers.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let emitted: HashSet<&str> = feature
        .commands
        .iter()
        .flat_map(|cmd| cmd.emits.iter())
        .chain(feature.jobs.iter().flat_map(|job| job.emits.iter()))
        .map(|event_ref| event_ref.rsplit('.').next().unwrap_or(event_ref.as_str()))
        .collect();

    feature
        .events
        .iter()
        .filter(|event| !event.payload_none)
        .filter(|event| is_internal_kind(&event.kind))
        .filter(|event| !emitted.contains(event.name.as_str()))
        .map(|event| Finding {
            path: path.to_path_buf(),
            event_name: event.name.clone(),
        })
        .collect()
}

fn is_internal_kind(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Domain)
}

// -- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{check, Finding};
    use std::path::Path;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        Event, EventField, EventKind, Feature, Policies, PolicyRef, QualifiedName, TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_cmd(name: &str, emits: Vec<&str>) -> Command {
        Command {
            name: name.to_owned(),
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: qn("Customer"),
                from_input: false,
                assignments: vec![],
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            emits: emits.iter().map(|s| s.to_string()).collect(),
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
                name: "customer_id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                optional: false,
            }],
            payload_none: false,
            level: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_trace_event(name: &str) -> Event {
        Event {
            kind: EventKind::Trace,
            ..mk_event(name)
        }
    }

    fn mk_payload_none_event(name: &str) -> Event {
        Event {
            payload: vec![],
            payload_none: true,
            ..mk_event(name)
        }
    }

    fn mk_feature(commands: Vec<Command>, events: Vec<Event>) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
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
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_event_without_emitter_fires() {
        let feature = mk_feature(vec![], vec![mk_event("archived")]);
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].event_name, "archived");
        assert_eq!(Finding::CODE, "VOCAB-EVENT-ORPHAN-001");
        assert!(findings[0].message().contains("archived"));
    }

    #[test]
    fn positive_trace_event_does_not_fire() {
        let feature = mk_feature(vec![], vec![mk_trace_event("audit_exported")]);

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "trace events are outside the reaction graph and must not fire"
        );
    }

    #[test]
    fn negative_event_with_command_emits_does_not_fire() {
        let feature = mk_feature(
            vec![mk_cmd("archive_customer", vec!["archived"])],
            vec![mk_event("archived")],
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "same-feature command emits must satisfy the event declaration"
        );
    }

    #[test]
    fn negative_payload_none_event_does_not_fire() {
        let feature = mk_feature(vec![], vec![mk_payload_none_event("heartbeat")]);

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "`payload none` events intentionally opt out of typed event contracts"
        );
    }

    #[test]
    fn positive_multiple_orphans_fire_per_event() {
        let feature = mk_feature(
            vec![],
            vec![
                mk_event("archived"),
                mk_event("deleted"),
                mk_event("merged"),
            ],
        );
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));

        assert_eq!(findings.len(), 3);
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.event_name.as_str())
                .collect::<Vec<_>>(),
            vec!["archived", "deleted", "merged"]
        );
    }

    #[test]
    fn negative_qualified_command_emit_does_not_fire() {
        let feature = mk_feature(
            vec![mk_cmd("archive_customer", vec!["customer.archived"])],
            vec![mk_event("archived")],
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "qualified same-feature event refs resolve by local event name"
        );
    }
}
