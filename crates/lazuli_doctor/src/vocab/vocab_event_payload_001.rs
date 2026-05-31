//! VOCAB-EVENT-PAYLOAD-001 — `emits` without typed payload.
//!
//! ## Rule statement
//!
//! Fires when a `command` declares `emits <event.name>` but the emitted event
//! does not have an IR-visible payload contract: either no same-feature
//! `event <name>` declaration exists, or the event exists but has neither typed
//! payload fields nor the explicit `payload none` sentinel. This keeps event
//! producers visible to doctor, codegen, projections, and reaction-graph tools
//! instead of letting handlers publish untyped names.
//!
//! ## Severity profile
//!
//! Severity: `warning` in both strict and production profiles. Production does
//! not escalate because some payload-less events are legitimate, but those cases
//! must opt out with `payload none` so the contract remains explicit.
//!
//! ## Fixture example
//!
//! ```lzi
//! feature post
//!   command archive_post
//!     emits post.archived
//! ```
//!
//! Canonical fix:
//!
//! ```lzi
//! feature post
//!   event archived
//!     payload
//!       post_id: ID required
//!   command archive_post
//!     emits archived
//! ```
//!
//! For an intentionally payload-less signal, the canonical fix is `event
//! liveness payload none`; the IR stores that as `Event { payload_none: true,
//! payload: [] }`.
//!
//! ## Proposal anchor
//!
//! Historical proposal: `docs/proposals/doctor-vocabulary-lints.md`
//! §VOCAB-EVENT-PAYLOAD-001 (extracted to `lazuli-ops` in commit `acbc3c14`).
//!
//! Diagnostic ID / code constant: `VOCAB-EVENT-PAYLOAD-001`;
//! `Finding::CODE` is `pub const CODE: &'static str =
//! "VOCAB-EVENT-PAYLOAD-001";`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{Event, EventGroup, EventVariant, Feature};

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
    /// Which sub-rule fired — drives the prose in `message`.
    pub kind: FindingKind,
}

/// The two distinct shapes VOCAB-EVENT-PAYLOAD-001 surfaces.
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
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-EVENT-PAYLOAD-001";

    /// Render the per-kind message: undeclared events ask for an
    /// `event ... payload ...` declaration; missing-payload events ask
    /// for an explicit `payload <Type>` or `payload none` opt-out.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_event_payload_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     event_name: "post.archived".into(),
    ///     kind: FindingKind::MissingPayload,
    /// };
    /// assert!(f.message().contains("payload"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_event_payload_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with events + commands");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
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
                    // Not a flat `event` — try `event_group <pattern> on
                    // <Resource>` declarations. A command emits the
                    // *expanded* group event name (pattern `account_*` +
                    // event `signed_up` => `account_signed_up`).
                    match resolve_group_event(&feature.event_groups, event_ref) {
                        // Typed variant: fire MissingPayload only when it
                        // carries no fields (group-level analog of a flat
                        // event missing `payload`); a typed variant ⇒ quiet.
                        GroupResolution::Variant(variant) => {
                            if variant.fields.is_empty() {
                                findings.push(Finding {
                                    path: path.to_path_buf(),
                                    event_name: event_ref.clone(),
                                    kind: FindingKind::MissingPayload,
                                });
                            }
                            continue;
                        }
                        // Resolved by group-event NAME only (this build
                        // lifts inline-field group events into
                        // `EventGroup.events` names, not typed `variants`).
                        // The event IS declared ⇒ quiet; typed-payload
                        // lifting of inline group-event fields is a
                        // separate analyzer gap, not this rule's concern.
                        GroupResolution::NameOnly => continue,
                        GroupResolution::Unresolved => {}
                    }
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

/// Outcome of resolving an emitted event name against the feature's
/// `event_group` declarations.
enum GroupResolution<'a> {
    /// Matched a typed `EventVariant` — caller inspects `fields`.
    Variant(&'a EventVariant),
    /// Matched a group event by NAME only (variant-less inline-field
    /// shape: declared, payload typing unknown to this rule).
    NameOnly,
    /// No group declares this event.
    Unresolved,
}

/// Resolve an emitted event name against `event_group <pattern> on
/// <Resource>` declarations. A command emits the EXPANDED event name (the
/// group pattern's `*` replaced by the short name, e.g. pattern
/// `account_*` + event `signed_up` => `account_signed_up`); some pilot
/// features emit the bare short name, so we accept either spelling.
///
/// Prefers a typed `variant` match (the `payload`-block group shape,
/// which carries `fields`); falls back to the group's `events` NAME list
/// (the inline-field group shape lifts only into names). Either way the
/// event is declared, so the rule must not report it Undeclared.
fn resolve_group_event<'a>(groups: &'a [EventGroup], event_ref: &str) -> GroupResolution<'a> {
    let local = event_ref.rsplit('.').next().unwrap_or(event_ref);
    let matches = |candidate: &str, prefix: &str| -> bool {
        let expanded = format!("{prefix}{candidate}");
        event_ref == expanded || local == expanded || event_ref == candidate || local == candidate
    };
    // First pass: typed variants (richest signal).
    for group in groups {
        let prefix = group.pattern.split('*').next().unwrap_or("");
        for variant in &group.variants {
            if matches(&variant.name, prefix) {
                return GroupResolution::Variant(variant);
            }
        }
    }
    // Second pass: group event names (variant-less inline-field shape).
    for group in groups {
        let prefix = group.pattern.split('*').next().unwrap_or("");
        for event_name in &group.events {
            if matches(event_name, prefix) {
                return GroupResolution::NameOnly;
            }
        }
    }
    GroupResolution::Unresolved
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        Event, EventField, EventGroup, EventKind, EventVariant, EventVariantKind, Feature,
        OutboxMode, Policies, PolicyRef, QualifiedName, ReturnsEffect, TypeRef,
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
            public_contract: None,
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
            policy_when_denied: None,
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
            outbox: OutboxMode::None,
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
            outbox: OutboxMode::None,
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
            outbox: OutboxMode::None,
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
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events,
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

    // ── positive (i): undeclared event ────────────────────────────────────────

    /// Command emits `post.archived` but no feature-level event declared.
    #[test]
    fn positive_i_undeclared_event_fires() {
        let cmd = mk_cmd_emits("archive_post", vec!["post.archived"]);
        let feature = mk_feature(vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "expected one finding for undeclared event"
        );
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
            public_contract: None,
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
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
            derived_from: None,
        };
        let feature = mk_feature(vec![cmd], vec![]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "command with no emits must not fire"
        );
    }

    // ── spec 0012: event_group resolution ─────────────────────────────────────

    fn mk_feature_with_group(
        commands: Vec<Command>,
        pattern: &str,
        variant_name: &str,
        variant_fields: Vec<EventField>,
    ) -> Feature {
        let mut feature = mk_feature(commands, vec![]);
        feature.event_groups = vec![EventGroup {
            pattern: pattern.to_owned(),
            on_resource: Some("User".to_owned()),
            raw_payload: vec![],
            raw_audit: None,
            events: vec![variant_name.to_owned()],
            events_outbox: vec![],
            variants: vec![EventVariant {
                name: variant_name.to_owned(),
                kind: EventVariantKind::Committed,
                outbox: OutboxMode::None,
                fields: variant_fields,
                span_ref: None,
            }],
            span_ref: None,
        }];
        feature
    }

    /// Pilot shape: emitted event declared inside an `event_group` with a
    /// typed payload variant. Must NOT fire (the false positive fixed).
    #[test]
    fn event_payload_quiet_on_event_group_declared() {
        let cmd = mk_cmd_emits("signup", vec!["account_signed_up"]);
        let fields = vec![EventField {
            name: "user_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            optional: false,
        }];
        let feature = mk_feature_with_group(vec![cmd], "account_*", "signed_up", fields);
        assert!(
            check(&feature, Path::new("features/account/account.lzi")).is_empty(),
            "an event declared inside an event_group with a typed payload must not fire"
        );
    }

    /// A truly undeclared event (no flat event AND no group variant) must
    /// still fire — proves fixed, not disabled.
    #[test]
    fn event_payload_fires_on_truly_undeclared() {
        let cmd = mk_cmd_emits("orphan", vec!["account_ghost_event"]);
        let fields = vec![EventField {
            name: "user_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            optional: false,
        }];
        let feature = mk_feature_with_group(vec![cmd], "account_*", "signed_up", fields);
        let findings = check(&feature, Path::new("features/account/account.lzi"));
        assert_eq!(findings.len(), 1, "truly undeclared emit must fire: {findings:?}");
        assert_eq!(findings[0].kind, FindingKind::Undeclared);
        assert_eq!(findings[0].event_name, "account_ghost_event");
    }

    /// A group variant authored with NO fields fires MissingPayload.
    #[test]
    fn event_payload_fires_on_group_variant_without_payload() {
        let cmd = mk_cmd_emits("signup", vec!["account_signed_up"]);
        let feature = mk_feature_with_group(vec![cmd], "account_*", "signed_up", vec![]);
        let findings = check(&feature, Path::new("features/account/account.lzi"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, FindingKind::MissingPayload);
    }

    /// Pilot shape (pauta `account.lzi`, verified against the lowered IR):
    /// the inline-field group-event syntax lifts event NAMES into
    /// `EventGroup.events` but NOT into typed `variants`. The emit must
    /// still resolve (quiet) — the event IS declared. Without the
    /// name-list fallback the rule false-fired on every grouped event.
    #[test]
    fn event_payload_quiet_on_group_event_name_only() {
        let cmd = mk_cmd_emits("signup", vec!["account_signed_up"]);
        let mut feature = mk_feature(vec![cmd], vec![]);
        feature.event_groups = vec![EventGroup {
            pattern: "account_*".to_owned(),
            on_resource: Some("User".to_owned()),
            raw_payload: vec![],
            raw_audit: None,
            events: vec!["signed_up".to_owned(), "deleted".to_owned()],
            events_outbox: vec![],
            variants: vec![],
            span_ref: None,
        }];
        assert!(
            check(&feature, Path::new("features/account/account.lzi")).is_empty(),
            "a group event present by name (variant-less inline-field shape) must not fire"
        );
    }
}
