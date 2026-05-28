//! Cell E2 — `Resource` + `Record` kind emission. Walks every
//! `Resource` / `Record` declared on a feature and emits the typed
//! struct(s) plus the resource-side `lazuli.Resource[T]` value into
//! `<feature>/resource.gen.go`.
//!
//! Proposal references:
//! - §3.1 — struct layout, tag conventions, `lazuli.Resource[T]` shape.
//! - §3.1 — Record sub-shape (typed struct without identity).
//! - §5.1 — `created_at` / `updated_at` / `deleted_at` lift rules.
//! - §11 — boundary discipline: every `lazuli.*` reference flows
//!   through `types::go_type_for` so `imports::ImportSet` records it.
//!
//! Determinism: features are walked in `module.rs`'s `BTreeMap` order;
//! within a feature, `resources` and `records` are sorted by name; field
//! order preserves the IR `Vec` (already deterministic). Imports
//! collected via `ImportSet` which de-dups and sorts inside three
//! buckets.
//!
//! Boundary: `resource.rs` is the only emitter that knows the
//! `Resource[T]` value shape; the printer remains IR-agnostic.

use lazuli_ir::{Feature, Record, Resource};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::TypeCtx;

mod attributes;
mod encryption;
mod struct_emit;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod encryption_helper_tests;
#[cfg(test)]
mod json_skip_tests;
#[cfg(test)]
mod struct_emit_tests;

use attributes::{register_imports_for_type, uses_timestamps};
use encryption::{emit_encryption_helpers, encrypted_fields};
use struct_emit::{emit_record, emit_resource};

/// Emit `<feature>/resource.gen.go` for a feature, or `None` when the
/// feature declares no resources or records (so `module.rs` skips the
/// file entirely — gofmt would warn on an empty package body).
///
/// `module_name` and `cross_index` are threaded through so the
/// `types::go_type_for` resolver can lift cross-feature references
/// (e.g. a `customer.Customer` field that names a `User` declared in
/// the `org` feature emits `*orggen.User` plus the corresponding
/// `<module_name>/org` import). Callers in `module.rs` construct
/// these once per `generate_v1` run.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_resource_file("billing.lzi", &feature, "demo", &cross_index, &emit_ctx);
/// ```
pub fn emit_resource_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    if feature.resources.is_empty() && feature.records.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    // Cross-feature resolver. Reused for every type lookup inside this
    // file so cross-feature refs land as `<owner>.<Name>` plus a
    // `<module_name>/<owner>` import (proposal §11 boundary).
    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    // Collect sorted resource / record names so iteration order is
    // independent of how the IR `Vec` happened to be populated.
    let mut resources: Vec<&Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    let mut records: Vec<&Record> = feature.records.iter().collect();
    records.sort_by(|a, b| a.name.cmp(&b.name));

    // Pre-walk to populate imports. Each resource may pull in
    // `lazuli.dev/runtime/lazuli` (for `lazuli.ID` / `lazuli.Time` /
    // `lazuli.Resource` / `lazuli.RetentionSpec` / `lazuli.TenancyOrg`)
    // plus per-field imports surfaced by `types::go_type_for`. Records
    // need only the per-field imports.
    for resource in &resources {
        // Lazuli runtime is always present: at minimum the `ID` /
        // `Resource[T]` value + tenancy constant live there.
        imports.add("lazuli.dev/runtime/lazuli");
        if uses_timestamps(feature, resource) || resource.soft_delete {
            // `lazuli.Time` lives in the same `lazuli.dev/runtime/lazuli`
            // package; nothing extra to register.
        }
        for field in &resource.fields {
            register_imports_for_type(&field.type_ref, &type_ctx, &mut imports);
        }
        // W3 GAP-03 — `computed_date` fields emit a `Compute<Field>`
        // helper that parses the base via `time.Parse` and calls
        // `time.Time.AddDate`. Register the stdlib `time` package only
        // when the resource declares at least one such field.
        if resource.fields.iter().any(|f| f.computed_date.is_some()) {
            imports.add("time");
        }
        // Encryption helpers (`Encrypt<Resource>` / `Decrypt<Resource>`)
        // call into `encryption.ForCtx`; register the runtime package
        // only when the resource actually has at least one
        // `@cap.Encrypted` or `@cap.E2ee` field. Proposal §Codegen
        // (`docs/proposals/encryption-vocab.md`).
        if encrypted_fields(resource).next().is_some() {
            imports.add("lazuli.dev/runtime/lazuli/encryption");
        }
    }
    for record in &records {
        for field in &record.fields {
            register_imports_for_type(&field.type_ref, &type_ctx, &mut imports);
        }
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for resource in &resources {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_resource(&mut p, feature, resource, &type_ctx);
        // Encryption helpers — only when the resource carries at least
        // one `@cap.Encrypted` / `@cap.E2ee` field. Emitted right after
        // the `Resource[T]` value so the helpers visually sit next to
        // the resource they apply to.
        if encrypted_fields(resource).next().is_some() {
            p.blank();
            emit_encryption_helpers(&mut p, resource);
        }
    }
    for record in &records {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_record(&mut p, record, &type_ctx);
    }

    Some(p.finish())
}

pub(super) fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}


#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BuiltinType, Defaults, Field, Module, Policies, Resource, TypeRef,
    };

    fn synthetic_feature_with_resource() -> Feature {
        Feature {
            name: "inventory".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: vec![Resource {
                name: "item".to_owned(),
                public_contract: None,
                tenancy: None,
                soft_delete: false,
                timestamps: None,
                fields: vec![Field {
                    name: "sku".to_owned(),
                    type_ref: TypeRef::Builtin(BuiltinType::Text),
                    required: true,
                    unique: true,
                    slug: false,
                    default: None,
                    derived_from: None,
                    computed_date: None,
                    constraints: lazuli_ir::FieldConstraints::default(),
                    full_text: false,
                    previous_names: Vec::new(),
                    pii: None,
                    owner_axis: None,
                    cross_feature_target: None,
                    span_ref: None,
                }],
                constraints: Vec::new(),
                validate: None,
                validates: Vec::new(),
                retention: None,
                previous_names: Vec::new(),
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
            }],
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn module_for(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "inventory-app".to_owned(),
                title: None,
                version: None,
                lazuli_version: None,
                targets: Vec::new(),
                default_locale: None,
                default_timezone: None,
                auth_failed_redirect: None,
                not_found: None,
                error_pages: Vec::new(),
                uses: Vec::new(),
                packs: Vec::new(),
                bindings: Vec::new(),
                architecture: None,
                services: Vec::new(),
                communication: None,
                environments: Vec::new(),
                urls: Vec::new(),
                cors: None,
                headers: None,
                cookie: None,
                proxy: None,
                limits: None,
                env: Vec::new(),
                integrations: Vec::new(),
                capabilities: Vec::new(),
                runtime: Vec::new(),
                deploy: None,
                logging: None,
                tracing: None,
                observability: None,
                locale: None,
                encryption_bindings: Vec::new(),
                route_guard: None,
                actor_query: None,
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    #[test]
    fn feature_emit_resource_entry_point_outputs_resource_file_shape() {
        let feature = synthetic_feature_with_resource();
        let module = module_for(feature.clone());
        let cross_index = CrossFeatureIndex::build(&module);

        let out = emit_resource_file(
            "features/inventory/inventory.lzi",
            &feature,
            "lazuli/inventory-app",
            &cross_index,
        )
        .expect("resource feature entry point should emit for a feature with one resource");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package inventorygen"));
        assert!(out.contains("type Item struct {"));
        assert!(out.contains("Sku string"));
        assert!(out.contains("var itemResource = lazuli.Resource[Item]{"));
    }
}
