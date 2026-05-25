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

use lazuli_ir::{BuiltinType, CapabilityRef, Feature, Field, Module, TypeRef};

use super::super::cross_feature::{CrossFeatureIndex, DeclKind};
use super::sql_builder::sql_ident;
use super::topo::foreign_key_owner;

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
    if let TypeRef::UserDefined(qname) = &field.type_ref {
        if foreign_key_owner(module, feature, qname, cross_index).is_some() {
            return PgType {
                sql: "BIGINT".to_owned(),
                uses_postgis: false,
            };
        }
        // A8: `record X` columns lower to JSONB (the runtime serializes
        // the struct with `json.Marshal` and the synth lookup query's
        // `pgx.RowToStructByName` then scans JSONB→struct cleanly).
        if cross_index.kind(qname.name.as_str()) == Some(DeclKind::Record) {
            return PgType {
                sql: "JSONB".to_owned(),
                uses_postgis: false,
            };
        }
    }

    // A8: `many <Record>` collapses to a SINGLE `JSONB` document rather
    // than `JSONB[]` — the Postgres array of jsonb breaks pgx's struct
    // scan path for `[]X` whereas a JSONB array document round-trips.
    if let TypeRef::Many(inner) = &field.type_ref {
        if let TypeRef::UserDefined(inner_q) = inner.as_ref() {
            if cross_index.kind(inner_q.name.as_str()) == Some(DeclKind::Record) {
                return PgType {
                    sql: "JSONB".to_owned(),
                    uses_postgis: false,
                };
            }
        }
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
        | BuiltinType::SemanticCurrency => "TEXT",
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
