//! CHANNEL-PAYLOAD-001 — realtime channel's `payload <Type>` references
//! a type that doesn't resolve to a `record` or `resource` declared in
//! the same feature.
//!
//! Realtime bucket cycle MVP rule. The cycle commits to a closed
//! three-child grammar for `channel <name>` (`tenant_from`, `policy`,
//! `payload`) per `docs/proposals/bucket-realtime-cycle.md`. The
//! payload field carries a verbatim type name; this rule resolves it
//! against the feature's declared `records` / `resources` so cold
//! readers and codegen consume a typed shape.
//!
//! Cross-feature payload resolution (importing a record from a
//! sibling feature) is out of MVP scope; the rule deliberately
//! limits itself to same-feature lookups so the gate stays narrow
//! and the codegen substrate has unambiguous input. Promotion to
//! cross-feature resolution waits for pilot evidence per
//! `docs/scope-discipline.md`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// output

/// One CHANNEL-PAYLOAD-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub channel: String,
    pub payload_type: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "CHANNEL-PAYLOAD-001";

    /// Render the "no record/resource declared" message and prompt
    /// the author to declare the missing type or fix the reference.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::channel_payload_unresolved_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     channel: "customer_activity".into(),
    ///     payload_type: "UnknownPayload".into(),
    /// };
    /// assert!(f.message().contains("Cross-feature payload resolution"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "channel `{}` declares `payload {}` but no `record` or `resource` with that \
             name is declared in this feature. Add `record {}` (or `resource {}`) under \
             `domain`, or fix the payload reference to point at an existing typed shape. \
             Cross-feature payload resolution is out of MVP scope — declare the record \
             in this feature for v0.",
            self.channel, self.payload_type, self.payload_type, self.payload_type
        )
    }
}

// detection

/// Run CHANNEL-PAYLOAD-001 against all channels in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O
/// is performed here. Channel payload references are resolved against
/// the feature's own `records` and `resources`; cross-feature lookups
/// are deferred to a future cycle.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::channel_payload_unresolved_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with channels");
/// let _ = check(&feature, Path::new("realtime.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if feature.channels.is_empty() {
        return Vec::new();
    }

    let mut declared = Vec::with_capacity(feature.records.len() + feature.resources.len());
    for record in &feature.records {
        declared.push(record.name.as_str());
    }
    for resource in &feature.resources {
        declared.push(resource.name.as_str());
    }

    let mut out = Vec::new();
    for channel in &feature.channels {
        if !declared.iter().any(|name| *name == channel.payload) {
            out.push(Finding {
                path: path.to_path_buf(),
                channel: channel.name.clone(),
                payload_type: channel.payload.clone(),
            });
        }
    }
    out
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Channel, Defaults, Field, FieldConstraints, Policies, PolicyRef,
        QualifiedName, Record, Resource, TenantFromSpec, TypeRef,
    };

    fn mk_path() -> &'static Path {
        Path::new("features/customer/customer.lzi")
    }

    fn mk_channel(name: &str, payload: &str) -> Channel {
        Channel {
            name: name.to_owned(),
            tenant_from: TenantFromSpec {
                path: lazuli_ir::Path {
                    segments: vec!["org".to_owned()],
                },
            },
            policy: PolicyRef::Atom("@policy.read".to_owned()),
            policy_when_denied: None,
            payload: payload.to_owned(),
            span_ref: None,
        }
    }

    fn mk_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: vec![Field {
                name: "id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: FieldConstraints::default(),
                full_text: false,
                previous_names: vec![],
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            }],
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
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
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(
        channels: Vec<Channel>,
        records: Vec<Record>,
        resources: Vec<Resource>,
    ) -> Feature {
        Feature {
            name: "customer".to_owned(),
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
            commands: vec![],
            apis: vec![],
            records,
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
            channels,
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_payload_unknown_record_fires() {
        let feature = mk_feature(
            vec![mk_channel("customer_activity", "UnknownPayload")],
            vec![],
            vec![],
        );

        let findings = check(&feature, mk_path());

        assert_eq!(findings.len(), 1, "expected exactly one finding");
        assert_eq!(Finding::CODE, "CHANNEL-PAYLOAD-001");
        assert_eq!(findings[0].channel, "customer_activity");
        assert_eq!(findings[0].payload_type, "UnknownPayload");
        let msg = findings[0].message();
        assert!(
            msg.contains("UnknownPayload"),
            "message should cite the unresolved type, got: {msg}"
        );
        assert!(
            msg.contains("MVP scope"),
            "message should mention MVP scope guidance, got: {msg}"
        );
    }

    #[test]
    fn negative_payload_resolves_to_record_does_not_fire() {
        let feature = mk_feature(
            vec![mk_channel("customer_activity", "CustomerActivityEvent")],
            vec![mk_record("CustomerActivityEvent")],
            vec![],
        );

        let findings = check(&feature, mk_path());

        assert!(
            findings.is_empty(),
            "no diagnostic expected when payload resolves to a record, got: {findings:?}"
        );
    }

    #[test]
    fn negative_payload_resolves_to_resource_does_not_fire() {
        let feature = mk_feature(
            vec![mk_channel("customer_activity", "Customer")],
            vec![],
            vec![mk_resource("Customer")],
        );

        assert!(
            check(&feature, mk_path()).is_empty(),
            "no diagnostic expected when payload resolves to a resource"
        );
    }

    #[test]
    fn negative_feature_without_channels_does_not_fire() {
        let feature = mk_feature(vec![], vec![mk_record("Unused")], vec![]);
        assert!(check(&feature, mk_path()).is_empty());
    }

    #[test]
    fn positive_multiple_channels_only_unresolved_fires() {
        let feature = mk_feature(
            vec![
                mk_channel("activity", "CustomerActivityEvent"),
                mk_channel("billing", "MissingPayload"),
            ],
            vec![mk_record("CustomerActivityEvent")],
            vec![],
        );

        let findings = check(&feature, mk_path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].channel, "billing");
        assert_eq!(findings[0].payload_type, "MissingPayload");
    }
}
