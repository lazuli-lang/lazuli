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
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-EVENT-ORPHAN-001";

    /// Render the "no command emits this event" message, naming the
    /// event and offering the canonical fixes.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_event_orphan_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     event_name: "billing.cycle.completed".into(),
    /// };
    /// assert!(f.message().contains("Orphan events leak abstraction"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_event_orphan_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with events");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // EVENT_GROUP-HOIST — `event_group` variants are now hoisted into
    // `Feature.events`, so this rule sees them like any standalone event.
    // Two consequences the producer set has to absorb:
    //
    //  1. Group events are very often produced by a `webhook` /
    //     `notification` / `poller` rather than a `command`/`job` (the
    //     hostpoint MercadoPago `charge_*` family is emitted from the
    //     `mp_payment_event` webhook), so the set spans every `emits`
    //     surface or the hoist would manufacture orphan false positives.
    //
    //  2. A producer may emit either the bare variant name
    //     (`emits mp_status_received`) or the group-prefixed canonical
    //     name (`emits charge_mp_status_received`) — pilots mix both, and
    //     the hoisted event carries the prefixed name. So for every emit
    //     we register the bare last segment AND its expansion under each
    //     group pattern prefix, matching how VOCAB-EVENT-PAYLOAD-001
    //     resolves the same references.
    let group_prefixes: Vec<&str> = feature
        .event_groups
        .iter()
        .map(|g| g.pattern.strip_suffix('*').unwrap_or(g.pattern.as_str()))
        .collect();
    let mut emitted: HashSet<String> = HashSet::new();
    for event_ref in feature
        .commands
        .iter()
        .flat_map(|cmd| cmd.emits.iter())
        .chain(feature.jobs.iter().flat_map(|job| job.emits.iter()))
        .chain(feature.webhooks.iter().flat_map(|wh| wh.emits.iter()))
        .chain(feature.notifications.iter().flat_map(|n| n.emits.iter()))
        .chain(feature.pollers.iter().flat_map(|p| p.emits.iter()))
    {
        let local = event_ref.rsplit('.').next().unwrap_or(event_ref.as_str());
        emitted.insert(local.to_owned());
        for prefix in &group_prefixes {
            if !prefix.is_empty() && !local.starts_with(prefix) {
                emitted.insert(format!("{prefix}{local}"));
            }
        }
    }

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
    use super::{Finding, check};
    use std::path::Path;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults,
        Event, EventField, EventGroup, EventKind, EventVariant, EventVariantKind, Feature,
        OutboxMode, PathRef, Policies, PolicyRef, QualifiedName, TypeRef, Webhook,
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
            public_contract: None,
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
            outbox: OutboxMode::None,
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

    // ── EVENT_GROUP-HOIST producer-set + name-resolution coverage ─────────────

    fn mk_webhook(emits: Vec<&str>) -> Webhook {
        Webhook {
            name: "wh".to_owned(),
            route: "/wh".to_owned(),
            verify: PathRef::authored(String::new()),
            structured_verify: None,
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: PathRef::authored("./wh.go"),
            returns: None,
            emits: emits.iter().map(|s| s.to_string()).collect(),
            emit_predicates: vec![],
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_group(pattern: &str, variant_short_names: &[&str]) -> EventGroup {
        EventGroup {
            pattern: pattern.to_owned(),
            on_resource: Some("Charge".to_owned()),
            raw_payload: vec![],
            raw_audit: None,
            events: variant_short_names.iter().map(|s| s.to_string()).collect(),
            events_outbox: vec![],
            variants: variant_short_names
                .iter()
                .map(|s| EventVariant {
                    name: (*s).to_owned(),
                    kind: EventVariantKind::Committed,
                    outbox: OutboxMode::None,
                    fields: vec![],
                    span_ref: None,
                })
                .collect(),
            span_ref: None,
        }
    }

    /// Hoisted group event whose only producer is a `webhook` (the
    /// hostpoint MercadoPago shape). Without spanning the webhook
    /// `emits` surface the hoist would manufacture an orphan false
    /// positive.
    #[test]
    fn webhook_emit_satisfies_hoisted_group_event() {
        let mut feature = mk_feature(vec![], vec![mk_event("charge_confirmed")]);
        feature.event_groups = vec![mk_group("charge_*", &["confirmed"])];
        feature.webhooks = vec![mk_webhook(vec!["charge_confirmed"])];
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "a webhook emitting the hoisted group event must satisfy the orphan rule"
        );
    }

    /// A webhook may emit the BARE variant name (`mp_status_received`)
    /// while the hoisted event carries the group-prefixed canonical name
    /// (`charge_mp_status_received`). The producer set must expand the
    /// bare emit under the group prefix so the two reconcile.
    #[test]
    fn bare_webhook_emit_resolves_prefixed_hoisted_event() {
        let mut feature = mk_feature(vec![], vec![mk_event("charge_mp_status_received")]);
        feature.event_groups = vec![mk_group("charge_*", &["mp_status_received"])];
        // Webhook authored the bare short name (pilots mix both spellings).
        feature.webhooks = vec![mk_webhook(vec!["mp_status_received"])];
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "a bare webhook emit must reconcile with the prefixed hoisted event name"
        );
    }

    /// A hoisted group event that genuinely has NO producer anywhere
    /// must still fire — the hoist surfaces real dead-code, it does not
    /// blanket-silence the rule.
    #[test]
    fn truly_unemitted_hoisted_group_event_still_fires() {
        let mut feature = mk_feature(vec![], vec![mk_event("charge_canceled")]);
        feature.event_groups = vec![mk_group("charge_*", &["canceled"])];
        // No command/job/webhook/notification/poller emits it.
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].event_name, "charge_canceled");
    }
}
