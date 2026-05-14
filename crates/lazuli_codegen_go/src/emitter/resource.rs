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

use std::fmt::Write;

use lazuli_ir::{
    BuiltinType, Feature, Record, RetentionAction, Resource, Tenancy, TypeRef,
};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

/// Emit `<feature>/resource.gen.go` for a feature, or `None` when the
/// feature declares no resources or records (so `module.rs` skips the
/// file entirely — gofmt would warn on an empty package body).
///
/// `module_name` and `cross_index` are threaded through so the
/// `types::go_type_for` resolver can lift cross-feature references
/// (e.g. a `customer.Customer` field that names a `User` declared in
/// the `org` feature emits `*org.User` plus the corresponding
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
    }
    for record in &records {
        for field in &record.fields {
            register_imports_for_type(&field.type_ref, &type_ctx, &mut imports);
        }
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for resource in &resources {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_resource(&mut p, feature, resource, &type_ctx);
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

/// Walk a single `Resource` — struct + `lazuli.Resource[T]` value.
fn emit_resource(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: &Resource,
    ctx: &TypeCtx<'_>,
) {
    let pascal = pascal_case(&resource.name);
    let var_name = format!("{}Resource", lower_camel(&resource.name));
    let resource_dsl_name = &resource.name;

    // Section banner mirrors the `runtime.rs` spike so generated files
    // remain visually scannable.
    write_section_banner(
        p,
        &[
            format!("Resource: {pascal}"),
            format!("  resource {pascal}"),
        ],
    );

    // Struct body — collect (name, type, db_col, json_tag_suffix)
    // tuples first, then column-align the rendered rows.
    let mut tagged: Vec<TaggedField> = Vec::new();

    // Implicit `ID` row — every resource carries an identity column.
    tagged.push(TaggedField {
        name: "ID".to_owned(),
        go_type: "lazuli.ID".to_owned(),
        db_col: "id".to_owned(),
        json_suffix: "id".to_owned(),
        comment: None,
    });

    // Implicit tenancy column. Feature `defaults` may pin tenancy and
    // resource may override; resolution mirrors §5.1.
    if matches!(effective_tenancy(feature, resource), Tenancy::Org) {
        tagged.push(TaggedField {
            name: "OrgID".to_owned(),
            go_type: "lazuli.ID".to_owned(),
            db_col: "org_id".to_owned(),
            json_suffix: "org_id".to_owned(),
            comment: None,
        });
    }

    // User-declared fields. `derived_from` columns are GENERATED ALWAYS
    // AS — they appear in DDL but not in the Go struct (cell I2 owns
    // the DDL surface). Emit a leading comment so the omission is
    // visible in review.
    for field in &resource.fields {
        if field.derived_from.is_some() {
            tagged.push(TaggedField {
                name: String::new(),
                go_type: String::new(),
                db_col: String::new(),
                json_suffix: String::new(),
                comment: Some(format!(
                    "// {} is derived (`derived from {}`); column lives in DDL only.",
                    pascal_case(&field.name),
                    field.derived_from.as_deref().unwrap_or("<expr>"),
                )),
            });
            continue;
        }
        let (go_type, _import) = types::go_type_for(&field.type_ref, ctx);
        let optional = !field.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", field.name)
        } else {
            field.name.clone()
        };
        let db_col = db_col_for(field, &field.type_ref);
        tagged.push(TaggedField {
            name: pascal_case(&field.name),
            go_type: final_type,
            db_col,
            json_suffix,
            comment: None,
        });
    }

    // Implicit timestamps. `Defaults.timestamps` is feature-wide;
    // `Resource.timestamps` overrides. `None` on the resource side
    // means "inherit feature default".
    if uses_timestamps(feature, resource) {
        tagged.push(TaggedField {
            name: "CreatedAt".to_owned(),
            go_type: "lazuli.Time".to_owned(),
            db_col: "created_at".to_owned(),
            json_suffix: "created_at".to_owned(),
            comment: None,
        });
        tagged.push(TaggedField {
            name: "UpdatedAt".to_owned(),
            go_type: "lazuli.Time".to_owned(),
            db_col: "updated_at".to_owned(),
            json_suffix: "updated_at".to_owned(),
            comment: None,
        });
    }

    // Soft delete sentinel — nullable; omitempty so default-encoded
    // JSON stays clean for active rows.
    if resource.soft_delete {
        tagged.push(TaggedField {
            name: "DeletedAt".to_owned(),
            go_type: "*lazuli.Time".to_owned(),
            db_col: "deleted_at".to_owned(),
            json_suffix: "deleted_at,omitempty".to_owned(),
            comment: None,
        });
    }

    p.line(&format!(
        "// {pascal} is the row materialised from the {pascal} resource. Each field"
    ));
    p.line("// carries `db` and `json` tags for pgx scan and HTTP encode respectively.");
    p.line(&format!("type {pascal} struct {{"));
    p.indent();
    write_struct_rows(p, &tagged);
    p.dedent();
    p.line("}");
    p.blank();

    // Resource[T] value. Keys are column-aligned so the value literal
    // reads as a stable table; `SoftDelete` is the longest key in the
    // closed catalog so we pad to its width.
    p.line(&format!(
        "var {var_name} = lazuli.Resource[{pascal}]{{"
    ));
    p.indent();
    let tenancy_const = tenancy_const(effective_tenancy(feature, resource));
    let mut kv_rows: Vec<(String, String)> = vec![
        ("Name:".to_owned(), format!("\"{resource_dsl_name}\",")),
        ("Feature:".to_owned(), format!("\"{}\",", feature.name)),
        ("Tenancy:".to_owned(), format!("{tenancy_const},")),
    ];
    if resource.soft_delete {
        kv_rows.push(("SoftDelete:".to_owned(), "true,".to_owned()));
    }
    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    if let Some(retention) = &resource.retention {
        p.line("Retention: &lazuli.RetentionSpec{");
        p.indent();
        p.line(&format!(
            "Window: lazuli.Duration(\"{}\"),",
            retention.duration
        ));
        let action = retention_action_const(retention.action);
        p.line(&format!("Then:   {action},"));
        p.dedent();
        p.line("},");
    }
    p.dedent();
    p.line("}");
}

/// Walk a `Record` — typed struct only, no resource value. Records
/// carry no identity, tenancy, soft-delete, or retention axis.
fn emit_record(p: &mut GoPrinter, record: &Record, ctx: &TypeCtx<'_>) {
    let pascal = pascal_case(&record.name);
    write_section_banner(
        p,
        &[
            format!("Record: {pascal}"),
            format!("  record {pascal}"),
        ],
    );

    let mut tagged: Vec<TaggedField> = Vec::new();
    for field in &record.fields {
        if field.derived_from.is_some() {
            tagged.push(TaggedField {
                name: String::new(),
                go_type: String::new(),
                db_col: String::new(),
                json_suffix: String::new(),
                comment: Some(format!(
                    "// {} is derived (`derived from {}`).",
                    pascal_case(&field.name),
                    field.derived_from.as_deref().unwrap_or("<expr>"),
                )),
            });
            continue;
        }
        let (go_type, _import) = types::go_type_for(&field.type_ref, ctx);
        let optional = !field.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", field.name)
        } else {
            field.name.clone()
        };
        let db_col = db_col_for(field, &field.type_ref);
        tagged.push(TaggedField {
            name: pascal_case(&field.name),
            go_type: final_type,
            db_col,
            json_suffix,
            comment: None,
        });
    }

    p.line(&format!(
        "// {pascal} is a typed value record (proposal §3.1)."
    ));
    p.line(&format!("type {pascal} struct {{"));
    p.indent();
    write_struct_rows(p, &tagged);
    p.dedent();
    p.line("}");
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Decoded form of one struct row. Comments interleave between rows;
/// rendering uses `aligned_struct_rows` for the data rows so the
/// `db`/`json` tag columns line up.
struct TaggedField {
    name: String,
    go_type: String,
    db_col: String,
    json_suffix: String,
    /// When set, render as a standalone comment line — used for
    /// `derived from` columns that live in DDL only.
    comment: Option<String>,
}

/// Emit the struct body. Every data row pulls its column widths from
/// a single global pass over the typed fields so the `name`, `type`,
/// and `tag` columns stay aligned even when a stand-alone comment row
/// interrupts the visual block (gofmt resets alignment across comments
/// — we keep it consistent to mirror the runtime-spike fixture).
fn write_struct_rows(p: &mut GoPrinter, tagged: &[TaggedField]) {
    // Two-pass: build rendered rows first so we can compute global
    // name/type/tag widths, then emit with `aligned_struct_rows` for
    // contiguous data slices (comments interrupt the slice, but each
    // slice is rendered with the *global* widths so columns line up
    // across the comment line).
    let max_db_width = tagged
        .iter()
        .filter(|f| f.comment.is_none())
        .map(|f| db_segment(&f.db_col).len())
        .max()
        .unwrap_or(0);

    enum Row {
        Data { name: String, ty: String, tag: String },
        Comment(String),
    }

    let rows: Vec<Row> = tagged
        .iter()
        .map(|field| match &field.comment {
            Some(comment) => Row::Comment(comment.clone()),
            None => Row::Data {
                name: field.name.clone(),
                ty: field.go_type.clone(),
                tag: build_tag(&field.db_col, &field.json_suffix, max_db_width),
            },
        })
        .collect();

    let name_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Data { name, .. } => Some(name.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let ty_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Data { ty, .. } => Some(ty.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    for row in &rows {
        match row {
            Row::Comment(text) => p.line(text),
            Row::Data { name, ty, tag } => {
                let mut scratch = String::with_capacity(name_width + ty_width + tag.len() + 4);
                let _ = write!(
                    scratch,
                    "{name:<name_width$} {ty:<ty_width$} {tag}",
                    name = name,
                    name_width = name_width,
                    ty = ty,
                    ty_width = ty_width,
                    tag = tag,
                );
                p.line(&scratch);
            }
        }
    }
}

/// Format the `db:"…"` token segment (no surrounding back-ticks). Used
/// to compute the alignment width for the json column.
fn db_segment(db_col: &str) -> String {
    format!("db:\"{}\"", db_col)
}

/// Build a column-aligned tag string padding the `db:"…"` portion so
/// the `json:` token aligns across the struct. Mirrors the pattern
/// proven in `runtime.rs:664-682`.
fn build_tag(db_col: &str, json_suffix: &str, max_db_width: usize) -> String {
    let db_part = db_segment(db_col);
    let pad = max_db_width.saturating_sub(db_part.len());
    format!(
        "`{}{} json:\"{}\"`",
        db_part,
        " ".repeat(pad),
        json_suffix
    )
}

/// Resolve effective tenancy by combining feature defaults with the
/// resource-level override. Resource override wins; if both unset we
/// default to `Tenancy::None` because the registered Lazuli Go lib
/// only recognises `TenancyOrg`/`TenancyNone` today.
fn effective_tenancy(feature: &Feature, resource: &Resource) -> Tenancy {
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
fn uses_timestamps(feature: &Feature, resource: &Resource) -> bool {
    resource
        .timestamps
        .unwrap_or(feature.defaults.timestamps)
}

/// Lift a `Tenancy` IR variant onto the `lazuli.Tenancy*` constant
/// name. The Lazuli Go lib carries `TenancyNone`/`TenancyOrg`/
/// `TenancyTeam`/`TenancyCustom` (the last is a structural marker
/// — the per-resource custom axis name is not yet threaded through
/// `Resource[T]`; the runtime widens this when a pilot product
/// needs per-row custom-axis scoping).
fn tenancy_const(tenancy: Tenancy) -> &'static str {
    match tenancy {
        Tenancy::Org => "lazuli.TenancyOrg",
        Tenancy::Team => "lazuli.TenancyTeam",
        Tenancy::Custom(_) => "lazuli.TenancyCustom",
        Tenancy::None => "lazuli.TenancyNone",
    }
}

fn retention_action_const(action: RetentionAction) -> &'static str {
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
fn db_col_for(field: &lazuli_ir::Field, type_ref: &TypeRef) -> String {
    if is_geo_point(type_ref) {
        return format!("{},type:geography(point,4326)", field.name);
    }
    field.name.clone()
}

fn is_geo_point(type_ref: &TypeRef) -> bool {
    matches!(
        type_ref,
        TypeRef::Builtin(BuiltinType::SemanticGeoPoint)
    )
}

/// Register every import surfaced by a `TypeRef` onto the file-level
/// `ImportSet`. Recurses into `Many<T>` so wrapping a typed `T`
/// carries its import through. Cross-feature refs surface here as
/// `<module>/<feature>` paths and route through `ImportSet::add`'s
/// "third-party" bucket (the classifier doesn't know the difference,
/// but the `import` block still compiles — Go doesn't care which
/// group a local module sits in).
fn register_imports_for_type(
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

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

// Delegate to the shared `casing` module so resource/record names use
// the same word-splitting + acronym rules as every other emitter. The
// previous local copy split only on `_`/`-`, missing case transitions
// (`ApiKey` stayed one word, then emitted as `ApiKey`), which diverged
// from `query.rs` / `command.rs` references to the same type (rendered
// as `APIKey` after their split-on-case-transition + `api` acronym).
use super::casing::{lower_camel, pascal_case};

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, CapabilityRef, Defaults, EncryptedCapability, Feature, Field, Module,
        Policies, Record, Resource, RetentionAction, RetentionSpec, Tenancy, TypeRef,
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
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
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
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn simple_field(name: &str, builtin: BuiltinType, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(builtin),
            required,
            unique: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn simple_resource(name: &str, fields: Vec<Field>) -> Resource {
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
        assert!(out.contains("package customer"));
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
    fn record_emits_struct_without_resource_value() {
        let mut feature = base_feature("customer");
        feature.records.push(Record {
            name: "CustomerLtv".to_owned(),
            fields: vec![
                simple_field("customer_id", BuiltinType::Id, true),
                simple_field("amount", BuiltinType::SemanticMoney, true),
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
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                previous_names: Vec::new(),
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
            vec![simple_field("location", BuiltinType::SemanticGeoPoint, true)],
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
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                previous_names: Vec::new(),
                span_ref: None,
            }],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("ExternalID lazuli.EncryptedRef"));
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
        // to `*org.User` plus an `lazuli/test/org` import.
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
                default: None,
                derived_from: None,
                constraints: lazuli_ir::FieldConstraints::default(),
                previous_names: Vec::new(),
                span_ref: None,
            }],
        );
        customer.resources.push(resource);

        let mut org = base_feature("org");
        org.resources
            .push(simple_resource("user", Vec::new()));
        // The org resource is named lowercase; pascal-case the
        // emitter-side declared type still appears as `User` in
        // the index because `CrossFeatureIndex` keys on
        // `Resource.name` verbatim. So we build the index against
        // an explicit `User` resource.
        org.resources.clear();
        org.resources.push(Resource {
            name: "User".to_owned(),
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
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            features: vec![customer.clone(), org],
        };
        let index = CrossFeatureIndex::build(&module);
        let out = emit_resource_file("examples/x.lzi", &customer, "lazuli/test", &index)
            .expect("must emit");

        assert!(
            out.contains("Owner *org.User"),
            "expected `*org.User` cross-feature ref, got:\n{out}"
        );
        assert!(
            out.contains("\"lazuli/test/org\""),
            "expected cross-feature import `lazuli/test/org`, got:\n{out}"
        );
    }
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{AppManifest, Defaults, Field, Module, Policies, Resource};

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
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: vec![Resource {
                name: "item".to_owned(),
                tenancy: None,
                soft_delete: false,
                timestamps: None,
                fields: vec![Field {
                    name: "sku".to_owned(),
                    type_ref: TypeRef::Builtin(BuiltinType::Text),
                    required: true,
                    unique: true,
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
            }],
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
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
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
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
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
        assert!(out.contains("package inventory"));
        assert!(out.contains("type Item struct {"));
        assert!(out.contains("Sku string"));
        assert!(out.contains("var itemResource = lazuli.Resource[Item]{"));
    }
}
