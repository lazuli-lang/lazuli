//! `tenant_migration <name>` AST — schema-only migration declared on a
//! tenant axis.
//!
//! Reference: migrations bucket cycle Route C.
//!
//! A tenant migration is by design **pure schema work**. The surface
//! mirrors `Job`'s spine subset (no body styles, no `emits`, no
//! `policy`) because migrations are not allowed to publish events or
//! gate on policies — they run as a privileged operator action.
//!
//! Two target forms:
//! - Modern: `target query.<name>` / `target command.<name>` —
//!   migrates the per-row schema for a specific callable.
//! - Legacy: `target tenants <axis>` — broad axis migration.
//!
//! `JobRetry` is re-used verbatim for the `retry` slot (same backoff
//! catalog as Job).

use serde::{Deserialize, Serialize};

use super::{JobRetry, Span};

/// Migrations bucket cycle Route C — `tenant_migration <name>` AST
/// surface. Mirrors `Job`'s spine subset (no body styles, no `emits`,
/// no `policy`): a tenant migration is by design pure schema work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigration {
    pub name: String,
    /// `target query.<name>` / `target command.<name>` — required by the
    /// current surface. The legacy `target tenants <axis>` form leaves this
    /// unset and stores the axis below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    /// `axis <name>` or legacy `target tenants <axis>` — required.
    pub target_axis: String,
    /// `idempotency <path>` / legacy `idempotency by <path>` — mandatory; stored
    /// as `Option<String>` so the parser surfaces the absence as an
    /// IR-level diagnostic rather than a parse error (matches `Job`).
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — optional.
    pub retry: Option<JobRetry>,
    /// `timeout "<duration>"` — optional adapter-parsed literal.
    pub timeout: Option<String>,
    /// `handler "<path>"` — required path to the Go handler.
    pub handler: String,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_migration_axis_required_in_struct() {
        // Smoke construction: legacy `target tenants <axis>` form leaves
        // target_ref unset and still records the axis.
        let m = TenantMigration {
            name: "split_orders".into(),
            target_ref: None,
            target_axis: "workspace".into(),
            idempotency_by: Some("by row.id".into()),
            retry: None,
            timeout: None,
            handler: "./tenant/migrations/split_orders.go".into(),
            span: Span::new(0, 0),
        };
        assert_eq!(m.target_axis, "workspace");
        assert!(m.target_ref.is_none());
    }
}
