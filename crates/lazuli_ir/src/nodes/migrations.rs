//! Tenant-migration IR — `tenant_migration <name>` declarations.
//!
//! Tenant migrations are the bucket-cycle Route C primitive that handles
//! per-tenant data and schema rewrites. They mirror [`crate::Job`]'s spine
//! (idempotency / retry / timeout / handler) but are scoped to one tenancy
//! axis at a time so the runtime can fan out the rewrite without
//! collapsing tenants into a single critical section. Where `Job` is the
//! generalist async-work primitive, `TenantMigration` is the typed
//! schema-aware sibling — it always carries an `axis`, always carries an
//! `idempotency by` key, and always lowers through the same predictable
//! handler-call shape.
//!
//! ## Why mandatory idempotency
//!
//! [`TenantMigration::idempotency`] is required because schema migrations
//! are not safely re-runnable without a key. The closed-grammar rule is
//! enforced upstream by parser-side `TM-IDEMP-001` and doctor; the IR
//! shape encodes the invariant by giving the field a non-`Option` type.
//! Authoring a migration without `idempotency by` is impossible — both
//! the syntax and the IR refuse it.
//!
//! ## Target shape
//!
//! [`TenantMigrationTargetOperation`] captures the `target query.<name>`
//! / `target command.<name>` discriminator with the optional cross-feature
//! `feature` qualifier. The `axis` slot on [`TenantMigrationTarget`] is
//! the verbatim tenancy axis (`org`, `team`, …) that doctor cross-checks
//! against `defaults.tenancy` declared in the same feature.
//!
//! ## See also
//!
//! - `docs/proposals/bucket-migrations-cycle.md` — Route C design.
//! - [`crate::Job`] — generalist async-work primitive.
//! - [`crate::IdempotencyKey`] — shared idempotency shape.

use serde::{Deserialize, Serialize};

use crate::{IdempotencyKey, PathRef, RetryPolicy, SpanRef};

/// Migrations bucket cycle Route C — `tenant_migration <name>` declaration.
///
/// Mirrors `Job`'s spine (idempotency / retry / timeout / handler) but is
/// scoped to per-tenant data/schema migrations. The closed body shape is:
/// `target query.<name>|command.<name>`, `axis <name>`, `idempotency <path>`
/// (mandatory), `retry <count> backoff <strategy>`, `timeout "<duration>"`,
/// and `handler "<path>"`.
///
/// Doctor cross-checks:
///   - `TM-AXIS-001` — `target` axis matches a `defaults.tenancy` axis
///     in the same feature.
///   - `TM-IDEMP-001` — `idempotency by` is mandatory; absence is an
///     error because schema migrations are not safely re-runnable
///     without an idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigration {
    pub name: String,
    pub target: TenantMigrationTarget,
    /// Mandatory; absence triggers `TM-IDEMP-001`.
    pub idempotency: IdempotencyKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub handler: PathRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Migrations bucket cycle Route C — `target tenants <axis>` fanout
/// directive for tenant migrations. Captures the axis verbatim; doctor
/// cross-checks against `defaults.tenancy` declared in the same feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigrationTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<TenantMigrationTargetOperation>,
    pub axis: String,
}

/// Closed catalog of operations a [`TenantMigrationTarget`] can
/// reference. The migration runtime re-issues the operation per-tenant
/// during the migration window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TenantMigrationTargetOperation {
    /// `query.<kind> <name>` reference — read.
    Query {
        feature: Option<String>,
        name: String,
    },
    /// `command <name>` reference — write.
    Command {
        feature: Option<String>,
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Path;
    use serde_json::json;

    fn sample() -> TenantMigration {
        TenantMigration {
            name: "backfill_org_settings".to_owned(),
            target: TenantMigrationTarget {
                operation: Some(TenantMigrationTargetOperation::Command {
                    feature: None,
                    name: "rewrite_org_settings".to_owned(),
                }),
                axis: "org".to_owned(),
            },
            idempotency: IdempotencyKey {
                by: Path::from_segments(["payload", "org_id"]),
            },
            retry: None,
            timeout: Some("30s".to_owned()),
            handler: PathRef::convention("./migrations/backfill.go"),
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn tenant_migration_round_trips_through_json() {
        let tm = sample();
        let json = serde_json::to_value(&tm).unwrap();
        let back: TenantMigration = serde_json::from_value(json).unwrap();
        assert_eq!(back, tm);
    }

    #[test]
    fn tenant_migration_target_operation_serializes_with_kind_tag() {
        let tm = sample();
        let value = serde_json::to_value(&tm).unwrap();
        assert_eq!(value["target"]["operation"]["kind"], json!("command"));
        assert_eq!(
            value["target"]["operation"]["name"],
            json!("rewrite_org_settings")
        );
    }

    #[test]
    fn tenant_migration_omits_optional_fields_when_unset() {
        let mut tm = sample();
        tm.timeout = None;
        let value = serde_json::to_value(&tm).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("retry"));
        assert!(!obj.contains_key("timeout"));
        assert!(!obj.contains_key("previous_names"));
        assert!(!obj.contains_key("span_ref"));
    }

    #[test]
    fn tenant_migration_query_target_round_trips() {
        let op = TenantMigrationTargetOperation::Query {
            feature: Some("billing".to_owned()),
            name: "list_orgs".to_owned(),
        };
        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["kind"], json!("query"));
        assert_eq!(json["feature"], json!("billing"));
        let back: TenantMigrationTargetOperation = serde_json::from_value(json).unwrap();
        assert_eq!(back, op);
    }
}
