//! Cell G4 - `Storage` kind emission. Walks every typed
//! `@cap.File(...)` site declared on a feature and emits one
//! `storage.FileContract` value per site into `<feature>/storage.gen.go`.
//!
//! Resource-field sites lower with `Resource` + `Field`; API-output
//! sites lower with `API`. The Lazuli Go runtime stores both shapes in
//! `storage.FileContract` so upload, private fetch, and signed URL
//! helpers can consume a single contract type.
//!
//! Determinism: resource and API sites are collected into one vector and
//! sorted by kind/name before emission. Imports flow through `ImportSet`
//! so the `storage` runtime import and optional `time` import remain
//! stable and de-duplicated.

use lazuli_ir::{
    Api, CapabilityRef, Feature, Field, FileCapability, FileSizeLiteral, FileVisibility, MimeType,
    Resource, TypeRef,
};

use super::casing::{lower_camel, pascal_case};
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/storage.gen.go` for a feature, or `None` when the
/// feature declares no typed `@cap.File(...)` resource fields or API
/// outputs.
pub fn emit_storage_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    // Storage emission is local to the current feature; the signature
    // mirrors the other per-kind emitters so module orchestration can
    // call every emitter through the same shape.
    let _ = (module_name, cross_index);

    let mut sites = collect_storage_sites(feature);
    if sites.is_empty() {
        return None;
    }
    sites.sort_by(|a, b| site_sort_key(a).cmp(&site_sort_key(b)));

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli/storage");
    if sites.iter().any(|site| site_uses_time(site)) {
        imports.add("time");
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    for (i, site) in sites.iter().enumerate() {
        if i > 0 {
            p.blank();
        }
        emit_storage_site(&mut p, feature, site);
    }

    Some(p.finish())
}

#[derive(Clone, Copy)]
enum StorageSite<'a> {
    ResourceField {
        resource: &'a Resource,
        field: &'a Field,
        capability: &'a FileCapability,
    },
    ApiOutput {
        api: &'a Api,
        capability: &'a FileCapability,
    },
}

fn collect_storage_sites(feature: &Feature) -> Vec<StorageSite<'_>> {
    let mut sites = Vec::new();

    for resource in &feature.resources {
        for field in &resource.fields {
            if let Some(capability) = file_capability(&field.type_ref) {
                sites.push(StorageSite::ResourceField {
                    resource,
                    field,
                    capability,
                });
            }
        }
    }

    for api in &feature.apis {
        if let Some(capability) = file_capability(&api.output) {
            sites.push(StorageSite::ApiOutput { api, capability });
        }
    }

    sites
}

fn file_capability(type_ref: &TypeRef) -> Option<&FileCapability> {
    match type_ref {
        TypeRef::Capability(CapabilityRef::File(file)) => Some(file),
        _ => None,
    }
}

fn site_sort_key<'a>(site: &'a StorageSite<'a>) -> (u8, &'a str, &'a str) {
    match site {
        StorageSite::ResourceField {
            resource, field, ..
        } => (0, resource.name.as_str(), field.name.as_str()),
        StorageSite::ApiOutput { api, .. } => (1, api.name.as_str(), ""),
    }
}

fn site_uses_time(site: &StorageSite<'_>) -> bool {
    duration_expr(site_capability(site))
        .map(|expr| expr.contains("time."))
        .unwrap_or(false)
}

fn site_capability<'a>(site: &StorageSite<'a>) -> &'a FileCapability {
    match site {
        StorageSite::ResourceField { capability, .. } => capability,
        StorageSite::ApiOutput { capability, .. } => capability,
    }
}

fn emit_storage_site(p: &mut GoPrinter, feature: &Feature, site: &StorageSite<'_>) {
    write_section_banner(p, &site_banner(site));

    let var_name = storage_var_name(feature, site);
    p.line(&format!("var {var_name} = storage.FileContract{{"));
    p.indent();

    let mut rows: Vec<(String, String)> = Vec::new();
    match site {
        StorageSite::ResourceField {
            resource, field, ..
        } => {
            rows.push((
                "Resource:".to_owned(),
                format!("\"{}\",", escape_string(&resource.name)),
            ));
            rows.push((
                "Field:".to_owned(),
                format!("\"{}\",", escape_string(&field.name)),
            ));
        }
        StorageSite::ApiOutput { api, .. } => {
            rows.push((
                "API:".to_owned(),
                format!("\"{}\",", escape_string(&api.name)),
            ));
        }
    }

    let capability = site_capability(site);
    rows.push((
        "MaxSize:".to_owned(),
        format!("{},", max_size_expr(capability)),
    ));
    rows.push(("Accept:".to_owned(), format_accept(&capability.accept)));
    rows.push((
        "Visibility:".to_owned(),
        format!("{},", visibility_const(capability)),
    ));
    if capability.signed_ttl.is_some() {
        rows.push(("SignedTTL:".to_owned(), signed_ttl_value(capability)));
    }

    let key_width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in &rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }

    p.dedent();
    p.line("}");
}

fn site_banner(site: &StorageSite<'_>) -> Vec<String> {
    match site {
        StorageSite::ResourceField {
            resource, field, ..
        } => vec![
            format!("Storage: {}.{}", resource.name, field.name),
            format!("  resource {}.{}", resource.name, field.name),
        ],
        StorageSite::ApiOutput { api, .. } => vec![
            format!("Storage: api {}", api.name),
            format!("  api {} output", api.name),
        ],
    }
}

fn storage_var_name(feature: &Feature, site: &StorageSite<'_>) -> String {
    let suffix_source = match site {
        StorageSite::ResourceField { field, .. } => field.name.as_str(),
        StorageSite::ApiOutput { api, .. } => api.name.as_str(),
    };
    format!(
        "{}{}File",
        lower_camel(&feature.name),
        pascal_case(suffix_source)
    )
}

fn max_size_expr(capability: &FileCapability) -> String {
    match capability.max_size.literal {
        FileSizeLiteral::Kb(n) => format!("{n} * 1024"),
        FileSizeLiteral::Mb(n) => format!("{n} * 1024 * 1024"),
        FileSizeLiteral::Gb(n) => format!("{n} * 1024 * 1024 * 1024"),
    }
}

fn format_accept(accept: &[MimeType]) -> String {
    if accept.is_empty() {
        return "nil,".to_owned();
    }
    let entries: Vec<String> = accept
        .iter()
        .map(|mime| {
            format!(
                "{{Family: \"{}\", Subtype: \"{}\"}}",
                escape_string(&mime.family),
                escape_string(&mime.subtype)
            )
        })
        .collect();
    format!("[]storage.MimeType{{{}}},", entries.join(", "))
}

fn visibility_const(capability: &FileCapability) -> &'static str {
    match capability.visibility.unwrap_or(FileVisibility::Private) {
        FileVisibility::Public => "storage.VisibilityPublic",
        FileVisibility::Private => "storage.VisibilityPrivate",
        FileVisibility::Signed => "storage.VisibilitySigned",
    }
}

fn signed_ttl_value(capability: &FileCapability) -> String {
    match duration_expr(capability) {
        Some(expr) => format!("{expr},"),
        None => format!(
            "0, // TODO(ir): unsupported signed_ttl literal \"{}\"",
            escape_string(capability.signed_ttl.as_deref().unwrap_or(""))
        ),
    }
}

fn duration_expr(capability: &FileCapability) -> Option<String> {
    let raw = capability.signed_ttl.as_deref()?.trim();
    let digit_count = raw.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let amount: u64 = raw[..digit_count].parse().ok()?;
    if amount == 0 {
        return Some("0".to_owned());
    }

    let unit = raw[digit_count..].trim();
    match unit {
        "s" | "sec" | "second" | "seconds" => Some(format!("{amount} * time.Second")),
        "m" | "min" | "minute" | "minutes" => Some(format!("{amount} * time.Minute")),
        "h" | "hr" | "hour" | "hours" => Some(format!("{amount} * time.Hour")),
        "d" | "day" | "days" => Some(format!("{amount} * 24 * time.Hour")),
        _ => None,
    }
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Api, AppManifest, CapabilityRef, Defaults, Feature, Field, FileCapability, FileSize,
        FileSizeLiteral, FileVisibility, HttpMethod, MimeType, Module, PathRef, Policies,
        PolicyRef, Resource, TypeRef,
    };

    fn emit(feature: &Feature) -> Option<String> {
        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        let index = CrossFeatureIndex::build(&module);
        emit_storage_file("examples/x.lzi", feature, "lazuli/test", &index)
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
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
            span_ref: None,
        }
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
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
        }
    }

    fn file_field(name: &str, capability: FileCapability) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::File(capability)),
            required: true,
            unique: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn make_file_capability(
        literal: FileSizeLiteral,
        accept: Vec<(&str, &str)>,
        visibility: Option<FileVisibility>,
        signed_ttl: Option<&str>,
    ) -> FileCapability {
        let max_size = FileSize {
            bytes: literal.bytes(),
            literal,
        };
        FileCapability {
            max_size,
            accept: accept
                .into_iter()
                .map(|(family, subtype)| MimeType {
                    family: family.to_owned(),
                    subtype: subtype.to_owned(),
                })
                .collect(),
            visibility,
            signed_ttl: signed_ttl.map(str::to_owned),
        }
    }

    fn mb(n: u32) -> FileSizeLiteral {
        FileSizeLiteral::Mb(n)
    }

    fn api(name: &str, output: TypeRef) -> Api {
        Api {
            name: name.to_owned(),
            method: HttpMethod::Get,
            path: format!("/api/{name}"),
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            output,
            handler: PathRef::authored("./api/handler.go"),
            locale_negotiate: None,
            deprecated: None,
            span_ref: None,
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn resource_field_emits_private_contract() {
        let mut feature = base_feature("customer_import");
        feature.resources.push(resource(
            "CustomerImportBatch",
            vec![file_field(
                "file",
                make_file_capability(
                    mb(25),
                    vec![("text", "csv")],
                    Some(FileVisibility::Private),
                    None,
                ),
            )],
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customer_import"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(!out.contains("\"time\""));
        assert!(out.contains("var customerImportFileFile = storage.FileContract{"));
        assert!(out.contains("Resource:   \"CustomerImportBatch\","));
        assert!(out.contains("Field:      \"file\","));
        assert!(out.contains("MaxSize:    25 * 1024 * 1024,"));
        assert!(
            out.contains("Accept:     []storage.MimeType{{Family: \"text\", Subtype: \"csv\"}},")
        );
        assert!(out.contains("Visibility: storage.VisibilityPrivate,"));
    }

    #[test]
    fn api_output_emits_signed_contract_and_time_import() {
        let mut feature = base_feature("customer");
        let cap = make_file_capability(
            mb(100),
            vec![("text", "csv")],
            Some(FileVisibility::Signed),
            Some("1h"),
        );
        feature.apis.push(api(
            "customer_export",
            TypeRef::Capability(CapabilityRef::File(cap)),
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("import (\n\t\"time\"\n\n\t\"lazuli.dev/runtime/lazuli/storage\"\n)"));
        assert!(out.contains("var customerCustomerExportFile = storage.FileContract{"));
        assert!(out.contains("API:        \"customer_export\","));
        assert!(out.contains("Visibility: storage.VisibilitySigned,"));
        assert!(out.contains("SignedTTL:  1 * time.Hour,"));
    }

    #[test]
    fn public_wildcard_accept_renders_mime_literals() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![file_field(
                "profile_photo",
                make_file_capability(
                    mb(5),
                    vec![("image", "*")],
                    Some(FileVisibility::Public),
                    None,
                ),
            )],
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("var customerProfilePhotoFile = storage.FileContract{"));
        assert!(out.contains("Resource:   \"Customer\","));
        assert!(out.contains("Field:      \"profile_photo\","));
        assert!(
            out.contains("Accept:     []storage.MimeType{{Family: \"image\", Subtype: \"*\"}},")
        );
        assert!(out.contains("Visibility: storage.VisibilityPublic,"));
    }

    #[test]
    fn deterministic_site_order_across_runs() {
        let mut feature = base_feature("docs");
        feature.resources.push(resource(
            "Zebra",
            vec![file_field(
                "z_file",
                make_file_capability(
                    mb(1),
                    vec![("text", "csv")],
                    Some(FileVisibility::Private),
                    None,
                ),
            )],
        ));
        feature.resources.push(resource(
            "Alpha",
            vec![file_field(
                "a_file",
                make_file_capability(
                    mb(1),
                    vec![("text", "csv")],
                    Some(FileVisibility::Private),
                    None,
                ),
            )],
        ));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
        let alpha_pos = a.find("Storage: Alpha.a_file").expect("alpha banner");
        let zebra_pos = a.find("Storage: Zebra.z_file").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, CapabilityRef, Defaults, Feature, Field, FileCapability, FileSize,
        FileSizeLiteral, FileVisibility, MimeType, Module, Policies, Resource, TypeRef,
    };

    fn minimal_app() -> AppManifest {
        AppManifest {
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
            span_ref: None,
        }
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn emit_entry_point(feature: &Feature) -> Option<String> {
        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        let index = CrossFeatureIndex::build(&module);
        emit_storage_file(
            "features/documents/documents.lzi",
            feature,
            "lazuli/test",
            &index,
        )
    }

    #[test]
    fn entry_point_emits_representative_resource_contract() {
        let mut feature = base_feature("documents");
        feature.resources.push(Resource {
            name: "Document".to_owned(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![Field {
                name: "attachment".to_owned(),
                type_ref: TypeRef::Capability(CapabilityRef::File(FileCapability {
                    max_size: FileSize {
                        bytes: FileSizeLiteral::Mb(10).bytes(),
                        literal: FileSizeLiteral::Mb(10),
                    },
                    accept: vec![MimeType {
                        family: "application".to_owned(),
                        subtype: "pdf".to_owned(),
                    }],
                    visibility: Some(FileVisibility::Signed),
                    signed_ttl: Some("30m".to_owned()),
                })),
                required: true,
                unique: false,
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                previous_names: Vec::new(),
                span_ref: None,
            }],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
        });

        let out = emit_entry_point(&feature).expect("typed file field must emit storage.gen.go");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package documents"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(out.contains("var documentsAttachmentFile = storage.FileContract{"));
        assert!(out.contains("Resource:   \"Document\","));
        assert!(out.contains("Field:      \"attachment\","));
        assert!(out.contains(
            "Accept:     []storage.MimeType{{Family: \"application\", Subtype: \"pdf\"}},"
        ));
        assert!(out.contains("SignedTTL:  30 * time.Minute,"));
    }
}
