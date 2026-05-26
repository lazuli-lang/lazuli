//! EVENT-GROUP-VARIANT-TYPE-001 — `event <name>` declared under an
//! `event_group` carries a payload field whose type literal didn't
//! resolve to a known scalar / capability / record / enum.
//!
//! B5 framework gap 1 follow-up — the analyzer now lifts per-event
//! typed field rows into `EventVariant.fields`. When a field's
//! `TypeRef::UserDefined(qname)` is unresolved (the lifter's
//! fallthrough for unknown identifiers), subscribers will receive an
//! `any`-typed Go field, which silently bypasses the typed contract.
//! Surface the gap at authoring time so the contract stays tight.
//!
//! Scope: only `TypeRef::UserDefined` triggers the rule; built-in
//! scalars, `@semantic.*`, `@cap.*`, records, enums, and resources
//! all pass through.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    EventVariant, Feature, QualifiedName, Resource, TypeRef,
};

/// One EVENT-GROUP-VARIANT-TYPE-001 finding — an `event_group` variant
/// field type doesn't resolve to a known resource/record/enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending event_group lives in.
    pub path: PathBuf,
    /// Feature owning the event_group.
    pub feature: String,
    /// Source pattern of the event_group (e.g. `billing.*`).
    pub group_pattern: String,
    /// Variant whose field type failed to resolve.
    pub variant_name: String,
    /// Offending field name within the variant.
    pub field_name: String,
    /// Verbatim type identifier the author wrote.
    pub authored_type: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "EVENT-GROUP-VARIANT-TYPE-001";

    /// Render the "does not resolve to a known type" message warning
    /// the author that subscribers will receive an untyped slot.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::event_group_variant_type_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     group_pattern: "billing.*".into(),
    ///     variant_name: "PaymentFailed".into(),
    ///     field_name: "reason".into(),
    ///     authored_type: "MissingEnum".into(),
    /// };
    /// assert!(f.message().contains("untyped"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "event_group `{}` variant `{}` declares field `{}: {}` but `{}` does not resolve \
             to a known type. Subscribers will receive an untyped `any` payload slot.",
            self.group_pattern,
            self.variant_name,
            self.field_name,
            self.authored_type,
            self.authored_type
        )
    }
}

/// Run EVENT-GROUP-VARIANT-TYPE-001 over one feature.
///
/// Walks every variant of every `event_group` and emits one finding
/// per unresolved field type. No I/O — `file_path` anchors findings.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::event_group_variant_type_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with event_groups");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, file_path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    let resource_names: Vec<&str> = feature
        .resources
        .iter()
        .map(|r: &Resource| r.name.as_str())
        .collect();
    let record_names: Vec<&str> = feature.records.iter().map(|r| r.name.as_str()).collect();
    let enum_names: Vec<&str> = feature.enums.iter().map(|e| e.name.as_str()).collect();

    for group in &feature.event_groups {
        for variant in &group.variants {
            check_variant(
                feature,
                group.pattern.as_str(),
                variant,
                &resource_names,
                &record_names,
                &enum_names,
                file_path,
                &mut out,
            );
        }
    }
    out
}

fn check_variant(
    feature: &Feature,
    group_pattern: &str,
    variant: &EventVariant,
    resource_names: &[&str],
    record_names: &[&str],
    enum_names: &[&str],
    file_path: &Path,
    out: &mut Vec<Finding>,
) {
    for field in &variant.fields {
        if let TypeRef::UserDefined(qname) = &field.type_ref {
            if !user_defined_resolves(qname, resource_names, record_names, enum_names) {
                out.push(Finding {
                    path: file_path.to_path_buf(),
                    feature: feature.name.clone(),
                    group_pattern: group_pattern.to_owned(),
                    variant_name: variant.name.clone(),
                    field_name: field.name.clone(),
                    authored_type: qname_string(qname),
                });
            }
        }
    }
}

fn user_defined_resolves(
    qname: &QualifiedName,
    resources: &[&str],
    records: &[&str],
    enums: &[&str],
) -> bool {
    let name = qname.name.as_str();
    resources.iter().any(|r| *r == name)
        || records.iter().any(|r| *r == name)
        || enums.iter().any(|e| *e == name)
}

fn qname_string(qname: &QualifiedName) -> String {
    match &qname.feature {
        Some(feature) => format!("{}.{}", feature, qname.name),
        None => qname.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, EventField, EventVariant, EventVariantKind, EventGroup, Feature, OutboxMode,
        Policies, QualifiedName, TypeRef,
    };
    use std::path::PathBuf;

    fn mk_feature() -> Feature {
        Feature {
            name: "payments".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_group_with_variant(field: EventField) -> EventGroup {
        EventGroup {
            pattern: "charge_*".to_owned(),
            on_resource: Some("Charge".to_owned()),
            raw_payload: Vec::new(),
            raw_audit: None,
            events: vec!["confirmed".to_owned()],
            events_outbox: vec![OutboxMode::None],
            variants: vec![EventVariant {
                name: "confirmed".to_owned(),
                kind: EventVariantKind::Committed,
                outbox: OutboxMode::None,
                fields: vec![field],
                span_ref: None,
            }],
            span_ref: None,
        }
    }

    #[test]
    fn builtin_type_passes() {
        let mut feature = mk_feature();
        feature.event_groups.push(mk_group_with_variant(EventField {
            name: "amount".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            optional: false,
        }));
        let findings = check(&feature, &PathBuf::from("payments.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn unresolved_user_defined_fires() {
        let mut feature = mk_feature();
        feature.event_groups.push(mk_group_with_variant(EventField {
            name: "kind".to_owned(),
            type_ref: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "DoesNotExist".to_owned(),
            }),
            optional: false,
        }));
        let findings = check(&feature, &PathBuf::from("payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field_name, "kind");
        assert_eq!(findings[0].authored_type, "DoesNotExist");
    }
}
