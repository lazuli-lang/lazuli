//! DDL migration emission for resource tables.
//!
//! `emit_migrations` is the entry point used by the top-level Go
//! codegen orchestrator. It walks every feature's resources, sorts
//! them so FK targets come before their references (Kahn's algorithm,
//! see `topo`), and emits one `<NNN>_<feature>_<resource>.sql` up
//! migration plus a paired `.down.sql` companion. A shared
//! `audit_log.down.sql` rollback closes the run — the matching
//! `audit_log.sql` up migration is emitted by the top-level module
//! emitter and is intentionally not the responsibility of this
//! walker.
//!
//! The sub-tree is split Rails-style for cold-readability:
//!
//! - [`sql_builder`] — identifier quoting, snake-casing, reserved
//!   words, and dialect-agnostic helpers used by every sibling.
//! - [`sql_column`] — `SqlColumn` rendering + `pg_type_for*` IR-to-
//!   Postgres type lowering, plus the `encryption_marker_for` `--`
//!   comment helper.
//! - [`constraint`] — UNIQUE (inline + block), FOREIGN KEY, composite
//!   key clauses.
//! - [`index`] — authored indexes (`@full_text`, method-tagged
//!   b-tree/GIN/GIST) + session-rotation companion indexes.
//! - [`topo`] — FK-aware topological sort + `foreign_key_owner`
//!   resolution.
//! - [`create_table`] — `CREATE TABLE` body + column gather +
//!   session-rotation column auto-injection.
//! - [`drop_table`] — `DROP TABLE` body + commented `DROP INDEX`
//!   hints + the shared audit-log rollback.
//! - [`alter_table`] — `ALTER TABLE` emission from `SchemaDiff`
//!   (cell A11). The TEMP-STUB shapes carried here will migrate to
//!   `crates/lazuli_codegen_go/src/emitter/schema_diff.rs` (cell
//!   A10) once that lands.
//!
//! Cross-feature FK resolution piggybacks on `super::cross_feature::
//! CrossFeatureIndex`, which the top-level module emitter builds
//! once per `Module`. See `crates/lazuli_codegen_go/src/emitter/
//! cross_feature.rs` for the indexing contract.

mod alter_table;
mod constraint;
mod create_table;
mod drop_table;
mod index;
mod sql_builder;
mod sql_column;
mod topo;

use lazuli_ir::{Feature, Module, Resource};

#[allow(unused_imports)]
use lazuli_ir::{BuiltinType, Constraint, TypeRef};

use super::cross_feature::CrossFeatureIndex;
use crate::GeneratedFile;

pub use alter_table::{
    AlterDefault, AlterEmitOptions, ColumnAdd, ColumnDrop, SchemaDiff, TypeChange,
    emit_alter_migration_file,
};

// Internal-only re-exports — tests live in this file and use
// `super::*` to reach the prod helpers. New code should reach into
// the sub-modules directly rather than via this aggregator.
use create_table::emit_resource_migration;
use drop_table::{emit_audit_log_down_migration, emit_resource_down_migration};
use sql_builder::lower_snake;
use topo::topo_sort_resources;

#[allow(unused_imports)]
use create_table::resource_columns;
#[allow(unused_imports)]
use sql_builder::{comment_value, quote_ident, sql_ident};
#[allow(unused_imports)]
use sql_column::{PgType, pg_type_for, pg_type_for_capability, pg_type_for_field};
#[allow(unused_imports)]
use topo::foreign_key_owner;

/// Emit SQL migrations in deterministic, cross-feature lexical order.
///
/// The returned paths are relative to the generated Go output root:
/// `migrations/<NNN>_<feature>_<resource>.sql` plus companion
/// `migrations/<NNN>_<feature>_<resource>.down.sql` rollback files.
/// The shared audit table rollback is emitted here because the matching
/// `migrations/audit_log.sql` up migration is always emitted by the
/// top-level module emitter.
pub fn emit_migrations(module: &Module, source_label: &str) -> Vec<GeneratedFile> {
    let cross_index = CrossFeatureIndex::build(module);
    let raw_resources: Vec<(&Feature, &Resource)> = module
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .resources
                .iter()
                .map(move |resource| (feature, resource))
        })
        .collect();

    // WAR-RUNTIME-MIGRATION-03 — order resources so every FK target's
    // CREATE TABLE runs BEFORE the referencing FOREIGN KEY constraint.
    // Lexical (feature, resource) is the tiebreaker for resources with
    // no dependency between them, so output stays stable when the
    // dependency graph doesn't pin a relative order.
    let resources = topo_sort_resources(module, &raw_resources, &cross_index);

    let mut files = Vec::with_capacity(resources.len() * 2 + 1);

    for (idx, (feature, resource)) in resources.iter().copied().enumerate() {
        let resource_slug = lower_snake(&resource.name);
        files.push(GeneratedFile {
            path: format!(
                "migrations/{:03}_{}_{}.sql",
                idx + 1,
                feature.name,
                resource_slug
            ),
            contents: emit_resource_migration(
                module,
                feature,
                resource,
                source_label,
                &cross_index,
            ),
        });
    }

    for (idx, (feature, resource)) in resources.iter().copied().enumerate() {
        let resource_slug = lower_snake(&resource.name);
        files.push(GeneratedFile {
            path: format!(
                "migrations/{:03}_{}_{}.down.sql",
                idx + 1,
                feature.name,
                resource_slug
            ),
            contents: emit_resource_down_migration(feature, resource),
        });
    }

    files.push(emit_audit_log_down_migration());
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Auth, AuthIdentity, AuthSessions, CapabilityRef, Defaults, EncryptedCapability, Feature,
        Field, FieldRef, FileCapability, FileSize, FileSizeLiteral, FileVisibility, HashAlgorithm,
        HashedCapability, MimeType, Module, Policies, QualifiedName, Resource, RotationConfig,
        Tenancy, TokenCapability, TokenStore, TypeRef, UniqueConstraint,
    };

    fn base_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn parsed_module(source: &str) -> Module {
        let features = lazuli_syntax::parse_feature_skeletons(source)
            .expect("feature source should parse")
            .into_iter()
            .map(|feature| {
                lazuli_analyzer::lower_feature_skeleton(&feature)
                    .expect("feature source should lower")
            })
            .collect();
        base_module(features)
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

    fn resource(name: &str, fields: Vec<Field>) -> Resource {
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

    fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
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

    fn builtin(name: &str, builtin: BuiltinType, required: bool) -> Field {
        field(name, TypeRef::Builtin(builtin), required)
    }

    fn qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn auth_session_module(rotation: Option<RotationConfig>) -> Module {
        let mut feature = base_feature("account");
        feature.resources.push(resource(
            "User",
            vec![builtin("email", BuiltinType::SemanticEmail, true)],
        ));
        feature.resources.push(resource(
            "UserSession",
            vec![
                field("user", TypeRef::UserDefined(qname("User")), true),
                field(
                    "token_hash",
                    TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                        algorithm: HashAlgorithm::Argon2id,
                    })),
                    true,
                ),
                builtin("expires_at", BuiltinType::DateTime, true),
            ],
        ));
        feature.auth = Some(Auth {
            identity: AuthIdentity {
                field: FieldRef {
                    resource: qname("User"),
                    field: "email".to_owned(),
                },
                public_contract: None,
            },
            password: None,
            sessions: Some(AuthSessions {
                resource: qname("UserSession"),
                ttl: "7 days".to_owned(),
                refresh: false,
                extra_columns: Vec::new(),
                access_ttl: None,
                rotation,
            }),
            mfa: None,
            oauth: Vec::new(),
            span_ref: None,
        });
        base_module(vec![feature])
    }

    #[test]
    fn emits_audit_log_down_for_modules_without_resources() {
        let module = base_module(vec![base_feature("customer")]);
        let files = emit_migrations(&module, "app");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "migrations/audit_log.down.sql");
        assert_eq!(
            files[0].contents,
            "\
-- Code generated by lazuli; DO NOT EDIT.
-- rollback for audit_log

DROP TABLE IF EXISTS audit_log;
"
        );
    }

    #[test]
    fn sorts_resources_across_features_and_numbers_resources() {
        let mut zeta = base_feature("zeta");
        zeta.resources.push(resource(
            "Zulu",
            vec![builtin("name", BuiltinType::Text, true)],
        ));
        let mut alpha = base_feature("alpha");
        alpha.resources.push(resource(
            "Beta",
            vec![builtin("name", BuiltinType::Text, true)],
        ));
        alpha.resources.push(resource(
            "Alpha",
            vec![builtin("name", BuiltinType::Text, true)],
        ));

        let module = base_module(vec![zeta, alpha]);
        let files = emit_migrations(&module, "shop");

        assert_eq!(files[0].path, "migrations/001_alpha_alpha.sql");
        assert_eq!(files[1].path, "migrations/002_alpha_beta.sql");
        assert_eq!(files[2].path, "migrations/003_zeta_zulu.sql");
        assert!(files.iter().any(|file| {
            file.path == "migrations/001_alpha_alpha.down.sql"
                && file.contents.contains("DROP TABLE IF EXISTS \"alpha\";")
        }));
        assert!(files.iter().any(|file| {
            file.path == "migrations/002_alpha_beta.down.sql"
                && file.contents.contains("DROP TABLE IF EXISTS \"beta\";")
        }));
        assert!(files.iter().any(|file| {
            file.path == "migrations/003_zeta_zulu.down.sql"
                && file.contents.contains("DROP TABLE IF EXISTS \"zulu\";")
        }));
        assert!(files[0].contents.contains("-- resource: alpha.Alpha"));
    }

    #[test]
    fn no_rotation_session_migration_stays_byte_identical() {
        let module = auth_session_module(None);
        let files = emit_migrations(&module, "auth-refresh");
        let session = files
            .iter()
            .find(|file| file.path == "migrations/002_account_user_session.sql")
            .expect("expected session migration");

        assert_eq!(
            session.contents,
            "\
-- Code generated by lazuli; DO NOT EDIT.
-- source: auth-refresh
-- resource: account.UserSession

CREATE TABLE IF NOT EXISTS \"user_session\" (
    id BIGSERIAL PRIMARY KEY,
    \"user\" BIGINT NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (\"user\") REFERENCES \"user\" (id)
);
"
        );
    }

    #[test]
    fn rotation_session_migration_adds_refresh_columns_and_indexes() {
        let module = auth_session_module(Some(RotationConfig {
            refresh_ttl: None,
            grace: None,
            theft_detection_action: None,
            span_ref: None,
        }));
        let files = emit_migrations(&module, "auth-refresh");
        let session = files
            .iter()
            .find(|file| file.path == "migrations/002_account_user_session.sql")
            .expect("expected session migration");

        assert_eq!(session.path, "migrations/002_account_user_session.sql");
        assert_eq!(
            session.contents,
            "\
-- Code generated by lazuli; DO NOT EDIT.
-- source: auth-refresh
-- resource: account.UserSession

CREATE TABLE IF NOT EXISTS \"user_session\" (
    id BIGSERIAL PRIMARY KEY,
    \"user\" BIGINT NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    refresh_token_hash TEXT NOT NULL DEFAULT '',
    refresh_expires_at TIMESTAMPTZ,
    parent_session_id BIGINT REFERENCES \"user_session\" (id) ON DELETE SET NULL,
    theft_detected_at TIMESTAMPTZ,
    FOREIGN KEY (\"user\") REFERENCES \"user\" (id)
);

CREATE INDEX ON \"user_session\" (parent_session_id) WHERE parent_session_id IS NOT NULL;

CREATE INDEX ON \"user_session\" (refresh_token_hash) WHERE refresh_token_hash != '';
"
        );
    }

    #[test]
    fn emits_down_migration_for_each_resource_with_drop_table_shape() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![builtin("name", BuiltinType::Text, true)],
        ));

        let files = emit_migrations(&base_module(vec![feature]), "crm");
        let down = files
            .iter()
            .find(|file| file.path == "migrations/001_customer_customer.down.sql")
            .expect("expected customer down migration");

        assert_eq!(
            down.contents,
            "\
-- Code generated by lazuli; DO NOT EDIT.
-- rollback for customer.Customer

DROP TABLE IF EXISTS \"customer\";
-- Note: cross-feature FK columns rely on parent feature's down migration
-- to drop their target. Run rollbacks in reverse order if cascading.
"
        );
    }

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
            sql.contains(
                "cents_currency TEXT NOT NULL CHECK (cents_currency = 'BRL') DEFAULT 'BRL'"
            )
        );
    }

    #[test]
    fn emits_generated_always_stored_for_derived_field() {
        let mut feature = base_feature("invoice");
        let mut total = builtin("total", BuiltinType::Integer, true);
        total.derived_from = Some("subtotal + tax".to_owned());
        feature.resources.push(resource(
            "Invoice",
            vec![builtin("subtotal", BuiltinType::Integer, true), total],
        ));

        let files = emit_migrations(&base_module(vec![feature]), "billing");
        let sql = &files[0].contents;

        assert!(sql.contains("subtotal BIGINT NOT NULL,"));
        assert!(sql.contains("total BIGINT NOT NULL GENERATED ALWAYS AS (subtotal + tax) STORED"));
    }

    #[test]
    fn emits_org_tenancy_and_timestamps_from_effective_settings() {
        let mut feature = base_feature("customer");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.defaults.timestamps = true;
        feature.resources.push(resource(
            "Customer",
            vec![builtin("name", BuiltinType::Text, true)],
        ));

        let files = emit_migrations(&base_module(vec![feature]), "crm");
        let sql = &files[0].contents;

        assert!(sql.contains("org_id BIGINT NOT NULL,"));
        assert!(sql.contains("created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),"));
        assert!(sql.contains("updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()"));
    }

    #[test]
    fn resource_timestamp_override_and_soft_delete_are_reflected() {
        let mut feature = base_feature("customer");
        feature.defaults.timestamps = true;
        let mut item = resource("Customer", vec![builtin("name", BuiltinType::Text, true)]);
        item.timestamps = Some(false);
        item.soft_delete = true;
        feature.resources.push(item);

        let files = emit_migrations(&base_module(vec![feature]), "crm");
        let sql = &files[0].contents;

        assert!(!sql.contains("created_at"));
        assert!(!sql.contains("updated_at"));
        assert!(sql.contains("deleted_at TIMESTAMPTZ"));
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
            }));
        customer
            .constraints
            .push(Constraint::Unique(UniqueConstraint {
                fields: vec!["external_id".to_owned()],
                per: None,
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

        let files = emit_migrations(&module, "hostpoint");
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

        let files = emit_migrations(&module, "hostpoint");
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
        assert!(sql.contains(
            "CREATE INDEX place_coordinates_gist ON \"place\" USING GIST (coordinates);"
        ));

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
    fn emits_foreign_key_for_cross_feature_resource_ref() {
        let mut customer = base_feature("customer");
        customer.resources.push(resource(
            "Customer",
            vec![builtin("email", BuiltinType::SemanticEmail, true)],
        ));
        let mut orders = base_feature("orders");
        orders.resources.push(resource(
            "Order",
            vec![field(
                "customer",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                }),
                true,
            )],
        ));

        let files = emit_migrations(&base_module(vec![orders, customer]), "shop");
        let order_sql = files
            .iter()
            .find(|file| file.path == "migrations/002_orders_order.sql")
            .map(|file| file.contents.as_str())
            .unwrap();

        assert!(order_sql.contains("customer BIGINT NOT NULL,"));
        // FK target must match the table name emitted by
        // `emit_resource_migration` (bare `"customer"`, not the legacy
        // `<feature>_<resource>` form that produced broken migrations).
        // See migration_ddl.rs::foreign_key_constraints for the contract.
        assert!(
            order_sql.contains("FOREIGN KEY (customer) REFERENCES \"customer\" (id)"),
            "FK target must match CREATE TABLE name; got:\n{order_sql}"
        );
        assert!(
            !order_sql.contains("customer_customer"),
            "legacy `<feature>_<resource>` FK target leaked back in:\n{order_sql}"
        );
    }

    #[test]
    fn emits_foreign_key_for_same_feature_resource_ref() {
        // Regression for the same-feature FK gap (discovered during
        // bug #9 cross-feature work, 2026-05-15). A resource referencing
        // another resource in the SAME feature (e.g. `Category.parent:
        // Category` parent-child link, or `Membership.workspace:
        // Workspace` when both live in the `org` feature) must emit a
        // FOREIGN KEY constraint just like the cross-feature case.
        // Previously `foreign_key_owner` returned `None` for these and
        // the constraint silently disappeared.
        let mut org = base_feature("org");
        org.resources.push(resource(
            "Workspace",
            vec![builtin("name", BuiltinType::Text, true)],
        ));
        org.resources.push(resource(
            "Membership",
            vec![field(
                "workspace",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Workspace".to_owned(),
                }),
                true,
            )],
        ));

        let files = emit_migrations(&base_module(vec![org]), "saas");
        // Topo-sorted: Workspace (no FK deps) emits first; Membership
        // (depends on Workspace) follows. Look up by `_membership.sql`
        // suffix so the test isn't coupled to the migration index.
        let membership_sql = files
            .iter()
            .find(|file| file.path.ends_with("_org_membership.sql"))
            .map(|file| file.contents.as_str())
            .unwrap();

        assert!(
            membership_sql.contains("workspace BIGINT NOT NULL,"),
            "expected workspace FK column; got:\n{membership_sql}"
        );
        assert!(
            membership_sql.contains("FOREIGN KEY (workspace) REFERENCES \"workspace\" (id)"),
            "same-feature FK must reference the actual table; got:\n{membership_sql}"
        );

        // Topo invariant: the FK target's CREATE TABLE migration index
        // must be smaller than the referencing one (otherwise applying
        // the second fails with `relation "workspace" does not exist`).
        let workspace_idx = files
            .iter()
            .position(|file| file.path.ends_with("_org_workspace.sql"))
            .unwrap();
        let membership_idx = files
            .iter()
            .position(|file| file.path.ends_with("_org_membership.sql"))
            .unwrap();
        assert!(
            workspace_idx < membership_idx,
            "FK target migration must precede the referencing migration: workspace at {workspace_idx}, membership at {membership_idx}"
        );
    }

    #[test]
    fn fk_target_table_matches_actual_create_table_name() {
        // Regression guard for the `<feature>_<resource>` FK drift —
        // every FOREIGN KEY emitted across the module must reference a
        // table that some `CREATE TABLE` statement actually creates in
        // the same module. Without this guard the codegen can produce
        // migrations that fail to apply with `relation "X" does not
        // exist`.
        //
        // Scope note: this test only covers **cross-feature** FKs.
        // Same-feature FKs are intentionally not emitted today
        // (`foreign_key_owner` filters out same-feature refs); when
        // that gap closes the test will need a same-feature case too.
        use std::collections::HashSet;

        // Two cross-feature references: `Membership.user → account.User`
        // and `Membership.workspace → org.Workspace`. Both are
        // cross-feature so both should produce FK constraints.
        let mut account = base_feature("account");
        account.resources.push(resource(
            "User",
            vec![builtin("email", BuiltinType::SemanticEmail, true)],
        ));
        let mut org = base_feature("org");
        org.resources.push(resource(
            "Workspace",
            vec![builtin("name", BuiltinType::Text, true)],
        ));
        let mut members = base_feature("members");
        members.resources.push(resource(
            "Membership",
            vec![
                field(
                    "user",
                    TypeRef::UserDefined(QualifiedName {
                        feature: None,
                        name: "User".to_owned(),
                    }),
                    true,
                ),
                field(
                    "workspace",
                    TypeRef::UserDefined(QualifiedName {
                        feature: None,
                        name: "Workspace".to_owned(),
                    }),
                    true,
                ),
            ],
        ));

        let files = emit_migrations(&base_module(vec![account, org, members]), "saas");

        // Collect every quoted identifier that appears after `CREATE TABLE`
        // and every one that appears after `REFERENCES`. The second set
        // must be a subset of the first.
        let mut created: HashSet<String> = HashSet::new();
        let mut referenced: HashSet<String> = HashSet::new();
        for file in &files {
            for line in file.contents.lines() {
                if let Some(rest) = line
                    .trim_start()
                    .strip_prefix("CREATE TABLE IF NOT EXISTS ")
                {
                    if let Some(name) = rest
                        .split_whitespace()
                        .next()
                        .and_then(|tok| tok.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                    {
                        created.insert(name.to_owned());
                    }
                }
                if let Some(idx) = line.find("REFERENCES \"") {
                    let after = &line[idx + "REFERENCES \"".len()..];
                    if let Some(end) = after.find('"') {
                        referenced.insert(after[..end].to_owned());
                    }
                }
            }
        }

        for table in &referenced {
            assert!(
                created.contains(table),
                "FK references {:?} but no CREATE TABLE emits it; created = {:?}",
                table,
                created
            );
        }
        assert!(
            referenced.contains("user") && referenced.contains("workspace"),
            "expected cross-feature FKs to `user` and `workspace`; got {:?}",
            referenced
        );
    }

    #[test]
    fn topological_fk_order_emits_targets_before_referencers() {
        // WAR-RUNTIME-MIGRATION-03 regression. Build a cross-feature
        // dependency graph where lexical order would put the
        // referencing migration BEFORE its FK target — exercising the
        // failure mode that the topo sort closes.
        //
        // Resources:
        //   `zeta.Profile` (no deps)        — lexical last, topo any
        //   `alpha.Comment` (FK → Profile)  — lexical first, would fail
        //   `alpha.Vote`    (FK → Comment)  — chains a level deeper
        //
        // Pure lexical sort would write `001_alpha_comment.sql` first,
        // attempting to point at `zeta.profile` before its table
        // exists. Topo order must invert that.
        let mut zeta = base_feature("zeta");
        zeta.resources.push(resource(
            "Profile",
            vec![builtin("handle", BuiltinType::Text, true)],
        ));
        let mut alpha = base_feature("alpha");
        alpha.resources.push(resource(
            "Comment",
            vec![field(
                "author",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Profile".to_owned(),
                }),
                true,
            )],
        ));
        alpha.resources.push(resource(
            "Vote",
            vec![field(
                "comment",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Comment".to_owned(),
                }),
                true,
            )],
        ));

        let files = emit_migrations(&base_module(vec![alpha, zeta]), "topo");

        let pos = |suffix: &str| -> usize {
            files
                .iter()
                .position(|file| file.path.ends_with(suffix))
                .unwrap_or_else(|| {
                    panic!(
                        "expected file ending with {suffix}; got {:#?}",
                        files.iter().map(|f| &f.path).collect::<Vec<_>>()
                    )
                })
        };

        let profile = pos("_zeta_profile.sql");
        let comment = pos("_alpha_comment.sql");
        let vote = pos("_alpha_vote.sql");

        assert!(
            profile < comment,
            "profile (FK target) must precede comment (referencer); got profile={profile}, comment={comment}"
        );
        assert!(
            comment < vote,
            "comment (FK target) must precede vote (referencer); got comment={comment}, vote={vote}"
        );
    }

    #[test]
    fn topological_fk_order_falls_back_to_lexical_when_independent() {
        // Two resources with no FK relationship between them — topo
        // imposes no constraint, so the lexical (feature, resource)
        // tiebreaker decides. Without this guarantee the output would
        // be non-deterministic across runs.
        let mut a = base_feature("alpha");
        a.resources.push(resource(
            "Account",
            vec![builtin("name", BuiltinType::Text, true)],
        ));
        let mut b = base_feature("beta");
        b.resources.push(resource(
            "Bucket",
            vec![builtin("label", BuiltinType::Text, true)],
        ));

        let files = emit_migrations(&base_module(vec![b, a]), "stable");

        let account = files
            .iter()
            .position(|file| file.path.ends_with("_alpha_account.sql"))
            .unwrap();
        let bucket = files
            .iter()
            .position(|file| file.path.ends_with("_beta_bucket.sql"))
            .unwrap();
        assert!(
            account < bucket,
            "lexical tiebreaker should put alpha.account before beta.bucket; got account={account}, bucket={bucket}"
        );
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
            sql.contains(
                "encrypted_note BYTEA, -- lazuli:encrypted @key.tenant algorithm=aes_256_gcm"
            ),
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
}
