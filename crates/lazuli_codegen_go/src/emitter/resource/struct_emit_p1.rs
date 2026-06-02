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
        // B3 + GAP-R2 — merge the plugin-semantic dispatch key with the
        // inline `FieldConstraints` (min/max/pattern/…) + `required` flag.
        // Plugin-contributed `@semantic.<Name>` surfaces
        // `<plugin.name>.<validator>` for the runtime adapter dispatcher;
        // constraints project to `go-playground/validator` keywords. Both
        // ride the same `validate:"…"` clause. See `field_validate_tag`.
        let validate = field_validate_tag(field);
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
        // Spec 0015 — `soft_delete by` projects a nullable `deleted_by`
        // actor column (`ID`) alongside `deleted_at`. The runtime stamps
        // it from `ctx.actor` on the soft-delete write (mirroring
        // `deleted_at = now()`). Nullable + omitempty: live rows carry no
        // deleter, matching the hand-rolled `deleted_by: ID optional`
        // pairs this trait folds in.
        if resource.soft_delete_actor {
            tagged.push(TaggedField {
                name: "DeletedBy".to_owned(),
                go_type: "*lazuli.ID".to_owned(),
                db_col: "deleted_by".to_owned(),
                json_suffix: "deleted_by,omitempty".to_owned(),
                validate: None,
                comment: None,
            });
        }
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
    // Spec 0015 — `SoftDeleteActor: true` tells the runtime soft-delete
    // path to additionally stamp `"deleted_by" = $actor` in the SET
    // clause (mirroring `"deleted_at" = now()`). Gated on the actor form
    // so bare `soft_delete` resources never reference a non-existent
    // `deleted_by` column.
    if resource.soft_delete_actor {
        kv_rows.push(("SoftDeleteActor:".to_owned(), "true,".to_owned()));
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
    // HTML-sanitization wiring — emit the column→profile map so the
    // runtime can rewrite each bound string value through the matching
    // bluemonday policy at the write boundary (`applyCreates` /
    // `applyUpdates`). Skipped entirely when the resource declares no
    // `validate sanitize_html(<profile>)` field. Closes the previously
    // no-op `sanitize_html` constraint (stored-XSS hole).
    let sanitized_for_value: Vec<SanitizedFieldRef<'_>> = sanitized_fields(resource).collect();
    if !sanitized_for_value.is_empty() {
        emit_resource_value_sanitize_fields(p, &sanitized_for_value);
    }
    // W1-2 SEC-FIELDPOLICY-READ-NULL — emit per-column read policies from
    // the feature's `policies fields <R>` block so the runtime read paths
    // (`RunList` / `RunLookup`) can null out columns the active actor may
    // not read (e.g. `password_hash read: @actor.system`) instead of
    // pulling them via `SELECT *`. Skipped when the resource declares no
    // runtime-evaluable field read policy. See `field_policy.rs`.
    emit_resource_value_field_read_policies(p, feature, resource);
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
        // B3 + GAP-R2 — record fields participate in the SAME merged
        // validate-tag emission as resource fields: plugin-semantic
        // dispatch key + inline `FieldConstraints` + `required`. A
        // `Many<Record>` JSONB collection thus carries per-element field
        // validation (e.g. `percentage: @semantic.Percentage` enforces
        // 0..=100 via the `lazuli.Percentage` carrier; `min`/`max`
        // constraints ride the `validate:"…"` tag). The Go validator
        // runtime reads identical tags from any struct.
        let validate = field_validate_tag(field);
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
    p.blank();

    // GAP-R2 — emit a `Validate()` method that runs each field's own
    // `Validate()` (semantic carriers like `lazuli.Percentage`/`HexColor`
    // + nested records), recursing into `Many<Record>` slices. This is the
    // explicit validation path the parent wires when a record value is
    // constructed in Go (not via the wire `UnmarshalJSON` decode boundary,
    // which already fires the carriers). Wire-thin: a single reflection
    // call into `lazuli.ValidateValue`.
    emit_record_validate(p, &pascal);
}
