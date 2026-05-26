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
mod json_skip_tests;

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
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BuiltinType, CapabilityRef, Defaults, EncryptedCapability, Feature, Field,
        Module, Policies, Record, Resource, RetentionAction, RetentionSpec, Tenancy, TypeRef,
    };

    /// Test helper: build a single-feature module around `feature`,
    /// construct the cross-feature index against it, and emit the
    /// resource file. Pre-Phase-Prep tests didn't need the index;
    /// the helper keeps the suite concise while threading the new
    /// arguments through.
    fn emit(feature: &Feature) -> Option<String> {
        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
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
            features: vec![feature.clone()],
        };
        let index = CrossFeatureIndex::build(&module);
        emit_resource_file("examples/x.lzi", feature, "lazuli/test", &index)
    }

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
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
            resources: Vec::new(),
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

    fn simple_field(name: &str, builtin: BuiltinType, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn simple_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn single_resource_emits_struct_and_resource_value() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![
                simple_field("name", BuiltinType::Text, true),
                simple_field("email", BuiltinType::SemanticEmail, true),
            ],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");

        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("import (\n\t\"lazuli.dev/runtime/lazuli\"\n)"));
        assert!(out.contains("type Customer struct {"));
        assert!(out.contains("ID    lazuli.ID"));
        assert!(out.contains("Name  string"));
        assert!(out.contains("Email lazuli.Email"));
        assert!(out.contains("var customerResource = lazuli.Resource[Customer]{"));
        assert!(out.contains("Name:    \"customer\","));
        assert!(out.contains("Feature: \"customer\","));
        // No SoftDelete here, so key column pads to `Tenancy:` width (8).
        assert!(out.contains("Tenancy: lazuli.TenancyNone,"));
        // No retention block.
        assert!(!out.contains("RetentionSpec"));
        // No soft-delete row.
        assert!(!out.contains("DeletedAt"));
    }

    #[test]
    fn semantic_plugin_type_field_emits_validate_tag() {
        // B3 — `@semantic.BrazilianCPF` lowers to
        // `SemanticPluginType { plugin: "@lazuli/plugin-scalars-br", name:
        // "BrazilianCPF", carrier: Text, validator: "ValidateCPF" }`.
        // The emitted struct field must carry the `string` carrier
        // Go type plus a `validate:"scalars-br.ValidateCPF"` tag clause.
        // See `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
        let mut feature = base_feature("host");
        let plugin_field = Field {
            name: "cpf".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticPluginType {
                plugin: "@lazuli/plugin-scalars-br".to_owned(),
                name: "BrazilianCPF".to_owned(),
                carrier: Box::new(BuiltinType::Text),
                validator: "ValidateCPF".to_owned(),
                go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
                ts_package: "@lazuli/plugin-scalars-br".to_owned(),
                error_code: "cpf_invalid".to_owned(),
                message_key: String::new(),
                ts_validator: String::new(),
            }),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        };
        feature
            .resources
            .push(simple_resource("Host", vec![plugin_field]));
        let out = emit(&feature).expect("must emit");

        // Carrier is `Text` → Go `string`.
        assert!(out.contains("Cpf string"));
        // Validate tag uses the plugin short name + validator function.
        assert!(
            out.contains("validate:\"scalars-br.ValidateCPF\""),
            "expected validate tag, got: {out}"
        );
        // db + json tags preserved alongside validate.
        assert!(out.contains("db:\"cpf\""));
        assert!(out.contains("json:\"cpf\""));
    }

    #[test]
    fn record_emits_struct_without_resource_value() {
        let mut feature = base_feature("customer");
        feature.records.push(Record {
            name: "CustomerLtv".to_owned(),
            public_contract: None,
            fields: vec![
                simple_field("customer_id", BuiltinType::Id, true),
                simple_field(
                    "amount",
                    BuiltinType::SemanticMoney {
                        currency: lazuli_ir::CurrencyCode::BRL,
                    },
                    true,
                ),
            ],
            discriminator_field: None,
            span_ref: None,
        });
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerLtv struct {"));
        assert!(!out.contains("lazuli.Resource"));
        assert!(out.contains("CustomerID lazuli.ID"));
        assert!(out.contains("Amount     lazuli.Money"));
    }

    #[test]
    fn cap_file_field_registers_storage_import() {
        let mut feature = base_feature("docs");
        let resource = simple_resource(
            "document",
            vec![Field {
                name: "blob".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::CapFile),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(out.contains("Blob storage.FileRef"));
    }

    #[test]
    fn geopoint_field_emits_postgis_import_and_geo_tag() {
        let mut feature = base_feature("places");
        let resource = simple_resource(
            "place",
            vec![simple_field(
                "location",
                BuiltinType::SemanticGeoPoint,
                true,
            )],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"github.com/cridenour/go-postgis\""));
        assert!(out.contains("Location postgis.Point"));
        // The geo column tag must carry the PostGIS type modifier so
        // pgx scans + DDL stay aligned (proposal §3.1 / §9.1).
        assert!(
            out.contains("db:\"location,type:geography(point,4326)\""),
            "expected geo type modifier in db tag, output was:\n{out}"
        );
    }

    #[test]
    fn soft_delete_adds_deleted_at_pointer_with_omitempty() {
        let mut feature = base_feature("customer");
        let mut resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        resource.soft_delete = true;
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("DeletedAt *lazuli.Time"));
        assert!(out.contains("json:\"deleted_at,omitempty\""));
        // With soft_delete, `SoftDelete:` (11 chars) is the widest key
        // so it occupies the column verbatim without padding.
        assert!(out.contains("SoftDelete: true,"));
    }

    #[test]
    fn retention_spec_lifts_window_and_action() {
        let mut feature = base_feature("customer");
        let mut resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        resource.retention = Some(RetentionSpec {
            duration: "7y".to_owned(),
            action: RetentionAction::Anonymize,
        });
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("Retention: &lazuli.RetentionSpec{"));
        assert!(out.contains("Window: lazuli.Duration(\"7y\"),"));
        assert!(out.contains("Then:   lazuli.Anonymize,"));
    }

    #[test]
    fn tenancy_org_emits_orgid_field_and_constant() {
        let mut feature = base_feature("customer");
        let mut resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        resource.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("OrgID lazuli.ID"));
        assert!(out.contains("Tenancy: lazuli.TenancyOrg,"));
    }

    #[test]
    fn feature_default_timestamps_lift_created_updated_at() {
        let mut feature = base_feature("customer");
        feature.defaults.timestamps = true;
        let resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("CreatedAt lazuli.Time"));
        assert!(out.contains("UpdatedAt lazuli.Time"));
    }

    #[test]
    fn resource_timestamps_override_feature_default() {
        let mut feature = base_feature("customer");
        feature.defaults.timestamps = true;
        let mut resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        // Explicit opt-out on the resource side beats the feature default.
        resource.timestamps = Some(false);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("CreatedAt"));
        assert!(!out.contains("UpdatedAt"));
    }

    #[test]
    fn optional_field_emits_pointer_and_omitempty() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, false)],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("Name *string"));
        assert!(out.contains("json:\"name,omitempty\""));
    }

    #[test]
    fn derived_field_renders_as_comment_only() {
        let mut feature = base_feature("customer");
        let mut field = simple_field("is_high_value", BuiltinType::Boolean, true);
        field.derived_from = Some("score > 80".to_owned());
        let resource = simple_resource("customer", vec![field]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// IsHighValue is derived"));
        // Not a real struct field.
        assert!(!out.contains("IsHighValue bool"));
    }

    #[test]
    fn capability_encrypted_field_uses_lazuli_encrypted_ref() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![Field {
                name: "external_id".to_owned(),
                type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                    key: "@key.tenant".to_owned(),
                })),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("ExternalID lazuli.EncryptedRef"));
    }

    #[test]
    fn encrypted_field_emits_encrypt_and_decrypt_helpers_with_runtime_import() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![Field {
                name: "external_id".to_owned(),
                type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                    key: "@key.tenant".to_owned(),
                })),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");

        // Import wired only when the resource carries encrypted fields.
        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/encryption\""),
            "expected encryption import:\n{out}"
        );
        // Helper signatures.
        assert!(
            out.contains("func EncryptCustomer(ctx *lazuli.Ctx, row *Customer) error {"),
            "expected EncryptCustomer signature:\n{out}"
        );
        assert!(
            out.contains("func DecryptCustomer(ctx *lazuli.Ctx, row *Customer) error {"),
            "expected DecryptCustomer signature:\n{out}"
        );
        // Pattern annotations honour the lint contract.
        assert!(out.contains("//lazuli:pattern resource_encrypt v1"));
        assert!(out.contains("//lazuli:pattern resource_decrypt v1"));
        // Cipher resolution lifts the `@key.<scope>` reference verbatim.
        assert!(
            out.contains("encryption.ForCtx(ctx, \"@key.tenant\", \"\")"),
            "expected ForCtx call with @key.tenant scope:\n{out}"
        );
        // Required field — no nil-pointer guard, direct len check.
        assert!(out.contains("if len(row.ExternalID) > 0 {"));
        assert!(out.contains("cipher.Encrypt(row.ExternalID)"));
        assert!(out.contains("cipher.Decrypt(row.ExternalID)"));
    }

    #[test]
    fn optional_encrypted_field_guards_nil_and_dereferences() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![Field {
                name: "external_id".to_owned(),
                type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                    key: "@key.tenant".to_owned(),
                })),
                required: false,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("if row.ExternalID != nil && len(*row.ExternalID) > 0 {"),
            "expected pointer-nil guard:\n{out}"
        );
        assert!(out.contains("cipher.Encrypt((*row.ExternalID))"));
        assert!(out.contains("cipher.Decrypt((*row.ExternalID))"));
    }

    #[test]
    fn e2ee_field_skipped_on_decrypt_path() {
        use lazuli_ir::E2eeCapability;
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![Field {
                name: "private_note".to_owned(),
                type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
                    key: "@key.user".to_owned(),
                })),
                required: true,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        // Encrypt covers E2ee.
        assert!(out.contains("cipher.Encrypt(row.PrivateNote)"));
        // Decrypt skips E2ee — the helper body is the E2ee-only sentinel.
        assert!(
            out.contains("Every encrypted field on this resource is @cap.E2ee"),
            "expected E2ee-only Decrypt sentinel:\n{out}"
        );
        assert!(!out.contains("cipher.Decrypt(row.PrivateNote)"));
    }

    #[test]
    fn resource_without_encrypted_fields_omits_helpers_and_import() {
        let mut feature = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![simple_field("name", BuiltinType::Text, true)],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("lazuli/encryption"));
        assert!(!out.contains("EncryptCustomer"));
        assert!(!out.contains("DecryptCustomer"));
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource(
            "zebra",
            vec![simple_field("name", BuiltinType::Text, true)],
        ));
        feature.resources.push(simple_resource(
            "alpha",
            vec![simple_field("name", BuiltinType::Text, true)],
        ));
        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
        // Resources sorted alphabetically: alpha banner appears before zebra.
        let alpha_pos = a.find("Resource: Alpha").expect("alpha banner");
        let zebra_pos = a.find("Resource: Zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn cross_feature_user_defined_field_emits_qualified_ref_and_import() {
        // Two features: `customer` declares `Customer`; `org` declares
        // `User`. `Customer.owner: User` references a type the
        // analyzer left as `feature: None` because it lives in
        // another feature. The cross-feature resolver should lift it
        // to `*orggen.User` plus an `lazuli/test/org` import.
        let mut customer = base_feature("customer");
        let resource = simple_resource(
            "customer",
            vec![Field {
                name: "owner".to_owned(),
                type_ref: TypeRef::UserDefined(lazuli_ir::QualifiedName {
                    feature: None,
                    name: "User".to_owned(),
                }),
                required: false,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                full_text: false,
                previous_names: Vec::new(),
                pii: None,
                owner_axis: None,
                span_ref: None,
            }],
        );
        customer.resources.push(resource);

        let mut org = base_feature("org");
        org.resources.push(simple_resource("user", Vec::new()));
        // The org resource is named lowercase; pascal-case the
        // emitter-side declared type still appears as `User` in
        // the index because `CrossFeatureIndex` keys on
        // `Resource.name` verbatim. So we build the index against
        // an explicit `User` resource.
        org.resources.clear();
        org.resources.push(Resource {
            name: "User".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
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
        });

        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
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
            features: vec![customer.clone(), org],
        };
        let index = CrossFeatureIndex::build(&module);
        let out = emit_resource_file("examples/x.lzi", &customer, "lazuli/test", &index)
            .expect("must emit");

        // Resource FK fields collapse to `lazuli.ID` — the DB column
        // is BIGINT, so `pgx.RowToStructByName` can't scan into a
        // `*orggen.User` struct. The cross-feature import isn't
        // required for the FK column itself (it's just a BIGINT) but
        // it may still appear if other fields on the resource use a
        // record/enum from `org`; this resource has none.
        assert!(
            out.contains("Owner *lazuli.ID"),
            "expected `*lazuli.ID` FK field for cross-feature resource ref, got:\n{out}"
        );
    }

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
                    constraints: lazuli_ir::FieldConstraints::default(),
                    full_text: false,
                    previous_names: Vec::new(),
                    pii: None,
                    owner_axis: None,
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
