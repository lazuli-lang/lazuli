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
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::vocab::universal_columns::is_universal_column;
///
/// // `created_at` is universal regardless of declaration context.
/// // (Full call needs `&Field`, `&Feature`, `&Module` IR fixtures —
/// // construct via parser/analyzer in tests.)
/// let _ = is_universal_column;
/// ```
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
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::vocab::universal_columns::is_view_projection_record;
///
/// // Records named `CustomerView`/`OrderSnapshot` etc. with `<noun>_id`
/// // fields are recognised as projections. Construct a `Record` via the
/// // parser/analyzer to exercise the predicate end-to-end.
/// let _ = is_view_projection_record;
/// ```
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
    include!("universal_columns_tests.rs");
}
