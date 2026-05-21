//! Universal-column filter + view-projection detection helpers.
//!
//! Shared between VOCAB-SHADOW-RECORD-001 and VOCAB-RESOURCE-WIDE-CLUSTER-001
//! (proposal `docs/proposals/vocab-shadow-record-vo-extraction.md` v0.2 §5).
//!
//! Both rules walk a pre-filtered field set; "universal" means a field that
//! every resource carries by convention (`id`, timestamps, tenancy FK, peer
//! FKs) and that would otherwise dominate structural-similarity matches with
//! signal-poor agreement.
//!
//! Both functions are pure: no I/O, no allocation beyond the type lookup.

use lazuli_ir::{
    BuiltinType, DefaultValue, Feature, Field, Module, QualifiedName, Record, Resource, Tenancy,
    TypeRef,
};

/// Return true when `field` is a universal column that both shadow-record
/// and resource-wide-cluster lints should EXCLUDE from cluster matching.
///
/// `declaration_name` is the name of the surrounding resource/record so the
/// "FK self-reference" filter (`<resource>_id`) can compare. For command
/// inputs there is no enclosing record to self-reference, so callers pass
/// the command name (still safe — the self-reference filter only fires when
/// the field name matches `<declaration_name>_id` exactly).
///
/// `feature` is the enclosing feature (provides tenancy axis + same-feature
/// resources for the FK-to-peer-resource filter).
///
/// `module` is the IR module (provides cross-feature peer-resource lookup
/// for `uses <feature>` imports — `host: Host` in `catalog` resolves to the
/// `host` feature's `Host` resource).
pub fn is_universal_column(
    field: &Field,
    declaration_name: &str,
    feature: &Feature,
    module: &Module,
) -> bool {
    // Implicit timestamps + soft-delete sentinel.
    if matches!(
        field.name.as_str(),
        "created_at" | "updated_at" | "deleted_at"
    ) {
        return true;
    }

    // Implicit row identity. The `id: Id` column is auto-emitted when the
    // resource has no `composite_key`. Authored `id: Id` declarations are
    // unusual but still universal.
    if field.name == "id"
        && matches!(field.type_ref, TypeRef::Builtin(BuiltinType::Id))
    {
        return true;
    }

    // FK self-reference: `<declaration_name>_id: Id required`.
    if matches!(field.type_ref, TypeRef::Builtin(BuiltinType::Id)) {
        let self_ref_name = format!("{}_id", snake_case(declaration_name));
        if field.name == self_ref_name {
            return true;
        }
    }

    // FK to tenancy axis declared in the feature `defaults`.
    if let Some(tenancy) = &feature.defaults.tenancy {
        let tenancy_field_name = tenancy_field_name(tenancy);
        if let Some(expected) = tenancy_field_name.as_deref() {
            if field.name == expected
                && type_resolves_to_resource(&field.type_ref, expected_resource_name(tenancy).as_deref(), feature, module)
            {
                return true;
            }
        }
    }

    // FK to a peer resource in the capsule.
    if let TypeRef::UserDefined(qname) = &field.type_ref {
        if resolves_to_resource_anywhere(qname, feature, module) {
            return true;
        }
    }

    // Aggregation snapshot fields: `<x>_count: Integer required = 0`.
    if field.name.ends_with("_count")
        && matches!(field.type_ref, TypeRef::Builtin(BuiltinType::Integer))
        && matches!(field.default, Some(DefaultValue::Integer(0)))
    {
        return true;
    }

    false
}

/// Return true when the declaration is a view/projection record — should not
/// participate in shadow-record matching against authored resources.
///
/// Heuristic: name ends in `View` / `Snapshot` / `Entry` / `Item` AND has a
/// field of shape `<noun>_id: ID required` (the denormalised lookup column).
pub fn is_view_projection_record(record: &Record) -> bool {
    let suffix_hit = record_name_has_projection_suffix(&record.name);
    if !suffix_hit {
        return false;
    }
    record.fields.iter().any(|f| {
        f.name.ends_with("_id")
            && matches!(f.type_ref, TypeRef::Builtin(BuiltinType::Id))
            && f.required
    })
}

fn record_name_has_projection_suffix(name: &str) -> bool {
    name.ends_with("View")
        || name.ends_with("Snapshot")
        || name.ends_with("Entry")
        || name.ends_with("Item")
}

fn tenancy_field_name(tenancy: &Tenancy) -> Option<String> {
    match tenancy {
        Tenancy::Org => Some("org".to_owned()),
        Tenancy::Team => Some("team".to_owned()),
        Tenancy::Custom(name) => Some(name.clone()),
        Tenancy::None => None,
    }
}

fn expected_resource_name(tenancy: &Tenancy) -> Option<String> {
    match tenancy {
        Tenancy::Org => Some("Org".to_owned()),
        Tenancy::Team => Some("Team".to_owned()),
        Tenancy::Custom(name) => Some(pascal_case(name)),
        Tenancy::None => None,
    }
}

fn type_resolves_to_resource(
    type_ref: &TypeRef,
    expected: Option<&str>,
    feature: &Feature,
    module: &Module,
) -> bool {
    let Some(expected_name) = expected else {
        return false;
    };
    match type_ref {
        TypeRef::UserDefined(qname) if qname.name == expected_name => {
            resolves_to_resource_anywhere(qname, feature, module)
        }
        _ => false,
    }
}

fn resolves_to_resource_anywhere(
    qname: &QualifiedName,
    feature: &Feature,
    module: &Module,
) -> bool {
    // Explicit cross-feature qualification: `feature.SomeResource`.
    if let Some(origin) = &qname.feature {
        if let Some(other) = module.features.iter().find(|f| &f.name == origin) {
            return other.resources.iter().any(|r| r.name == qname.name);
        }
        return false;
    }
    // Local first; then walk `uses` for cross-feature peer resources.
    if feature.resources.iter().any(|r| r.name == qname.name) {
        return true;
    }
    for used in &feature.uses {
        if let Some(other) = module.features.iter().find(|f| &f.name == used) {
            if other.resources.iter().any(|r| r.name == qname.name) {
                return true;
            }
        }
    }
    false
}

fn snake_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for (i, ch) in input.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn pascal_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut upper_next = true;
    for ch in input.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Defaults, FieldConstraints, Module, Policies,
    };

    fn field(name: &str, type_ref: TypeRef, default: Option<DefaultValue>) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            span_ref: None,
        }
    }

    fn builtin(name: &str, ty: BuiltinType) -> Field {
        field(name, TypeRef::Builtin(ty), None)
    }

    fn user_defined(name: &str, target: &str) -> Field {
        field(
            name,
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: target.to_owned(),
            }),
            None,
        )
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
        }
    }

    fn mk_record(name: &str, fields: Vec<Field>) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields,
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_feature(
        name: &str,
        uses: Vec<String>,
        tenancy: Option<Tenancy>,
        resources: Vec<Resource>,
    ) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults {
                tenancy,
                timestamps: false,
                policy: None,
            },
            uses,
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
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
            span_ref: None,
        }
    }

    fn mk_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    #[test]
    fn timestamps_are_universal() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("created_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
        assert!(is_universal_column(
            &builtin("updated_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
        assert!(is_universal_column(
            &builtin("deleted_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn id_is_universal_only_with_builtin_id_type() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
        // `id: Text` is unusual but not universal — would be a real field.
        assert!(!is_universal_column(
            &builtin("id", BuiltinType::Text),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_self_reference_is_universal() {
        let foo = mk_resource("Foo", vec![]);
        let feature = mk_feature("x", vec![], None, vec![foo]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("foo_id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
        // `bar_id: Id` is NOT a self-reference when declaration is `Foo`.
        assert!(!is_universal_column(
            &builtin("bar_id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_org_tenancy_is_universal() {
        let org = mk_resource("Org", vec![]);
        let feature = mk_feature(
            "x",
            vec![],
            Some(Tenancy::Org),
            vec![org],
        );
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &user_defined("org", "Org"),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_peer_resource_in_same_feature_is_universal() {
        let host = mk_resource("Host", vec![]);
        let feature = mk_feature("x", vec![], None, vec![host]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &user_defined("host", "Host"),
            "Property",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_peer_resource_in_another_feature_via_uses_is_universal() {
        // catalog uses host; catalog.Property has `host: Host` which is FK
        // to host.Host. Without the cross-feature lookup, the rule would
        // mistakenly include `host` in cluster matching.
        let host_res = mk_resource("Host", vec![]);
        let host_feature = mk_feature("host", vec![], None, vec![host_res]);
        let catalog_feature = mk_feature(
            "catalog",
            vec!["host".to_owned()],
            None,
            vec![],
        );
        let module = mk_module(vec![host_feature, catalog_feature.clone()]);
        assert!(is_universal_column(
            &user_defined("host", "Host"),
            "Property",
            &catalog_feature,
            &module,
        ));
    }

    #[test]
    fn unresolved_user_defined_type_is_not_filtered() {
        // A `field: SomeType` where `SomeType` does not resolve to any
        // resource (perhaps a record or enum) — NOT a peer FK; should
        // remain in cluster matching.
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(!is_universal_column(
            &user_defined("category", "Category"),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn aggregation_snapshot_count_is_universal() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        let mut count_field = builtin("unread_count", BuiltinType::Integer);
        count_field.default = Some(DefaultValue::Integer(0));
        assert!(is_universal_column(
            &count_field,
            "Foo",
            &feature,
            &module,
        ));
        // Integer field that does NOT end in _count is real data.
        assert!(!is_universal_column(
            &builtin("rating", BuiltinType::Integer),
            "Foo",
            &feature,
            &module,
        ));
        // _count without 0 default is unusual — keep it (likely real).
        let mut count_no_default = builtin("notify_count", BuiltinType::Integer);
        count_no_default.default = Some(DefaultValue::Integer(5));
        assert!(!is_universal_column(
            &count_no_default,
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn view_projection_record_with_id_lookup_is_filtered() {
        let view = mk_record(
            "PendingReviewEntry",
            vec![
                builtin("review_id", BuiltinType::Id),
                builtin("rating", BuiltinType::Integer),
            ],
        );
        assert!(is_view_projection_record(&view));
    }

    #[test]
    fn record_without_projection_suffix_is_not_filtered() {
        let address = mk_record(
            "Address",
            vec![
                builtin("street", BuiltinType::Text),
                builtin("city", BuiltinType::Text),
            ],
        );
        assert!(!is_view_projection_record(&address));
    }

    #[test]
    fn record_with_projection_suffix_but_no_id_lookup_is_not_filtered() {
        // `OrderItem` ends in `Item` but carries no `<noun>_id` lookup column;
        // the suffix alone is not enough to call it a projection.
        let item = mk_record(
            "OrderItem",
            vec![
                builtin("sku", BuiltinType::Text),
                builtin("quantity", BuiltinType::Integer),
            ],
        );
        assert!(!is_view_projection_record(&item));
    }
}
