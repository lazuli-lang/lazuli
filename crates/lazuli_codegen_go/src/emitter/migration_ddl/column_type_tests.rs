//! Column-type tests: builtin → Postgres-type lowering, capability →
//! storage shape (TEXT / BYTEA / JSONB), authored UNIQUE / index
//! constraint emission, PostGIS extension + GIST index. Anything that
//! probes *what type a single column compiles to* belongs here.

#![cfg(test)]

use super::emit_migrations;
use super::sql_column::pg_type_for_capability;
use super::test_support::{base_feature, base_module, builtin, field, parsed_module, resource};
use lazuli_ir::{
    BuiltinType, CapabilityRef, Constraint, EncryptedCapability, FileCapability, FileSize,
    FileSizeLiteral, FileVisibility, HashAlgorithm, HashedCapability, MimeType, TokenCapability,
    TokenStore, TypeRef, UniqueConstraint,
};

#[test]
fn maps_builtin_types_and_requiredness() {
    let mut feature = base_feature("customer");
    feature.resources.push(resource(
        "Customer",
        vec![
            builtin("external_id", BuiltinType::Id, true),
            builtin("name", BuiltinType::Text, true),
            builtin("active", BuiltinType::Boolean, true),
            builtin("age", BuiltinType::Integer, false),
            builtin("balance", BuiltinType::Decimal, false),
            builtin("birthday", BuiltinType::Date, false),
            builtin("last_seen", BuiltinType::DateTime, false),
            builtin("metadata", BuiltinType::Json, false),
            builtin("email", BuiltinType::SemanticEmail, true),
            builtin("phone", BuiltinType::SemanticPhone, false),
            builtin("website", BuiltinType::SemanticUrl, false),
            builtin("uuid", BuiltinType::SemanticUuid, false),
            builtin("currency", BuiltinType::SemanticCurrency, true),
            builtin("brand_color", BuiltinType::SemanticHexColor, false),
            builtin("discount_rate", BuiltinType::SemanticPercentage, false),
            builtin(
                "cents",
                BuiltinType::SemanticMoney {
                    currency: lazuli_ir::CurrencyCode::BRL,
                },
                true,
            ),
        ],
    ));

    let files = emit_migrations(&base_module(vec![feature]), "crm");
    let sql = &files[0].contents;

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"customer\" ("));
    assert!(sql.contains("id BIGSERIAL PRIMARY KEY,"));
    assert!(sql.contains("external_id BIGINT NOT NULL,"));
    assert!(sql.contains("name TEXT NOT NULL,"));
    assert!(sql.contains("active BOOLEAN NOT NULL,"));
    assert!(sql.contains("age BIGINT,"));
    assert!(sql.contains("balance NUMERIC(20, 6),"));
    assert!(sql.contains("birthday DATE,"));
    assert!(sql.contains("last_seen TIMESTAMPTZ,"));
    assert!(sql.contains("metadata JSONB,"));
    assert!(sql.contains("email TEXT NOT NULL,"));
    assert!(sql.contains("phone TEXT,"));
    assert!(sql.contains("website TEXT,"));
    assert!(sql.contains("uuid TEXT,"));
    // W1 GAP-04 — HexColor is a text carrier (`#RRGGBB`).
    assert!(sql.contains("brand_color TEXT,"));
    // W1 GAP-05 — Percentage mirrors Decimal's NUMERIC precision.
    assert!(sql.contains("discount_rate NUMERIC(20, 6),"));
    // The author-declared `currency: Currency` column stays as-is
    // (the v0.5 codegen no longer mints a shared `currency` column
    // — it only emits per-Money-field `<field>_currency` columns
    // now — so there's no duplicate to suppress).
    assert!(sql.contains("currency TEXT NOT NULL,"));
    // MONEY-1 §3.2 (v0.5) — Money fields carry their currency in
    // IR, so the DDL emits a paired `<field>_currency` column with
    // a CHECK constraint pinned to the declared ISO and a DEFAULT
    // so ALTER-time inserts cannot drift.
    assert!(sql.contains("cents NUMERIC(20,4) NOT NULL"));
    assert!(
        sql.contains("cents_currency TEXT NOT NULL CHECK (cents_currency = 'BRL') DEFAULT 'BRL'")
    );
}

#[test]
fn emits_unique_resource_constraints_inside_create_table() {
    let mut feature = base_feature("customer");
    let mut customer = resource(
        "Customer",
        vec![
            builtin("email", BuiltinType::SemanticEmail, true),
            builtin("external_id", BuiltinType::Text, true),
        ],
    );
    customer
        .constraints
        .push(Constraint::Unique(UniqueConstraint {
            fields: vec!["email".to_owned()],
            per: Some("Org".to_owned()),
            when: None,
        }));
    customer
        .constraints
        .push(Constraint::Unique(UniqueConstraint {
            fields: vec!["external_id".to_owned()],
            per: None,
            when: None,
        }));
    feature.resources.push(customer);

    let files = emit_migrations(&base_module(vec![feature]), "crm");
    let sql = &files[0].contents;

    assert!(sql.contains("UNIQUE (email, org_id),"));
    assert!(sql.contains("UNIQUE (external_id)"));
}

#[test]
fn emits_authored_resource_indexes() {
    let module = parsed_module(
        r#"feature post
  domain
    resource Post
      title: Text required
      tags: list of Text
      unique (title, tags)
      index on (title)
      index on tags gin
      fts on (title, tags)
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_post_post.sql")
        .expect("post migration")
        .contents
        .as_str();

    assert!(sql.contains("UNIQUE (title, tags)"));
    assert!(sql.contains("CREATE INDEX post_title_idx ON \"post\" (title);"));
    assert!(sql.contains("CREATE INDEX post_tags_gin ON \"post\" USING GIN (tags);"));
    assert!(sql.contains(
        "CREATE INDEX post_title_tags_fts ON \"post\" USING GIN (to_tsvector('english', concat_ws(' ', title, tags)));"
    ));
}

#[test]
fn emits_conditional_unique_as_partial_index() {
    // GAP-NEW-001 — `unique <field> when <predicate>` lowers to a
    // partial `CREATE UNIQUE INDEX ... WHERE`, NOT a table UNIQUE clause.
    let module = parsed_module(
        r#"feature catalog
  domain
    resource PriceTable
      name: Text required
      is_default: Boolean required
      unique is_default when is_default == true
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_catalog_price_table.sql")
        .expect("price_table migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX price_table_is_default_uidx ON \"price_table\" (is_default) WHERE is_default = true;"
        ),
        "expected partial unique index:\n{sql}"
    );
    // The conditional unique must NOT appear as a table-level UNIQUE clause.
    assert!(
        !sql.contains("UNIQUE (is_default),"),
        "conditional unique must not be a table constraint:\n{sql}"
    );
}

#[test]
fn emits_cross_feature_target_as_logical_index_not_hard_fk() {
    // GAP-12 — `target @feature.X.Y` emits a btree index + comment, NOT
    // a hard `FOREIGN KEY` (target table belongs to another migration set).
    let module = parsed_module(
        r#"feature agency
  uses department

  domain
    resource Agency
      name: Text required
      default_department_id: ID target @feature.department.Department
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_agency_agency.sql")
        .expect("agency migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "CREATE INDEX agency_default_department_id_fkidx ON \"agency\" (default_department_id);"
        ),
        "expected logical cross-feature FK index:\n{sql}"
    );
    assert!(
        sql.contains("logical FK -> department.Department"),
        "expected logical-ref comment:\n{sql}"
    );
    // No hard FOREIGN KEY across the feature boundary.
    assert!(
        !sql.contains("FOREIGN KEY (default_department_id)"),
        "cross-feature target must not emit a hard FK:\n{sql}"
    );
}

#[test]
fn emits_polymorphic_ref_columns_check_and_index() {
    // GAP-13 — `polymorphic_ref` emits discriminator + id columns + a
    // CHECK over target names + a composite index, NOT a hard FK.
    let module = parsed_module(
        r#"feature ops
  domain
    resource ActivityLog
      summary: Text required
      polymorphic_ref entity_type entity_id targets [Job, Activity, Customer]
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_ops_activity_log.sql")
        .expect("activity_log migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "entity_type TEXT NOT NULL CHECK (entity_type IN ('Job', 'Activity', 'Customer'))"
        ),
        "expected discriminator column + CHECK:\n{sql}"
    );
    assert!(
        sql.contains("entity_id BIGINT NOT NULL"),
        "expected id column:\n{sql}"
    );
    assert!(
        sql.contains("CREATE INDEX activity_log_entity_type_entity_id_pidx ON \"activity_log\" (entity_type, entity_id);"),
        "expected composite polymorphic index:\n{sql}"
    );
    // Never a hard FK for a polymorphic referent.
    assert!(
        !sql.contains("FOREIGN KEY (entity_id)"),
        "polymorphic_ref must not emit a hard FK:\n{sql}"
    );
}

#[test]
fn inline_unique_field_on_org_tenancy_emits_composite_unique() {
    let module = parsed_module(
        r#"feature account
  defaults
    tenancy org

  domain
    resource User
      email: @semantic.Email required unique
      name: Text required
"#,
    );
    assert!(module.features[0].resources[0].fields[0].unique);

    let files = emit_migrations(&module, "the canonical pilot");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_account_user.sql")
        .map(|file| file.contents.as_str())
        .expect("expected account user migration");

    assert!(
        sql.contains("UNIQUE (email, org_id)"),
        "TenancyOrg inline unique field should be org-scoped:\n{sql}"
    );
}

#[test]
fn inline_unique_field_on_non_tenant_resource_emits_plain_unique() {
    let module = parsed_module(
        r#"feature account
  domain
    resource User
      email: @semantic.Email required unique
      name: Text required
"#,
    );
    assert!(module.features[0].resources[0].fields[0].unique);

    let files = emit_migrations(&module, "the canonical pilot");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_account_user.sql")
        .map(|file| file.contents.as_str())
        .expect("expected account user migration");

    assert!(
        sql.contains("UNIQUE (email)"),
        "non-tenant inline unique field should be global:\n{sql}"
    );
    assert!(
        !sql.contains("UNIQUE (email, org_id)"),
        "non-tenant inline unique field must not include org_id:\n{sql}"
    );
}

#[test]
fn emits_postgis_extension_geography_column_and_gist_index() {
    let mut feature = base_feature("places");
    feature.resources.push(resource(
        "Place",
        vec![builtin("coordinates", BuiltinType::SemanticGeoPoint, true)],
    ));

    let files = emit_migrations(&base_module(vec![feature]), "maps");
    let sql = &files[0].contents;

    assert!(sql.contains("CREATE EXTENSION IF NOT EXISTS postgis;"));
    assert_eq!(
        sql.matches("CREATE EXTENSION IF NOT EXISTS postgis;")
            .count(),
        1
    );
    assert!(sql.contains("coordinates geography(point, 4326) NOT NULL"));
    assert!(
        sql.contains("CREATE INDEX place_coordinates_gist ON \"place\" USING GIST (coordinates);")
    );

    let down = files
        .iter()
        .find(|file| file.path == "migrations/001_places_place.down.sql")
        .expect("expected place down migration");
    assert!(down.contents.contains("DROP TABLE IF EXISTS \"place\";"));
    assert!(
        down.contents
            .contains("-- DROP INDEX place_coordinates_gist;")
    );
    assert!(down.contents.contains(
        "-- Note: `CREATE EXTENSION postgis` is NOT dropped here \u{2014} extension\n\
-- removal is operational, not migration-driven."
    ));
}

#[test]
fn maps_capabilities_and_many_to_text_or_array_columns() {
    let mut feature = base_feature("security");
    feature.resources.push(resource(
        "Credential",
        vec![
            builtin("secret", BuiltinType::CapSecret, true),
            field(
                "hashed_password",
                TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                    algorithm: HashAlgorithm::Argon2id,
                })),
                true,
            ),
            field(
                "encrypted_note",
                TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                    key: "@key.tenant".to_owned(),
                })),
                false,
            ),
            field(
                "api_token",
                TypeRef::Capability(CapabilityRef::Token(TokenCapability {
                    ttl: "1h".to_owned(),
                    single_use: false,
                    store: TokenStore::Hashed,
                })),
                true,
            ),
            field(
                "tags",
                TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Text))),
                false,
            ),
        ],
    ));

    let files = emit_migrations(&base_module(vec![feature]), "crm");
    let sql = &files[0].contents;

    assert!(sql.contains("secret TEXT NOT NULL,"));
    assert!(sql.contains("hashed_password TEXT NOT NULL,"));
    // `@cap.Encrypted` columns store the AES-256-GCM ciphertext
    // envelope; BYTEA + operator-visibility comment with the
    // bound `@key.<scope>`.
    assert!(
        sql.contains("encrypted_note BYTEA, -- lazuli:encrypted @key.tenant algorithm=aes_256_gcm"),
        "expected BYTEA + lazuli:encrypted comment, sql:\n{sql}"
    );
    assert!(sql.contains("api_token TEXT NOT NULL,"));
    assert!(sql.contains("tags TEXT[]"));
}

#[test]
fn cap_file_field_emits_jsonb_column() {
    // Verify @cap.File resource fields lower to JSONB DDL columns
    // (FR-2 of fileref-jsonb-roundtrip.md). The Go FileRef struct's
    // Scanner/Valuer (FR-1) reads/writes the JSONB shape.
    let capability_ref = CapabilityRef::File(FileCapability {
        max_size: FileSize {
            bytes: 5 * 1024 * 1024,
            literal: FileSizeLiteral::Mb(5),
        },
        accept: vec![MimeType {
            family: "image".to_owned(),
            subtype: "jpeg".to_owned(),
        }],
        visibility: Some(FileVisibility::Private),
        signed_ttl: None,
        auto_photo_policy: None,
    });
    let pg = pg_type_for_capability(&capability_ref);
    assert_eq!(
        pg.sql, "JSONB",
        "@cap.File capability ref must lower to JSONB"
    );
}

#[test]
fn e2ee_capability_emits_bytea_with_e2ee_marker() {
    use lazuli_ir::E2eeCapability;
    let mut feature = base_feature("notes");
    feature.resources.push(resource(
        "PrivateNote",
        vec![field(
            "body",
            TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
                key: "@key.user".to_owned(),
            })),
            true,
        )],
    ));
    let files = emit_migrations(&base_module(vec![feature]), "notes");
    let sql = &files[0].contents;
    // Last column → no trailing comma; the comment still lands.
    assert!(
        sql.contains("body BYTEA NOT NULL -- lazuli:e2ee @key.user algorithm=aes_256_gcm"),
        "expected BYTEA + lazuli:e2ee comment, sql:\n{sql}"
    );
}

// spec 0016 — first-class `Money` lowers to representation-preserving
// storage: a minor-units NUMERIC amount column PLUS an enforced
// `<field>_currency` sibling column whose CHECK pins the declared ISO.
// The currency is locale-neutral — a `Money(currency: USD)` field emits a
// USD-pinned sibling, NOT a baked-in BRL. This is the `money` gate test.
#[test]
fn money_field_lowers_to_amount_plus_enforced_currency_column() {
    let mut feature = base_feature("payments");
    feature.resources.push(resource(
        "Charge",
        vec![builtin(
            "amount",
            BuiltinType::SemanticMoney {
                currency: lazuli_ir::CurrencyCode::USD,
            },
            true,
        )],
    ));

    let files = emit_migrations(&base_module(vec![feature]), "pay");
    let sql = &files[0].contents;

    // Amount lowers to a minor-units NUMERIC column (value/scale preserved).
    assert!(
        sql.contains("amount NUMERIC(20,4) NOT NULL"),
        "expected minor-units NUMERIC amount column, sql:\n{sql}"
    );
    // The currency sibling is ENFORCED by codegen — the author never
    // declared `amount_currency`, yet it appears, pinned to USD (no
    // baked-in BRL — locale-neutral).
    assert!(
        sql.contains("amount_currency TEXT NOT NULL CHECK (amount_currency = 'USD') DEFAULT 'USD'"),
        "expected enforced USD currency sibling column, sql:\n{sql}"
    );
    assert!(
        !sql.contains("'BRL'"),
        "Money(currency: USD) must NOT bake in BRL anywhere, sql:\n{sql}"
    );
}
