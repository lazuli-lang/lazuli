//! Column-type tests: builtin → Postgres-type lowering, capability →
//! storage shape (TEXT / BYTEA / JSONB), authored UNIQUE / index
//! constraint emission, PostGIS extension + GIST index. Anything that
//! probes *what type a single column compiles to* belongs here.

#![cfg(test)]

use super::emit_migrations;
use super::sql_column::pg_type_for_capability;
use super::test_support::{
    base_feature, base_module, builtin, field, parsed_module, qname, resource,
};
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
            builtin("unit_price", BuiltinType::SemanticPositiveDecimal, false),
            builtin("stock_count", BuiltinType::SemanticNonNegativeInt, false),
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
    // Batch E — PositiveDecimal mirrors Decimal's NUMERIC precision (the
    // `> 0` guard lives in the runtime carrier's UnmarshalJSON).
    assert!(sql.contains("unit_price NUMERIC(20, 6),"));
    // Batch E — NonNegativeInt mirrors Integer's BIGINT storage (the `>= 0`
    // guard lives in the runtime carrier's UnmarshalJSON).
    assert!(sql.contains("stock_count BIGINT,"));
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
            error_code: None,
        }));
    customer
        .constraints
        .push(Constraint::Unique(UniqueConstraint {
            fields: vec!["external_id".to_owned()],
            per: None,
            when: None,
            error_code: None,
        }));
    feature.resources.push(customer);

    let files = emit_migrations(&base_module(vec![feature]), "crm");
    let sql = &files[0].contents;

    assert!(sql.contains("UNIQUE (email, org_id),"));
    assert!(sql.contains("UNIQUE (external_id)"));
}

#[test]
fn emits_named_unique_constraint_when_error_code_present() {
    // A `unique ... error <CODE>` constraint MUST emit a DETERMINISTICALLY
    // NAMED `CONSTRAINT <table>_<field_slug>_key UNIQUE (...)` so the runtime
    // `pgErr.ConstraintName` match is reliable (an anonymous `UNIQUE (...)`
    // gets a Postgres auto-name that codegen cannot predict). The
    // unconditional-without-code form stays anonymous (back-compat).
    let module = parsed_module(
        r#"feature job
  domain
    resource JobMember
      job_id: ID required
      user_id: ID required
      unique (job_id, user_id) error MEMBER_ALREADY_IN_JOB
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_job_job_member.sql")
        .expect("job_member migration")
        .contents
        .as_str();

    assert!(
        sql.contains("CONSTRAINT job_member_job_id_user_id_key UNIQUE (job_id, user_id)"),
        "expected named unique constraint:\n{sql}"
    );
    // Must NOT emit an anonymous `UNIQUE (job_id, user_id)` (the name would
    // be unpredictable and the runtime match would silently miss).
    assert!(
        !sql.contains("\n  UNIQUE (job_id, user_id)")
            && !sql.contains(", UNIQUE (job_id, user_id)"),
        "coded unique must be named, not anonymous:\n{sql}"
    );
}

#[test]
fn emits_named_partial_unique_index_when_error_code_present() {
    // A coded partial unique (`error <CODE> when <pred>`) keeps its existing
    // deterministic `_uidx` index name (already matchable); the `error` code
    // is carried for the runtime registration glue, not the DDL name.
    let module = parsed_module(
        r#"feature catalog
  domain
    resource Category
      name: Text required
      deleted_at: DateTime
      unique name error CATEGORY_NAME_TAKEN when deleted_at == nil
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_catalog_category.sql")
        .expect("category migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX category_name_uidx ON \"category\" (name) WHERE deleted_at IS NULL;"
        ),
        "expected named partial unique index:\n{sql}"
    );
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
fn emits_soft_delete_conditional_unique_as_partial_is_null_index() {
    // GAP-NEW-002 — `unique <field> when deleted_at == nil` must lower the
    // nil comparison to `IS NULL` in the partial-index `WHERE`, NOT degrade
    // to a non-partial index + `-- unsupported` comment. A full unique index
    // would wrongly block name reuse after a soft-delete; the partial form
    // only enforces uniqueness over the live (non-deleted) rows.
    let module = parsed_module(
        r#"feature catalog
  domain
    resource Category
      name: Text required
      deleted_at: DateTime
      unique name when deleted_at == nil
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_catalog_category.sql")
        .expect("category migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX category_name_uidx ON \"category\" (name) WHERE deleted_at IS NULL;"
        ),
        "expected partial unique index with IS NULL predicate:\n{sql}"
    );
    // Must NOT degrade to the unsupported-predicate fallback.
    assert!(
        !sql.contains("WHERE clause unsupported"),
        "nil comparison must lower to IS NULL, not degrade:\n{sql}"
    );
}

#[test]
fn emits_conditional_unique_is_not_null_for_nil_inequality() {
    // GAP-NEW-002 — the inverse: `!= nil` lowers to `IS NOT NULL`.
    let module = parsed_module(
        r#"feature catalog
  domain
    resource Slot
      code: Text required
      assigned_to: ID
      unique code when assigned_to != nil
"#,
    );

    let files = emit_migrations(&module, "atelier");
    let sql = files
        .iter()
        .find(|file| file.path == "migrations/001_catalog_slot.sql")
        .expect("slot migration")
        .contents
        .as_str();

    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX slot_code_uidx ON \"slot\" (code) WHERE assigned_to IS NOT NULL;"
        ),
        "expected partial unique index with IS NOT NULL predicate:\n{sql}"
    );
    assert!(
        !sql.contains("WHERE clause unsupported"),
        "nil inequality must lower to IS NOT NULL, not degrade:\n{sql}"
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

#[test]
fn nested_record_field_lowers_to_jsonb_not_text() {
    // A `record X` value object stored on a resource lowers to a JSONB
    // column — pgx scans JSONB↔struct cleanly via RowToStructByName,
    // whereas a TEXT column 500s with `cannot scan text into *X` at read
    // time the moment a row carries real data. Both the required and the
    // optional (nullable) form must be JSONB.
    let module = parsed_module(
        r#"feature crm
  domain
    record Address
      street: Text required
      city: Text required

    resource Customer
      address: Address required
      billing_address: Address optional
"#,
    );
    let files = emit_migrations(&module, "pilot");
    let sql = files
        .iter()
        .find(|f| f.path == "migrations/001_crm_customer.sql")
        .expect("customer migration")
        .contents
        .as_str();

    assert!(
        sql.contains("address JSONB NOT NULL,"),
        "required nested record must be JSONB NOT NULL:\n{sql}"
    );
    assert!(
        sql.contains("billing_address JSONB"),
        "nullable nested record must be JSONB:\n{sql}"
    );
    assert!(
        !sql.contains("address TEXT"),
        "nested record must NOT lower to TEXT (the read-time 500):\n{sql}"
    );
}

#[test]
fn shared_record_name_across_features_still_lowers_to_jsonb() {
    // Regression for the real pauta failure: a value-object record like
    // `Address` is declared in MORE THAN ONE feature, which makes the
    // cross-feature index mark the name ambiguous → `kind()` returns
    // `None` → the record→JSONB branch was silently skipped and the
    // column fell back to TEXT, 500ing the list scan
    // (`cannot scan text into *Address`). Resolution must be by bare
    // name across the module, NOT via the ambiguity-sensitive index.
    let module = parsed_module(
        r#"feature customer_management
  domain
    record Address
      street: Text required
      city: Text required

    resource Customer
      address: Address required
      billing_address: Address optional

feature supplier
  domain
    record Address
      street: Text required
      city: Text required

    resource Supplier
      address: Address required
"#,
    );
    let files = emit_migrations(&module, "pauta");

    let cust = files
        .iter()
        .find(|f| f.path == "migrations/001_customer_management_customer.sql")
        .expect("customer migration")
        .contents
        .as_str();
    assert!(
        cust.contains("address JSONB NOT NULL,") && cust.contains("billing_address JSONB"),
        "shared-name record must STILL be JSONB despite cross-feature ambiguity:\n{cust}"
    );
    assert!(
        !cust.contains("address TEXT"),
        "ambiguous record name must not fall back to TEXT:\n{cust}"
    );

    let sup = files
        .iter()
        .find(|f| f.path == "migrations/002_supplier_supplier.sql")
        .expect("supplier migration")
        .contents
        .as_str();
    assert!(
        sup.contains("address JSONB NOT NULL"),
        "supplier's same-named record must also be JSONB:\n{sup}"
    );
}

#[test]
fn enum_ref_typed_field_lowers_to_bigint_like_user_defined() {
    // The CLI's full module analysis pins enum-typed fields as
    // `TypeRef::EnumRef(...)` (whereas `lower_feature_skeleton` alone
    // leaves them `UserDefined`). pauta's `situation: CustomerSituation
    // = prospect` lands as `EnumRef` and regressed to TEXT (the
    // `cannot scan text into **CustomerSituation` 500). Both the
    // `UserDefined` and the `EnumRef` carrier of the same int enum must
    // lower to BIGINT. Build the field with an explicit `EnumRef` so
    // this is locked independent of the analyzer's resolution mode.
    use lazuli_ir::{EnumDecl, EnumVariant, StorageValue};

    let mut feature = base_feature("crm");
    let mk_variant = |name: &str, v: i64| EnumVariant {
        name: name.to_owned(),
        storage_value: Some(StorageValue::Integer(v)),
        label_key: None,
        hint_key: None,
        icon_key: None,
        previous_names: Vec::new(),
    };
    feature.enums.push(EnumDecl {
        name: "CustomerSituation".to_owned(),
        public_contract: None,
        variants: vec![mk_variant("prospect", 10), mk_variant("active", 20)],
        previous_names: Vec::new(),
        span_ref: None,
    });
    feature.resources.push(resource(
        "Customer",
        vec![field(
            "situation",
            TypeRef::EnumRef(qname("CustomerSituation")),
            false,
        )],
    ));

    let files = emit_migrations(&base_module(vec![feature]), "pilot");
    let sql = &files[0].contents;
    assert!(
        sql.contains("situation BIGINT"),
        "EnumRef-carried int enum must lower to BIGINT:\n{sql}"
    );
    assert!(
        !sql.contains("situation TEXT"),
        "EnumRef int enum must NOT be TEXT (the read-time 500):\n{sql}"
    );
}

#[test]
fn int_storage_enum_field_lowers_to_bigint_not_text() {
    // An enum with integer storage values emits `type X int64` on the Go
    // side (emitter::enums). The DB column MUST be BIGINT so
    // `pgx.RowToStructByName` scans `int8`→`int64`. A TEXT column 500s
    // with `cannot scan text (OID 25) into *PersonType` — the exact bug.
    let module = parsed_module(
        r#"feature crm
  domain
    enum PersonType
      individual = 10
      company = 20

    resource Supplier
      person_type: PersonType required
"#,
    );
    let files = emit_migrations(&module, "pilot");
    let sql = files
        .iter()
        .find(|f| f.path == "migrations/001_crm_supplier.sql")
        .expect("supplier migration")
        .contents
        .as_str();

    assert!(
        sql.contains("person_type BIGINT NOT NULL"),
        "int-storage enum must be BIGINT (matches `type X int64`):\n{sql}"
    );
    assert!(
        !sql.contains("person_type TEXT"),
        "int-storage enum must NOT be TEXT (the read-time 500):\n{sql}"
    );
}

#[test]
fn string_storage_enum_field_stays_text() {
    // A string-storage enum emits `type X string` on the Go side, which
    // round-trips a TEXT column cleanly — keep TEXT (don't over-promote
    // to BIGINT, which would break the string round-trip).
    let module = parsed_module(
        r#"feature crm
  domain
    enum Color
      red = "RED"
      green = "GREEN"

    resource Widget
      color: Color required
"#,
    );
    let files = emit_migrations(&module, "pilot");
    let sql = files
        .iter()
        .find(|f| f.path == "migrations/001_crm_widget.sql")
        .expect("widget migration")
        .contents
        .as_str();

    assert!(
        sql.contains("color TEXT NOT NULL"),
        "string-storage enum must stay TEXT (matches `type X string`):\n{sql}"
    );
}
