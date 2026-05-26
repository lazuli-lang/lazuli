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

use super::casing::pascal_case;
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod feature_emit_tests;

/// Emit `<feature>/storage.gen.go` for a feature, or `None` when the
/// feature declares no typed `@cap.File(...)` resource fields or API
/// outputs.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_storage_file("photoshare.lzi", &feature, "demo", &cross_index);
/// ```
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

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
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
    // Cross-package handlers (`app/features/<feature>/handlers/`,
    // package `<feature>handlers`) reference the contract value via
    // `<feature>gen.<Feature><Field>File`. Go exports identifiers
    // beginning with an uppercase letter, so the var name MUST be
    // PascalCase — lower-camel names are package-private and the
    // hand-authored handler cannot reach them. Same fix shape as
    // commit c1ec2ba (auth/webhook contracts).
    let suffix_source = match site {
        StorageSite::ResourceField { field, .. } => field.name.as_str(),
        StorageSite::ApiOutput { api, .. } => api.name.as_str(),
    };
    format!(
        "{}{}File",
        pascal_case(&feature.name),
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

