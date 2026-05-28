//! Per-resource / per-field attribute resolvers.
//!
//! Holds the small "what does this attribute mean for codegen" helpers
//! that the struct + value-literal emitters call into: tenancy
//! resolution, timestamps inheritance, retention/tenancy constant
//! lifting, geo-point db tag shaping, plugin-semantic validate-tag
//! derivation, secret-bearing capability classification, and the
//! per-`TypeRef` import register.
//!
//! Boundary: nothing here writes Go code. The helpers produce strings
//! or booleans; the struct/value emitters in `super::struct_emit` /
//! `super::mod` decide where they land.

use lazuli_ir::{BuiltinType, CapabilityRef, Feature, RetentionAction, Tenancy, TypeRef};

use crate::emitter::imports::ImportSet;
use crate::emitter::types::{self, TypeCtx};

/// Resolve effective tenancy by combining feature defaults with the
/// resource-level override. Resource override wins; if both unset we
/// default to `Tenancy::None` because the registered Lazuli Go lib
/// only recognises `TenancyOrg`/`TenancyNone` today.
pub(super) fn effective_tenancy(feature: &Feature, resource: &lazuli_ir::Resource) -> Tenancy {
    if let Some(t) = resource.tenancy.clone() {
        return t;
    }
    if let Some(t) = feature.defaults.tenancy.clone() {
        return t;
    }
    Tenancy::None
}

/// Resolve effective timestamps flag — `Resource.timestamps` overrides
/// `Defaults.timestamps`. `Some(false)` is the explicit opt-out.
///
/// Falls through to explicit-field detection when neither knob is set:
/// if the author declared `created_at`/`updated_at` as resource fields
/// directly (without flipping `defaults timestamps` on), the runtime
/// still needs `Timestamps: true` on the emitted `Resource[T]` value so
/// `applyCreates` / `applyUpdates` auto-bump `updated_at = now()` and
/// `applyCreates` auto-sets `updated_at` at INSERT time. Without this,
/// resources that opt into timestamps via explicit-field declaration
/// hit `null value in column "updated_at" violates not-null constraint`
/// on the first INSERT (no automatic binding because `Resource.HasColumn`
/// returns false), even though the DDL has the column.
pub(super) fn uses_timestamps(feature: &Feature, resource: &lazuli_ir::Resource) -> bool {
    if let Some(explicit) = resource.timestamps {
        return explicit;
    }
    if feature.defaults.timestamps {
        return true;
    }
    has_explicit_timestamp_fields(resource)
}

/// True when the resource declares both `created_at` and `updated_at`
/// as explicit fields. Both must be present; a resource that only has
/// `created_at` is engaging "immutable row + audit-trail" semantics,
/// not the auto-touch-on-write convention.
fn has_explicit_timestamp_fields(resource: &lazuli_ir::Resource) -> bool {
    let mut has_created = false;
    let mut has_updated = false;
    for field in &resource.fields {
        match field.name.as_str() {
            "created_at" => has_created = true,
            "updated_at" => has_updated = true,
            _ => {}
        }
    }
    has_created && has_updated
}

/// Lift a `Tenancy` IR variant onto the `lazuli.Tenancy*` constant
/// name. The Lazuli Go lib carries `TenancyNone`/`TenancyOrg`/
/// `TenancyTeam`/`TenancyCustom` (the last is a structural marker
/// — the per-resource custom axis name is not yet threaded through
/// `Resource[T]`; the runtime widens this when a pilot product
/// needs per-row custom-axis scoping).
pub(super) fn tenancy_const(tenancy: Tenancy) -> &'static str {
    match tenancy {
        Tenancy::Org => "lazuli.TenancyOrg",
        Tenancy::Team => "lazuli.TenancyTeam",
        Tenancy::Custom(_) => "lazuli.TenancyCustom",
        Tenancy::None => "lazuli.TenancyNone",
    }
}

pub(super) fn retention_action_const(action: RetentionAction) -> &'static str {
    match action {
        RetentionAction::Anonymize => "lazuli.Anonymize",
        RetentionAction::Delete => "lazuli.Delete",
        RetentionAction::Archive => "lazuli.Archive",
    }
}

/// Compute the per-field `db:"<col>"` value. For `@semantic.GeoPoint`
/// columns the proposal §3.1/§9.1 require the PostGIS type modifier
/// to land in the tag so pgx scans + DDL roundtrip stay aligned. All
/// other types reuse the field name verbatim (lower-snake by
/// convention).
pub(super) fn db_col_for(field: &lazuli_ir::Field, type_ref: &TypeRef) -> String {
    if is_geo_point(type_ref) {
        return format!("{},type:geography(point,4326)", field.name);
    }
    field.name.clone()
}

/// B3 — derive the `validate:"<plugin-short>.<validator>"` clause for
/// plugin-contributed `@semantic.<Name>` fields. The IR carries the
/// plugin namespace verbatim (`@lazuli/plugin-scalars-br`) — strip the
/// `@lazuli/plugin-` prefix to recover the short name — plus the `validator`
/// function name lifted directly from the plugin's manifest. The
/// runtime dispatcher maps `<plugin-short>.<validator>` to the
/// registered Go function via the anonymous-import contract
/// described at `docs/plugin-authoring.md:227`.
///
/// See `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
pub(super) fn plugin_semantic_validate_tag(type_ref: &TypeRef) -> Option<String> {
    let TypeRef::Builtin(BuiltinType::SemanticPluginType {
        plugin, validator, ..
    }) = type_ref
    else {
        return None;
    };
    let short = plugin
        .strip_prefix("@lazuli/plugin-")
        .unwrap_or(plugin.as_str());
    Some(format!("{}.{}", short, validator))
}

fn is_geo_point(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::SemanticGeoPoint))
}

/// True for capability-typed fields whose value the wire must never
/// carry: password hashes, encrypted/E2EE blobs, tokens, and the legacy
/// `@cap.Secret` builtin. Drives the `json:"-"` carve-out so a generic
/// `json.Marshal(resource)` cannot leak credentials.
///
/// `CapabilityRef::File` is intentionally NOT included — file refs are
/// public handles to storage, not secrets.
pub(super) fn is_secret_capability(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Builtin(BuiltinType::CapSecret) => true,
        TypeRef::Capability(cap) => matches!(
            cap,
            CapabilityRef::Hashed(_)
                | CapabilityRef::Encrypted(_)
                | CapabilityRef::E2ee(_)
                | CapabilityRef::Token(_)
        ),
        _ => false,
    }
}

/// Register every import surfaced by a `TypeRef` onto the file-level
/// `ImportSet`. Recurses into `Many<T>` so wrapping a typed `T`
/// carries its import through. Cross-feature refs surface here as
/// `<module>/<feature>` paths and route through `ImportSet::add`'s
/// "third-party" bucket (the classifier doesn't know the difference,
/// but the `import` block still compiles — Go doesn't care which
/// group a local module sits in).
pub(super) fn register_imports_for_type(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}
