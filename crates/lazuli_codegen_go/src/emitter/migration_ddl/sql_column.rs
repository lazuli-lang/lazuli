//! Column-level type mapping and the `SqlColumn` rendering shape.
//!
//! `SqlColumn` is the lingua franca every DDL sub-emitter speaks: a
//! column body line that may carry a typed name + nullable flag +
//! optional `GENERATED ALWAYS AS` derivation + optional trailing
//! `-- comment` for operator visibility. Raw constraint clauses
//! (`PRIMARY KEY (...)`, `UNIQUE (...)`, `FOREIGN KEY (...)`) reuse
//! the same shape via `SqlColumn::raw` so the up-migration loop in
//! `create_table.rs` can emit one homogeneous list with consistent
//! comma + comment placement.
//!
//! `pg_type_for*` translates Lazuli IR types (`BuiltinType`,
//! `CapabilityRef`, `UserDefined`, `Many<T>`) into the Postgres column
//! type used inside `CREATE TABLE`. Capability columns
//! (`@cap.Encrypted`, `@cap.E2ee`, `@cap.File`) are handled here too —
//! `encryption_marker_for` returns the trailing comment that DBAs read
//! cold to see which `@key.<scope>` an encrypted column resolves
//! through.
//!
//! See the proposal `semantic-types-money-brazilian.md` v0.5 for the
//! Money/Currency paired-column rule applied in `create_table.rs`.

use lazuli_ir::{BuiltinType, CapabilityRef, EnumDecl, Feature, Field, Module, TypeRef};

use super::super::cross_feature::CrossFeatureIndex;
use super::super::enums::{StorageKind, classify_storage};
use super::sql_builder::sql_ident;
use super::topo::foreign_key_owner;

/// Find the `EnumDecl` named `name` anywhere in the module. Enum-typed
/// fields lower to `TypeRef::UserDefined(qname)` with `qname.feature =
/// None` (the analyzer does not pin the owning feature on the field), so
/// resolution is by bare name across every feature. Returns the first
/// match — duplicate enum names across features are a separate
/// doctor-flagged smell (`PersonType` appears in 3 pauta features) and
/// every same-named enum shares the same int/string storage kind in
/// practice, so the storage classification is stable regardless of which
/// declaration we land on.
fn find_enum_decl<'a>(module: &'a Module, name: &str) -> Option<&'a EnumDecl> {
    module
        .features
        .iter()
        .flat_map(|f| f.enums.iter())
        .find(|e| e.name == name)
}

/// `true` when `name` is declared as a `record` in any feature. Record
/// resolution is by bare name across the module for the same reason as
/// `find_enum_decl`: the field carries `qname.feature = None`, and the
/// cross-index `kind()` returns `None` for a name declared in more than
/// one feature (a shared value object like `Address` lives in 4 pauta
/// features), which would silently drop the record→JSONB lowering back
/// to TEXT — the exact `cannot scan text into *Address` read-time 500.
fn name_is_record(module: &Module, name: &str) -> bool {
    module
        .features
        .iter()
        .flat_map(|f| f.records.iter())
        .any(|r| r.name == name)
}

pub(super) enum SqlColumn {
    Raw(String),
    Typed {
        name: String,
        pg_type: String,
        required: bool,
        generated_as: Option<String>,
        /// Trailing SQL comment for operator visibility (e.g.
        /// `lazuli:encrypted @key.tenant`). Rendered as a `-- ...`
        /// suffix on the column line; no semantic effect on the DDL.
        trailing_comment: Option<String>,
    },
}

impl SqlColumn {
    pub(super) fn raw(sql: &str) -> Self {
        Self::Raw(sql.to_owned())
    }

    pub(super) fn typed(name: &str, pg_type: &str, required: bool) -> Self {
        Self::Typed {
            name: name.to_owned(),
            pg_type: pg_type.to_owned(),
            required,
            generated_as: None,
            trailing_comment: None,
        }
    }

    pub(super) fn typed_generated(
        name: &str,
        pg_type: &str,
        required: bool,
        generated_as: Option<String>,
    ) -> Self {
        Self::Typed {
            name: name.to_owned(),
            pg_type: pg_type.to_owned(),
            required,
            generated_as,
            trailing_comment: None,
        }
    }

    pub(super) fn with_trailing_comment(mut self, comment: String) -> Self {
        if let Self::Typed {
            trailing_comment, ..
        } = &mut self
        {
            *trailing_comment = Some(comment);
        }
        self
    }

    /// Body of the column line, without any trailing comment. The
    /// emitter appends the comma + comment separately so we don't
    /// end up with `BYTEA -- lazuli:encrypted, NOT NULL` (the comma
    /// belongs after the type and before the comment, not after the
    /// comment).
    pub(super) fn render_body(&self) -> String {
        match self {
            Self::Raw(sql) => sql.clone(),
            Self::Typed {
                name,
                pg_type,
                required,
                generated_as,
                ..
            } => {
                let mut rendered = format!("{} {}", sql_ident(name), pg_type);
                if *required {
                    rendered.push_str(" NOT NULL");
                }
                if let Some(expr) = generated_as {
                    rendered.push_str(" GENERATED ALWAYS AS (");
                    rendered.push_str(expr);
                    rendered.push_str(") STORED");
                }
                rendered
            }
        }
    }

    pub(super) fn trailing_comment(&self) -> Option<&str> {
        if let Self::Typed {
            trailing_comment, ..
        } = self
        {
            trailing_comment.as_deref()
        } else {
            None
        }
    }

    /// Back-compat: callers that only need the body (down-migration,
    /// audit DDL) continue to call `render`. Equivalent to
    /// `render_body` plus an inline `-- comment` suffix when
    /// declared, but the caller is responsible for placing the comma
    /// — see `emit_resource_migration` for the up-migration loop that
    /// preserves comma-before-comment ordering.
    #[allow(dead_code)]
    pub(super) fn render(&self) -> String {
        let body = self.render_body();
        match self.trailing_comment() {
            Some(comment) => format!("{body} -- {comment}"),
            None => body,
        }
    }
}

pub(super) struct PgType {
    pub(super) sql: String,
    pub(super) uses_postgis: bool,
}

pub(super) fn pg_type_for(type_ref: &TypeRef) -> PgType {
    match type_ref {
        TypeRef::Builtin(builtin) => pg_type_for_builtin(builtin),
        TypeRef::Many(inner) => {
            let inner = pg_type_for(inner);
            PgType {
                sql: format!("{}[]", inner.sql),
                uses_postgis: inner.uses_postgis,
            }
        }
        TypeRef::Capability(capability) => pg_type_for_capability(capability),
        TypeRef::UserDefined(_) | TypeRef::EnumRef(_) | TypeRef::Unresolved(_) => PgType {
            sql: "TEXT".to_owned(),
            uses_postgis: false,
        },
    }
}

pub(super) fn pg_type_for_field<'a>(
    module: &'a Module,
    feature: &'a Feature,
    field: &'a Field,
    cross_index: &CrossFeatureIndex<'a>,
) -> PgType {
    // A user-named type reaches the emitter as either `UserDefined`
    // (resource FK / record / enum, depending on what the name resolves
    // to) or — after the full module analysis pass that the CLI runs —
    // `EnumRef` (an enum-typed field whose ref the analyzer already
    // pinned). `lower_feature_skeleton` alone leaves enum fields as
    // `UserDefined`, so both variants reach here in practice (pauta's
    // `situation: CustomerSituation = prospect` lands as `EnumRef`, while
    // `person_type: PersonType` stayed `UserDefined`). Resolve the bare
    // name and branch on what it actually is.
    if let TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) = &field.type_ref {
        // FK / record checks only make sense for the `UserDefined` form —
        // an `EnumRef` is definitionally an enum, never a resource or
        // record.
        if matches!(field.type_ref, TypeRef::UserDefined(_)) {
            if foreign_key_owner(module, feature, qname, cross_index).is_some() {
                return PgType {
                    sql: "BIGINT".to_owned(),
                    uses_postgis: false,
                };
            }
            // A8: `record X` columns lower to JSONB (the runtime
            // serializes the struct with `json.Marshal` and the synth
            // lookup query's `pgx.RowToStructByName` then scans
            // JSONB→struct cleanly). Resolved by bare name (see
            // `name_is_record`) rather than via `cross_index.kind()` —
            // the latter returns `None` for a record name shared across
            // features (pauta's `Address` is in 4), which would drop the
            // lowering back to TEXT and 500 the read scan.
            if name_is_record(module, qname.name.as_str()) {
                return PgType {
                    sql: "JSONB".to_owned(),
                    uses_postgis: false,
                };
            }
        }
        // `enum X` columns must align with the emitted Go typed alias so
        // `pgx.RowToStructByName` round-trips the scan. An int-storage
        // enum emits `type X int64` (see emitter::enums) → the column is
        // BIGINT; scanning a `text` column into `*X` (int64) is exactly
        // the `cannot scan text (OID 25) into *CustomerSituation` failure
        // this fixes. A string-storage enum emits `type X string` → TEXT
        // already round-trips. We resolve the enum by name (the
        // cross-index `kind()` returns `None` for names declared in more
        // than one feature — e.g. `PersonType` in 3 pauta features — so
        // the cross-index alone would miss it; `find_enum_decl` walks the
        // module instead).
        if let Some(decl) = find_enum_decl(module, qname.name.as_str()) {
            let sql = match classify_storage(decl) {
                StorageKind::Int => "BIGINT",
                StorageKind::String => "TEXT",
            };
            return PgType {
                sql: sql.to_owned(),
                uses_postgis: false,
            };
        }
    }

    // A8: `many <Record>` collapses to a SINGLE `JSONB` document rather
    // than `JSONB[]` — the Postgres array of jsonb breaks pgx's struct
    // scan path for `[]X` whereas a JSONB array document round-trips.
    // Same bare-name resolution as the scalar record case so a shared
    // record name does not slip back to a `TEXT[]` column.
    if let TypeRef::Many(inner) = &field.type_ref
        && let TypeRef::UserDefined(inner_q) = inner.as_ref()
        && name_is_record(module, inner_q.name.as_str())
    {
        return PgType {
            sql: "JSONB".to_owned(),
            uses_postgis: false,
        };
    }

    pg_type_for(&field.type_ref)
}

pub(super) fn pg_type_for_builtin(builtin: &BuiltinType) -> PgType {
    let sql = match builtin {
        BuiltinType::Id => "BIGINT",
        BuiltinType::Text => "TEXT",
        BuiltinType::Boolean => "BOOLEAN",
        BuiltinType::Integer => "BIGINT",
        BuiltinType::Decimal => "NUMERIC(20, 6)",
        BuiltinType::Date => "DATE",
        BuiltinType::DateTime => "TIMESTAMPTZ",
        BuiltinType::Json => "JSONB",
        BuiltinType::SemanticEmail
        | BuiltinType::SemanticPhone
        | BuiltinType::SemanticUrl
        | BuiltinType::SemanticUuid
        | BuiltinType::SemanticCurrency
        // W1 GAP-04 — HexColor is a text carrier (`#RRGGBB`).
        | BuiltinType::SemanticHexColor => "TEXT",
        // W1 GAP-05 — Percentage is a decimal carrier; mirror `Decimal`'s
        // NUMERIC precision so the 0..=100 ratio stores losslessly.
        BuiltinType::SemanticPercentage => "NUMERIC(20, 6)",
        // Batch E — PositiveDecimal is a decimal carrier; mirror `Decimal`'s
        // NUMERIC precision (the `> 0` guard lives in the runtime carrier).
        BuiltinType::SemanticPositiveDecimal => "NUMERIC(20, 6)",
        // Batch E — NonNegativeInt is an integer carrier; mirror `Integer`'s
        // BIGINT storage (the `>= 0` guard lives in the runtime carrier).
        BuiltinType::SemanticNonNegativeInt => "BIGINT",
        // Per proposal `semantic-types-money-brazilian.md` v0.4:
        // Money stores as `NUMERIC(20,4)`. One shared resource-level
        // `currency TEXT` column is auto-emitted (see resource_columns);
        // authors opt out per-field by declaring an explicit
        // `<money>_currency: Currency` pair.
        BuiltinType::SemanticMoney { .. } => "NUMERIC(20,4)",
        BuiltinType::SemanticGeoPoint => {
            return PgType {
                sql: "geography(point, 4326)".to_owned(),
                uses_postgis: true,
            };
        }
        // B3 — plugin-contributed `@semantic.<Name>` columns inherit
        // the DDL of the carrier. v1 closed-catalog carrier is `String`
        // → `TEXT` (per `docs/proposals/semantic-types-plugin-locales.md`
        // §Manifest mechanism — non-string carriers need a separate
        // proposal because they affect DDL + wire shape).
        BuiltinType::SemanticPluginType { carrier, .. } => return pg_type_for_builtin(carrier),
        BuiltinType::CapSecret => "TEXT",
        BuiltinType::CapFile => "JSONB",
    };

    PgType {
        sql: sql.to_owned(),
        uses_postgis: false,
    }
}

pub(super) fn pg_type_for_capability(capability: &CapabilityRef) -> PgType {
    // `@cap.Encrypted` / `@cap.E2ee` columns store the AES-256-GCM
    // ciphertext envelope (`nonce || ciphertext || tag`) — BYTEA. The
    // `EncryptCustomer`/`DecryptCustomer` helpers in `resource.gen.go`
    // thread these bytes through `encryption.ForCtx` at the SQL
    // boundary. `@cap.File(...)` columns store the JSONB FileRef
    // shape `{key, content_type, size}` populated by the storage
    // runtime's `confirm_*_upload` lowering — see
    // docs/proposals/fileref-jsonb-roundtrip.md. Hashed + Token
    // capabilities stay TEXT until a pilot demands a richer shape.
    let sql = match capability {
        CapabilityRef::Encrypted(_) | CapabilityRef::E2ee(_) => "BYTEA",
        CapabilityRef::File(_) => "JSONB",
        _ => "TEXT",
    };
    PgType {
        sql: sql.to_owned(),
        uses_postgis: false,
    }
}

/// Render the operator-visibility marker for an `@cap.Encrypted` /
/// `@cap.E2ee` column. Lifted into the SQL as a trailing `-- ...`
/// comment so DBAs cold-reading the schema see which `@key.<scope>`
/// each encrypted column resolves through.
pub(super) fn encryption_marker_for(field: &Field) -> Option<String> {
    match &field.type_ref {
        TypeRef::Capability(CapabilityRef::Encrypted(cap)) => Some(format!(
            "lazuli:encrypted {} algorithm=aes_256_gcm",
            cap.key
        )),
        TypeRef::Capability(CapabilityRef::E2ee(cap)) => {
            Some(format!("lazuli:e2ee {} algorithm=aes_256_gcm", cap.key))
        }
        _ => None,
    }
}
