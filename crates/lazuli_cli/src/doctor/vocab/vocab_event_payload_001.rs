//! VOCAB-EVENT-PAYLOAD-001 — `emits` without typed payload.
//!
//! Fires when a `command` declares `emits <event.name>` and any of:
//!   (i)  the event is not declared at the feature level at all, OR
//!   (ii) the event is declared but has neither `payload <Type>` nor
//!        `payload none` (the explicit opt-out sentinel).
//!
//! `payload none` is the catalog-fixed opt-out for intentionally payload-less
//! events (heartbeats, liveness signals). It is represented in the IR as
//! `Event { payload_none: true, payload: [] }`.
//!
//! Severity: `warning` (strict-profile), `warning` (production-profile).
//! Reference: docs/proposals/doctor-vocabulary-lints.md §VOCAB-EVENT-PAYLOAD-001

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{Event, Feature};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-EVENT-PAYLOAD-001 finding: a command emits reference without a
/// typed payload on the corresponding feature-level event declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Fully-qualified event name as authored in `command.emits`
    /// (e.g. `"post.archived"`).
    pub event_name: String,
    pub kind: FindingKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// The event name appears in a command `emits` list but has no matching
    /// `event <name>` declaration at the feature level.
    Undeclared,
    /// The event is declared at the feature level but carries no `payload <Type>`
    /// and no `payload none` opt-out.
    MissingPayload,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-EVENT-PAYLOAD-001";

    pub fn message(&self) -> String {
        match self.kind {
            FindingKind::Undeclared => format!(
                "command emits `{}` but the event is not declared at the feature level; \
                 add `event {} payload <Type>` (or `payload none` to opt out explicitly). \
                 Unregistered event names are invisible to doctor, codegen, and \
                 the reaction graph.",
                self.event_name, self.event_name
            ),
            FindingKind::MissingPayload => format!(
                "event `{}` is declared but has no payload; add `payload <Type>` to give \
                 subscribers a typed contract, or `payload none` to explicitly opt out \
                 (required for intentionally payload-less events such as heartbeats).",
                self.event_name
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-EVENT-PAYLOAD-001 for all commands in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here. The caller (doctor walker) maps each `Finding` into a
/// `DoctorDiagnostic` and supplies the exact source line from
/// `Tier3FeatureFacts.command_lines`.
///
/// Deduplicates across multiple commands that emit the same undeclared event:
/// at most one `Undeclared` finding per unique event name.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // Build a lookup: local event name → Event declaration.
    // Feature-level events are stored unqualified; command emits use the
    // qualified `<feature>.<event>` form. We resolve by the suffix after the
    // last dot.
    let declared: HashMap<&str, &Event> = feature
        .events
        .iter()
        .map(|ev| (ev.name.as_str(), ev))
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
    let mut seen_undeclared: std::collections::HashSet<String> = std::collections::HashSet::new();

    for cmd in &feature.commands {
        for event_ref in &cmd.emits {
            // Resolve the local name: take the last segment after `.`.
            let local_name = event_ref.rsplit('.').next().unwrap_or(event_ref.as_str());

            match declared.get(local_name) {
                None => {
                    if seen_undeclared.insert(event_ref.clone()) {
                        findings.push(Finding {
                            path: path.to_path_buf(),
                            event_name: event_ref.clone(),
                            kind: FindingKind::Undeclared,
                        });
                    }
                }
                Some(ev) => {
                    // Declared. Fire only when both:
                    //   - no typed payload fields, AND
                    //   - `payload none` opt-out is absent.
                    if ev.payload.is_empty() && !ev.payload_none {
                        findings.push(Finding {
                            path: path.to_path_buf(),
                            event_name: event_ref.clone(),
                            kind: FindingKind::MissingPayload,
                        });
                    }
                }
            }
        }
    }

    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        Event, EventField, EventKind, Feature, Policies, PolicyRef, QualifiedName, ReturnsEffect,
        TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_cmd_emits(name: &str, emits: Vec<&str>) -> Command {
        Command {
            name: name.to_owned(),
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: qn("Post"),
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

    fn mk_event_with_payload(name: &str, field_name: &str) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload: vec![EventField {
                name: field_name.to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                optional: false,
            }],
            payload_none: false,
            level: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_event_no_payload(name: &str) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload: vec![],
            payload_none: false,
            level: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_event_payload_none(name: &str) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload: vec![],
            payload_none: true,
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

    // ── positive (i): undeclared event ────────────────────────────────────────

    /// Command emits `post.archived` but no feature-level event declared.
    #[test]
    fn positive_i_undeclared_event_fires() {
        let cmd = mk_cmd_emits("archive_post", vec!["post.archived"]);
        let feature = mk_feature(vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1, "expected one finding for undeclared event");
        assert_eq!(findings[0].event_name, "post.archived");
        assert_eq!(findings[0].kind, FindingKind::Undeclared);
        assert_eq!(Finding::CODE, "VOCAB-EVENT-PAYLOAD-001");
        assert!(findings[0].message().contains("post.archived"));
    }

    // ── positive (ii): declared event missing payload ─────────────────────────

    /// Event declared at feature level but `payload` block absent.
    #[test]
    fn positive_ii_declared_no_payload_fires() {
        let cmd = mk_cmd_emits("archive_post", vec!["archived"]);
        let ev = mk_event_no_payload("archived");
        let feature = mk_feature(vec![cmd], vec![ev]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "declared event without payload must fire"
        );
        assert_eq!(findings[0].kind, FindingKind::MissingPayload);
        assert!(findings[0].message().contains("archived"));
    }

    // ── negative (i): declared event with typed payload ───────────────────────

    /// `event post.archived payload Post` — must NOT fire.
    #[test]
    fn negative_i_typed_payload_does_not_fire() {
        let cmd = mk_cmd_emits("archive_post", vec!["archived"]);
        let ev = mk_event_with_payload("archived", "post_id");
        let feature = mk_feature(vec![cmd], vec![ev]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "event with typed payload must not trigger VOCAB-EVENT-PAYLOAD-001"
        );
    }

    // ── negative (ii): payload none opt-out ───────────────────────────────────

    /// `event post.archived payload none` — explicit opt-out; must NOT fire.
    #[test]
    fn negative_ii_payload_none_does_not_fire() {
        let cmd = mk_cmd_emits("heartbeat", vec!["liveness"]);
        let ev = mk_event_payload_none("liveness");
        let feature = mk_feature(vec![cmd], vec![ev]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "`payload none` opt-out must not trigger VOCAB-EVENT-PAYLOAD-001"
        );
    }

    // ── dedup: same undeclared event from two commands ────────────────────────

    #[test]
    fn dedup_undeclared_emitted_by_two_commands() {
        let cmd_a = mk_cmd_emits("archive", vec!["post.archived"]);
        let cmd_b = mk_cmd_emits("bulk_archive", vec!["post.archived"]);
        let feature = mk_feature(vec![cmd_a, cmd_b], vec![]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "same undeclared event emitted by two commands should produce one finding"
        );
    }

    // ── command emitting multiple events, one undeclared ─────────────────────

    #[test]
    fn mixed_one_declared_one_undeclared() {
        let cmd = mk_cmd_emits("publish", vec!["published", "feed.updated"]);
        // "published" is declared with payload; "feed.updated" is not declared.
        let ev = mk_event_with_payload("published", "post_id");
        let feature = mk_feature(vec![cmd], vec![ev]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "only the undeclared event should fire");
        assert_eq!(findings[0].event_name, "feed.updated");
        assert_eq!(findings[0].kind, FindingKind::Undeclared);
    }

    // ── command with no emits — silent ────────────────────────────────────────

    #[test]
    fn no_emits_is_silent() {
        let cmd = Command {
            name: "create_post".to_owned(),
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
            policy: PolicyRef::None,
            policy_expr: None,
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
            tests: None,
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(vec![cmd], vec![]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "command with no emits must not fire"
        );
    }
}
