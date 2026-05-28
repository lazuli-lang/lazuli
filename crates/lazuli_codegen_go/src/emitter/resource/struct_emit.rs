//! Struct + `Resource[T]` value emission for resources and records.
//!
//! `emit_resource` walks one `Resource`: implicit ID column, implicit
//! tenancy column, user-declared fields (with derived columns surfaced
//! as standalone comments), per-Money currency pair columns, implicit
//! timestamps, soft-delete sentinel, then the `lazuli.Resource[T]`
//! value literal with tenancy / soft-delete / timestamps / retention /
//! encryption metadata.
//!
//! `emit_record` is the lighter variant — typed struct only, no
//! resource value, no identity, no tenancy axis.
//!
//! Boundary: `write_section_banner` and the parent module's helpers
//! (`pascal_case`, `lower_camel`) live one level up so encryption
//! emission can share them.

use std::fmt::Write;

use lazuli_ir::{
    BuiltinType, ComputedDateBase, ComputedDateOffset, Feature, Record, Resource, Tenancy, TypeRef,
};

use crate::emitter::casing::{lower_camel, pascal_case};
use crate::emitter::printer::GoPrinter;
use crate::emitter::types::{self, TypeCtx};

use super::attributes::{
    db_col_for, effective_tenancy, is_secret_capability, plugin_semantic_validate_tag,
    retention_action_const, tenancy_const, uses_timestamps,
};
use super::encryption::{
    EncryptedFieldRef, emit_resource_value_encryption_fields, encrypted_fields,
};
use super::write_section_banner;

/// Walk a single `Resource` — struct + `lazuli.Resource[T]` value.
pub(super) fn emit_resource(
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
            if matches!(f.type_ref, TypeRef::Builtin(BuiltinType::SemanticCurrency)) {
                f.name.strip_suffix("_currency").map(|stem| stem.to_owned())
            } else {
                None
            }
        })
        .collect();
    for field in &resource.fields {
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
    // means "inherit feature default" — and now also "auto-detect from
    // explicit `created_at`+`updated_at` fields" (see `uses_timestamps`).
    //
    // Skip the auto-inject for either column the author already declared
    // explicitly. Without this guard a resource that engages the
    // convention via field declaration triggers `CreatedAt redeclared`
    // / `UpdatedAt redeclared` Go build errors because both the user-
    // field loop above AND this auto-inject would push the same field.
    if uses_timestamps(feature, resource) {
        let has_explicit_created_at = resource
            .fields
            .iter()
            .any(|f| f.name == "created_at");
        let has_explicit_updated_at = resource
            .fields
            .iter()
            .any(|f| f.name == "updated_at");
        if !has_explicit_created_at {
            tagged.push(TaggedField {
                name: "CreatedAt".to_owned(),
                go_type: "lazuli.Time".to_owned(),
                db_col: "created_at".to_owned(),
                json_suffix: "created_at".to_owned(),
                validate: None,
                comment: None,
            });
        }
        if !has_explicit_updated_at {
            tagged.push(TaggedField {
                name: "UpdatedAt".to_owned(),
                go_type: "lazuli.Time".to_owned(),
                db_col: "updated_at".to_owned(),
                json_suffix: "updated_at".to_owned(),
                validate: None,
                comment: None,
            });
        }
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

    // W3 GAP-03 — emit a `Compute<Field>` helper per `computed_date`
    // field. The runtime calls it before INSERT/UPDATE so the stored
    // `DATE` column carries `base + offset days`. Date math is wire-thin:
    // parse the base via `time.Parse`, `time.Time.AddDate(0, 0, offset)`,
    // re-format. See `docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`.
    emit_computed_date_helpers(p, &pascal, resource);
}

/// W3 GAP-03 — emit `Compute<Field>` methods for every `computed_date`
/// field on the resource. Each method recomputes the field in place via
/// `time.Time.AddDate(0, 0, <offset>)` (Go stdlib) so persistence carries
/// `base + (offset days)`. Wire-thin: parse + AddDate + format, no
/// homegrown calendar math. Emits nothing when no field is a
/// `computed_date` — resources without one stay free of the helper.
fn emit_computed_date_helpers(p: &mut GoPrinter, pascal: &str, resource: &Resource) {
    for field in &resource.fields {
        let Some(cd) = &field.computed_date else {
            continue;
        };
        let method = format!("Compute{}", pascal_case(&field.name));
        let dst = pascal_case(&field.name);
        let offset_expr = match &cd.offset {
            ComputedDateOffset::Field(name) => format!("int(m.{})", pascal_case(name)),
            ComputedDateOffset::Literal(n) => n.to_string(),
        };
        match &cd.base {
            // W3 GAP-03 — base is a same-resource `Date` field: parse it.
            ComputedDateBase::Field(base_field) => {
                let base = pascal_case(base_field);
                p.line("");
                p.line(&format!(
                    "// {method} recomputes {dst} as {base} + ({} days) — the",
                    offset_label(&cd.offset),
                ));
                p.line("// `computed_date` field kind. Wire-thin: time.Parse + AddDate + Format.");
                p.line(&format!("func (m *{pascal}) {method}() {{"));
                p.indent();
                // `lazuli.Date` is a string alias (RFC 3339 `YYYY-MM-DD`);
                // parse to time.Time for the arithmetic, then re-format.
                p.line(&format!(
                    "base, err := time.Parse(\"2006-01-02\", string(m.{base}))"
                ));
                p.line("if err != nil {");
                p.indent();
                p.line("return");
                p.dedent();
                p.line("}");
                p.line(&format!(
                    "m.{dst} = lazuli.Date(base.AddDate(0, 0, {offset_expr}).Format(\"2006-01-02\"))"
                ));
                p.dedent();
                p.line("}");
            }
            // W4 GAP-08 — base is selected by a bound `@fn` (the
            // `schedule_rule` form). The `@fn` returns the base `Date`
            // chosen by the rule arg; then the same AddDate arithmetic
            // applies. Wire-thin: registry lookup + AddDate + Format.
            ComputedDateBase::Rule { rule, fn_ref } => {
                p.line("");
                p.line(&format!(
                    "// {method} recomputes {dst} as @fn.{fn_ref}({rule}) + ({} days)",
                    offset_label(&cd.offset),
                ));
                p.line("// — the `schedule_rule` field kind. Wire-thin: bound @fn picks");
                p.line("// the base Date, then AddDate + Format.");
                p.line(&format!("func (m *{pascal}) {method}(rule string) {{"));
                p.indent();
                p.line(&format!(
                    "base, err := lazuli.ScheduleRuleDate(\"{fn_ref}\", rule)"
                ));
                p.line("if err != nil {");
                p.indent();
                p.line("return");
                p.dedent();
                p.line("}");
                p.line(&format!(
                    "m.{dst} = lazuli.Date(base.AddDate(0, 0, {offset_expr}).Format(\"2006-01-02\"))"
                ));
                p.dedent();
                p.line("}");
            }
        }
    }
}

/// Human-readable label for the offset operand, used in the helper's doc
/// comment.
fn offset_label(offset: &ComputedDateOffset) -> String {
    match offset {
        ComputedDateOffset::Field(name) => name.clone(),
        ComputedDateOffset::Literal(n) => n.to_string(),
    }
}

/// Walk a `Record` — typed struct only, no resource value. Records
/// carry no identity, tenancy, soft-delete, or retention axis.
pub(super) fn emit_record(p: &mut GoPrinter, record: &Record, ctx: &TypeCtx<'_>) {
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
