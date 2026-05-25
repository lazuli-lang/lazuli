//! App-level runtime + deploy IR — runtime units, deploy strategy,
//! migration hooks, and checkpoint pinning.
//!
//! These two siblings declare *how* an app boots and rolls forward without
//! pulling in any concrete container runtime, scheduler, or migration tool.
//! `runtime` declares the units the app exposes (each one a logical
//! process / lambda / worker) with their healthcheck + readiness wiring;
//! `deploy` declares the migration lock, destructive-migration policy,
//! and rollout strategy.
//!
//! The language stays adapter-agnostic — concrete migration runners
//! (atlas, sql-migrate, Liquibase) and concrete deploy strategies
//! (rolling, blue/green, canary) resolve at the runtime layer.
//!
//! ## Catalog
//!
//! - [`AppRuntimeUnit`] — one declarative runtime unit (server, worker,
//!   scheduler, ...). Carries `locale_negotiate` for global request-locale
//!   defaults.
//! - [`AppDeploy`] — migrations + rollback + rollout strategy block.
//! - [`DeployCheckpoint`] — pinned IR JSON snapshot for plan diffing.

use serde::{Deserialize, Serialize};

use crate::SpanRef;
use crate::nodes::app_manifest::locale::LocaleNegotiate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRuntimeUnit {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serves: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<String>,
    /// i18n bucket cycle — `locale_negotiate` decorator on the runtime
    /// unit. Declares the global default request-locale negotiation
    /// strategy. Per-api overrides live on `Api.locale_negotiate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_negotiate: Option<LocaleNegotiate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppDeploy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_lock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<String>,
    /// Migrations bucket cycle Route C — `strategy <rolling|blue_green|canary>`.
    /// Closed catalog enforced by `DEPLOY-STRATEGY-001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// Migrations bucket cycle Route C — `lock_timeout "<duration>"`.
    /// Adapter-parsed duration literal; the language keeps the literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_timeout: Option<String>,
    /// Migrations bucket cycle Route C — `pre_migration_hook "<path>"`.
    /// Optional shell hook the runtime invokes before applying migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_migration_hook: Option<String>,
    /// Migrations bucket cycle Route C — `post_migration_hook "<path>"`.
    /// Optional shell hook the runtime invokes after applying migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_migration_hook: Option<String>,
    /// Migrations bucket cycle Route C — `checkpoint <name> "<path>"`.
    /// Pins an IR JSON snapshot the runtime can diff against. `lazuli plan
    /// --check <name>` validates the snapshot's integrity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<DeployCheckpoint>,
}

/// Migrations bucket cycle Route C — declarative checkpoint pinning under
/// `app.deploy.checkpoint <name> "<path>"`. The path is captured verbatim;
/// `DEPLOY-CHECKPOINT-001` verifies the file resolves relative to
/// `app.lzi`, and `DEPLOY-CHECKPOINT-002` warns when the loaded snapshot's
/// `lazuli_version` lags the analyzer's expected version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployCheckpoint {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_runtime_unit_round_trips_with_locale_negotiate() {
        let v = AppRuntimeUnit {
            name: "web".into(),
            serves: vec!["public".into()],
            runs: vec![],
            healthcheck: Some("/healthz".into()),
            readiness: None,
            locale_negotiate: Some(LocaleNegotiate {
                source: Some("accept_language".into()),
                strategy: Some("best_match".into()),
                fallback: None,
            }),
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppRuntimeUnit = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn app_deploy_omits_optional_slots() {
        let v = AppDeploy {
            strategy: Some("rolling".into()),
            ..Default::default()
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"strategy\":\"rolling\""));
        assert!(!s.contains("rollback"));
        assert!(!s.contains("checkpoint"));
    }

    #[test]
    fn deploy_checkpoint_round_trips() {
        let v = DeployCheckpoint {
            name: "v1".into(),
            path: "./checkpoints/v1.json".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: DeployCheckpoint = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
