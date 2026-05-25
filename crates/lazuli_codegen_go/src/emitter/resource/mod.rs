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
    BuiltinType, CapabilityRef, Feature, Field, Record, Resource, RetentionAction, Tenancy, TypeRef,
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

/// Walk a single `Resource` — struct + `lazuli.Resource[T]` value.
fn emit_resource(p: &mut GoPrinter, feature: &Feature, resource: &Resource, ctx: &TypeCtx<'_>) {
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
        validate: None,
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
            validate: None,
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
                validate: None,
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
        // Secret-bearing capability fields (`@cap.Hashed/Encrypted/E2ee/
        // Token/Secret`) never appear in JSON output. The server stores
        // them; the wire MUST NOT carry the hash/ciphertext/token to
        // clients. `json:"-"` is Go stdlib's "skip" sentinel — `omitempty`
        // is not enough because a non-zero value would still serialize.
        let json_suffix = if is_secret_capability(&field.type_ref) {
            "-".to_owned()
        } else if optional {
            format!("{},omitempty", field.name)
        } else {
            field.name.clone()
        };
        let db_col = db_col_for(field, &field.type_ref);
        // B3 — plugin-contributed `@semantic.<Name>` carries the
        // declaring plugin + validator function; surface as a
        // `validate:"<plugin.name>.<validator>"` tag clause. The
        // validator key is decoded by the runtime dispatcher to
        // invoke the plugin adapter at write/read boundaries. See
        // `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
        let validate = plugin_semantic_validate_tag(&field.type_ref);
        tagged.push(TaggedField {
            name: pascal_case(&field.name),
            go_type: final_type,
            db_col,
            json_suffix,
            validate,
            comment: None,
        });
    }

    // MONEY-1 §3.2 (v0.5) — per-field `<field>_currency` Go fields
    // mirror the per-field DDL columns emitted by `migration_ddl.rs`.
    // Authors can suppress the auto-emit by declaring an explicit
    // `<field>_currency: Currency` of their own (back-compat for
    // hand-rolled migrations). The legacy v0.4 single shared `currency`
    // column has been retired now that IR carries currency per Money
    // field — see `BuiltinType::SemanticMoney { currency }`.
    let explicit_currency_overrides: std::collections::HashSet<String> = resource
        .fields
        .iter()
        .filter_map(|f| {
            use lazuli_ir::{BuiltinType, TypeRef};
            if matches!(f.type_ref, TypeRef::Builtin(BuiltinType::SemanticCurrency)) {
                f.name.strip_suffix("_currency").map(|stem| stem.to_owned())
            } else {
                None
            }
        })
        .collect();
    for field in &resource.fields {
        use lazuli_ir::{BuiltinType, TypeRef};
        let TypeRef::Builtin(BuiltinType::SemanticMoney { .. }) = field.type_ref else {
            continue;
        };
        if explicit_currency_overrides.contains(&field.name) {
            continue;
        }
        let pair_name = format!("{}_currency", field.name);
        let (go_type, json_suffix) = if field.required {
            ("lazuli.Currency".to_owned(), pair_name.clone())
        } else {
            (
                "*lazuli.Currency".to_owned(),
                format!("{},omitempty", pair_name),
            )
        };
        tagged.push(TaggedField {
            name: pascal_case(&pair_name),
            go_type,
            db_col: pair_name,
            json_suffix,
            validate: None,
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
            validate: None,
            comment: None,
        });
        tagged.push(TaggedField {
            name: "UpdatedAt".to_owned(),
            go_type: "lazuli.Time".to_owned(),
            db_col: "updated_at".to_owned(),
            json_suffix: "updated_at".to_owned(),
            validate: None,
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
            validate: None,
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
    p.line(&format!("var {var_name} = lazuli.Resource[{pascal}]{{"));
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
    // `Timestamps: true` mirrors the same predicate the column emitter
    // and the migration DDL use (`uses_timestamps`). The runtime gates
    // the `"updated_at" = now()` SET-clause append on this flag — when
    // the resource opts out of timestamps the column is absent and an
    // unconditional bump would raise PG 42703.
    if uses_timestamps(feature, resource) {
        kv_rows.push(("Timestamps:".to_owned(), "true,".to_owned()));
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
    // Encryption wiring — emit the column→scope map and the
    // typed-`*<Pascal>` Decrypt callback so the runtime can encrypt
    // bindings before INSERT/UPDATE and decrypt scanned rows after
    // RETURNING * / SELECT (per `docs/proposals/encryption-vocab.md`
    // §Runtime + §Codegen). Skipped entirely when the resource has no
    // encrypted fields so resources without encryption stay free of
    // the metadata.
    let encrypted_for_value: Vec<EncryptedFieldRef<'_>> = encrypted_fields(resource).collect();
    if !encrypted_for_value.is_empty() {
        emit_resource_value_encryption_fields(p, &pascal, &encrypted_for_value);
    }
    p.dedent();
    p.line("}");
}

/// Emit the `EncryptedColumns` + `Decrypt` fields on a `Resource[T]`
/// value literal. Both fields are typed against the runtime's
/// `Resource[T]` shape; the runtime uses them to wire the call sites
/// in `applyCreates`/`applyUpdates`/`applyDeletes`, `RunList`, and
/// `RunLookup`. Decrypt is a typed wrapper around the generated
/// `Decrypt<Pascal>` helper so the callback's `any` parameter type
/// matches the runtime's `func(*lazuli.Ctx, any) error` field while
/// the body stays typed against `*<Pascal>`.
fn emit_resource_value_encryption_fields(
    p: &mut GoPrinter,
    pascal: &str,
    encrypted: &[EncryptedFieldRef<'_>],
) {
    p.line("EncryptedColumns: map[string]string{");
    p.indent();
    // Walk in declared order — `encrypted_fields` already preserves
    // the IR `Vec` ordering so the emitted map literal is deterministic.
    for entry in encrypted {
        let col = &entry.field.name;
        let key = &entry.key;
        p.line(&format!("\"{col}\": \"{key}\","));
    }
    p.dedent();
    p.line("},");
    p.line(&format!(
        "Decrypt: func(ctx *lazuli.Ctx, row any) error {{ return Decrypt{pascal}(ctx, row.(*{pascal})) }},"
    ));
}

/// Walk a `Record` — typed struct only, no resource value. Records
/// carry no identity, tenancy, soft-delete, or retention axis.
fn emit_record(p: &mut GoPrinter, record: &Record, ctx: &TypeCtx<'_>) {
    let pascal = pascal_case(&record.name);
    write_section_banner(
        p,
        &[format!("Record: {pascal}"), format!("  record {pascal}")],
    );

    let mut tagged: Vec<TaggedField> = Vec::new();
    for field in &record.fields {
        if field.derived_from.is_some() {
            tagged.push(TaggedField {
                name: String::new(),
                go_type: String::new(),
                db_col: String::new(),
                json_suffix: String::new(),
                validate: None,
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
        // Secret-bearing capability fields (`@cap.Hashed/Encrypted/E2ee/
        // Token/Secret`) never appear in JSON output. The server stores
        // them; the wire MUST NOT carry the hash/ciphertext/token to
        // clients. `json:"-"` is Go stdlib's "skip" sentinel — `omitempty`
        // is not enough because a non-zero value would still serialize.
        let json_suffix = if is_secret_capability(&field.type_ref) {
            "-".to_owned()
        } else if optional {
            format!("{},omitempty", field.name)
        } else {
            field.name.clone()
        };
        let db_col = db_col_for(field, &field.type_ref);
        // B3 — record fields participate in the same plugin-semantic
        // validate-tag emission as resource fields. The shape carries
        // through (the Go validator runtime reads identical tags from
        // any struct).
        let validate = plugin_semantic_validate_tag(&field.type_ref);
        tagged.push(TaggedField {
            name: pascal_case(&field.name),
            go_type: final_type,
            db_col,
            json_suffix,
            validate,
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
// Encryption helpers (proposal `docs/proposals/encryption-vocab.md` §Codegen)
// ----------------------------------------------------------------------------

/// Field-encryption metadata extracted from `@cap.Encrypted` /
/// `@cap.E2ee` field decorators. The codegen threads `key` (the
/// `@key.<scope>` reference) through to `encryption.ForCtx(...)` at
/// the SQL boundary; `e2ee` flags fields the server stores but cannot
/// later decrypt — those get an Encrypt call site but are skipped on
/// Decrypt per proposal §Codegen rule 3.
struct EncryptedFieldRef<'a> {
    field: &'a Field,
    key: &'a str,
    e2ee: bool,
}

/// Iterator over a resource's `@cap.Encrypted` / `@cap.E2ee` fields,
/// preserving the IR `Vec` order so emission is deterministic. Used
/// both for the import-side gate (`imports.add("…/encryption")`) and
/// for the helper-function body.
fn encrypted_fields(resource: &Resource) -> impl Iterator<Item = EncryptedFieldRef<'_>> {
    resource
        .fields
        .iter()
        .filter_map(|field| match &field.type_ref {
            TypeRef::Capability(CapabilityRef::Encrypted(cap)) => Some(EncryptedFieldRef {
                field,
                key: cap.key.as_str(),
                e2ee: false,
            }),
            TypeRef::Capability(CapabilityRef::E2ee(cap)) => Some(EncryptedFieldRef {
                field,
                key: cap.key.as_str(),
                e2ee: true,
            }),
            _ => None,
        })
}

/// Emit `Encrypt<Pascal>` and `Decrypt<Pascal>` helpers for one
/// resource. The helpers operate in-place on `*<Pascal>` (the struct
/// emitted above) so callers can wrap them around their SQL
/// boundary without copying rows.
///
/// Wire-thin: the body of each helper is, per field,
/// `encryption.ForCtx(ctx, "@key.<scope>", "")` then `cipher.Encrypt(...)`
/// or `cipher.Decrypt(...)`. Zero crypto knowledge in codegen.
///
/// Decrypt skips `@cap.E2ee` fields (proposal §Codegen rule 3 — the
/// server stores ciphertext and the user holds the only key). The
/// `_ = ctx` no-op at the top of `Decrypt<Pascal>` keeps the
/// function signature uniform when every field is E2ee and the
/// body is otherwise empty.
fn emit_encryption_helpers(p: &mut GoPrinter, resource: &Resource) {
    let pascal = pascal_case(&resource.name);
    let encrypted: Vec<EncryptedFieldRef<'_>> = encrypted_fields(resource).collect();

    write_section_banner(
        p,
        &[
            format!("Encryption helpers: {pascal}"),
            "  one Encrypt/Decrypt call per @cap.Encrypted / @cap.E2ee field".to_owned(),
        ],
    );

    // Encrypt — covers every `@cap.Encrypted` AND `@cap.E2ee` field.
    p.line(&format!(
        "// Encrypt{pascal} ciphers each `@cap.Encrypted` / `@cap.E2ee` field on row in"
    ));
    p.line("// place. Call this immediately before INSERT / UPDATE so the row written");
    p.line("// to the database holds opaque AES-256-GCM bytes (BYTEA column, see");
    p.line("// migration_ddl.rs). The cipher is resolved per request via");
    p.line("// `encryption.ForCtx(ctx, scope, \"\")`; the runtime registry caches the");
    p.line("// derived key under (scope, resolved-template).");
    p.line("//lazuli:pattern resource_encrypt v1");
    p.line(&format!(
        "func Encrypt{pascal}(ctx *lazuli.Ctx, row *{pascal}) error {{"
    ));
    p.indent();
    for entry in &encrypted {
        emit_field_encrypt(p, entry);
    }
    p.line("return nil");
    p.dedent();
    p.line("}");
    p.blank();

    // Decrypt — skips E2ee per proposal §Codegen rule 3.
    let decryptable: Vec<&EncryptedFieldRef<'_>> = encrypted.iter().filter(|e| !e.e2ee).collect();
    p.line(&format!(
        "// Decrypt{pascal} undoes Encrypt{pascal} for every server-readable encrypted"
    ));
    p.line("// field after a SELECT / RETURNING. `@cap.E2ee` fields are intentionally");
    p.line("// skipped (the server stores ciphertext but cannot decrypt — the client");
    p.line("// holds the only key). Callers must check the returned error.");
    p.line("//lazuli:pattern resource_decrypt v1");
    p.line(&format!(
        "func Decrypt{pascal}(ctx *lazuli.Ctx, row *{pascal}) error {{"
    ));
    p.indent();
    if decryptable.is_empty() {
        p.line("// Every encrypted field on this resource is @cap.E2ee; nothing to decrypt.");
        p.line("_ = ctx");
        p.line("_ = row");
    } else {
        for entry in &decryptable {
            emit_field_decrypt(p, entry);
        }
    }
    p.line("return nil");
    p.dedent();
    p.line("}");
}

/// Emit one Encrypt call site. Optional fields are guarded by a
/// `if row.<F> != nil && len(*row.<F>) > 0` check so empty / unset
/// values never round-trip through the cipher (proposal §Codegen
/// rule 4). Required fields use `len(row.<F>) > 0` for the same
/// reason — an unset required field still has the zero `[]byte`
/// header.
fn emit_field_encrypt(p: &mut GoPrinter, entry: &EncryptedFieldRef<'_>) {
    emit_field_cipher_call(p, entry, "Encrypt", "ct");
}

/// Mirror of `emit_field_encrypt` for the read path.
fn emit_field_decrypt(p: &mut GoPrinter, entry: &EncryptedFieldRef<'_>) {
    emit_field_cipher_call(p, entry, "Decrypt", "pt");
}

fn emit_field_cipher_call(
    p: &mut GoPrinter,
    entry: &EncryptedFieldRef<'_>,
    method: &str,
    out_var: &str,
) {
    let pascal_field = pascal_case(&entry.field.name);
    let key_literal = go_str_literal(entry.key);
    let optional = !entry.field.required;
    let (guard, access) = if optional {
        (
            format!("row.{pascal_field} != nil && len(*row.{pascal_field}) > 0"),
            format!("(*row.{pascal_field})"),
        )
    } else {
        (
            format!("len(row.{pascal_field}) > 0"),
            format!("row.{pascal_field}"),
        )
    };
    p.line(&format!("if {guard} {{"));
    p.indent();
    p.line(&format!(
        "cipher, err := encryption.ForCtx(ctx, {key_literal}, \"\")"
    ));
    p.line("if err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("{out_var}, err := cipher.{method}({access})"));
    p.line("if err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("{access} = {out_var}"));
    p.dedent();
    p.line("}");
}

/// Minimal Go string-literal renderer for the `@key.<scope>` key
/// reference. The scope alphabet is `[a-z._@]` so we never hit
/// quote-escaping cases, but we keep the helper conservative.
fn go_str_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
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
    /// B3 — when set, append `validate:"<value>"` to the tag. Used for
    /// plugin-contributed semantic types: the value is
    /// `<plugin.name>.<validator>` (e.g. `scalars-br.ValidateCPF`).
    /// The runtime dispatcher reads this tag to invoke the plugin
    /// adapter's exported validator function.
    /// See `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
    validate: Option<String>,
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
        Data {
            name: String,
            ty: String,
            tag: String,
        },
        Comment(String),
    }

    let rows: Vec<Row> = tagged
        .iter()
        .map(|field| match &field.comment {
            Some(comment) => Row::Comment(comment.clone()),
            None => Row::Data {
                name: field.name.clone(),
                ty: field.go_type.clone(),
                tag: build_tag(
                    &field.db_col,
                    &field.json_suffix,
                    field.validate.as_deref(),
                    max_db_width,
                ),
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
/// proven in `runtime.rs:664-682`. When `validate` is set, a
/// `validate:"<value>"` clause follows the `json:` clause; this is
/// the B3 plugin-semantic dispatch tag.
fn build_tag(
    db_col: &str,
    json_suffix: &str,
    validate: Option<&str>,
    max_db_width: usize,
) -> String {
    let db_part = db_segment(db_col);
    let pad = max_db_width.saturating_sub(db_part.len());
    match validate {
        Some(v) => format!(
            "`{}{} json:\"{}\" validate:\"{}\"`",
            db_part,
            " ".repeat(pad),
            json_suffix,
            v
        ),
        None => format!("`{}{} json:\"{}\"`", db_part, " ".repeat(pad), json_suffix),
    }
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
    resource.timestamps.unwrap_or(feature.defaults.timestamps)
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
fn plugin_semantic_validate_tag(type_ref: &TypeRef) -> Option<String> {
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
fn is_secret_capability(type_ref: &TypeRef) -> bool {
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
fn register_imports_for_type(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
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

    // -----------------------------------------------------------------
    // Secret-bearing capability fields must not appear in JSON output.
    // A leaking `password_hash` or token in a generic `json.Marshal`
    // response would be a credential disclosure; the codegen carves
    // these out with `json:"-"` regardless of required/optional axis.
    // -----------------------------------------------------------------

    fn hashed_field(name: &str, required: bool) -> Field {
        use lazuli_ir::{HashAlgorithm, HashedCapability};
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
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

    fn encrypted_field(name: &str, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                key: "@key.tenant".to_owned(),
            })),
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

    fn token_field(name: &str, required: bool) -> Field {
        use lazuli_ir::{TokenCapability, TokenStore};
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::Token(TokenCapability {
                ttl: "24h".to_owned(),
                single_use: false,
                store: TokenStore::Hashed,
            })),
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

    fn e2ee_field(name: &str, required: bool) -> Field {
        use lazuli_ir::E2eeCapability;
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
                key: "@key.user".to_owned(),
            })),
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

    #[test]
    fn hashed_field_emits_json_skip_sentinel() {
        let mut feature = base_feature("account");
        let resource = simple_resource("user", vec![hashed_field("password_hash", true)]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("json:\"-\""),
            "expected json:\"-\" carve-out for @cap.Hashed field, got:\n{out}"
        );
        assert!(
            !out.contains("json:\"password_hash\""),
            "hashed field MUST NOT leak via JSON name, got:\n{out}"
        );
    }

    #[test]
    fn encrypted_field_emits_json_skip_sentinel() {
        let mut feature = base_feature("customer");
        let resource = simple_resource("customer", vec![encrypted_field("external_id", true)]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("json:\"-\""),
            "expected json:\"-\" carve-out for @cap.Encrypted field, got:\n{out}"
        );
        assert!(!out.contains("json:\"external_id\""));
    }

    #[test]
    fn e2ee_field_emits_json_skip_sentinel() {
        let mut feature = base_feature("customer");
        let resource = simple_resource("customer", vec![e2ee_field("private_note", true)]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("json:\"-\""),
            "expected json:\"-\" carve-out for @cap.E2ee field, got:\n{out}"
        );
        assert!(!out.contains("json:\"private_note\""));
    }

    #[test]
    fn token_field_emits_json_skip_sentinel() {
        let mut feature = base_feature("auth");
        let resource = simple_resource("session", vec![token_field("refresh_token", true)]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("json:\"-\""),
            "expected json:\"-\" carve-out for @cap.Token field, got:\n{out}"
        );
        assert!(!out.contains("json:\"refresh_token\""));
    }

    #[test]
    fn legacy_cap_secret_emits_json_skip_sentinel() {
        let mut feature = base_feature("auth");
        let resource = simple_resource(
            "credential",
            vec![simple_field("api_key", BuiltinType::CapSecret, true)],
        );
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("json:\"-\""),
            "expected json:\"-\" carve-out for @cap.Secret (legacy) field, got:\n{out}"
        );
    }

    #[test]
    fn optional_hashed_field_still_skips_json() {
        // `json:"-"` must beat `,omitempty` — an optional hashed value
        // is still a secret if present.
        let mut feature = base_feature("account");
        let resource = simple_resource("user", vec![hashed_field("password_hash", false)]);
        feature.resources.push(resource);
        let out = emit(&feature).expect("must emit");
        assert!(out.contains("json:\"-\""));
        assert!(!out.contains("password_hash,omitempty"));
    }

    #[test]
    fn cap_file_field_keeps_json_tag() {
        // `@cap.File` is a public handle to storage, not a secret —
        // the JSON tag must remain so clients can address uploads.
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
        assert!(out.contains("json:\"blob\""));
        assert!(!out.contains("json:\"-\""));
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
