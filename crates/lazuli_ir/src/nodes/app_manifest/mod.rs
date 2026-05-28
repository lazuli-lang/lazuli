//! App manifest IR — the lowered shape of `app.lzi` + `registry.lzi`.
//!
//! [`AppManifest`] is the *root* of the operational contract. It pulls
//! together every cross-cutting concern that lives outside any single
//! feature: locale, security headers, cookie hygiene, proxy trust, request
//! limits, runtime units, deploy strategy, observability, integrations, and
//! the registry catalog of `@cap.*` / `@adapter.*` / `@tool.*` bindings.
//!
//! ## Why one struct?
//!
//! `app.lzi` is one document and the analyzer lowers it into one
//! [`AppManifest`]. Splitting [`AppManifest`] into multiple top-level structs
//! would force consumers (codegens, doctor, MCP, planner) to thread three
//! arguments instead of one, and it would lose the editorial signal that
//! everything here is "the app contract".
//!
//! The struct is broad but flat: each slot is `Option<…>` or `Vec<…>`, all
//! default-friendly. Authors only set what they want to override.
//!
//! ## Wave R3-F split
//!
//! The supporting types are organised by editorial concern:
//!
//! - [`security`] — headers, cookies, proxy trust, limits, CORS,
//!   secret-rotation profiles.
//! - [`registry`] — env vars, integrations, capabilities, packs, tools,
//!   webhook event schemas, plus the registry-adjacent shapes (`AppService`,
//!   `AppUrl`, `ErrorPage`, …).
//! - [`runtime`] — runtime units + deploy strategy + checkpoint pinning.
//! - [`observability`] — logging, tracing, panic-recovery policy.
//! - [`locale`] — supported BCP-47 tags + fallback graph + per-feature
//!   translation catalogs.
//!
//! The crate root re-exports every public type from these submodules via
//! `pub use nodes::app_manifest::*` so the ABI surface stays
//! `lazuli_ir::AppManifest`, `lazuli_ir::AppHeaders`, …
//!
//! ## See also
//!
//! - [`crate::ExperienceModule`] — sibling root for `.lzx` documents.
//! - `docs/proposals/` Cut A.11, the Roadmap §1.x bucket cycle notes, and
//!   `bucket-observability-cycle.md` for the concrete design history.

pub mod locale;
pub mod observability;
pub mod registry;
pub mod runtime;
pub mod security;

pub use locale::{
    AppLocale, LocaleFallback, LocaleNegotiate, Translation, TranslationKey, TranslationPluralArm,
    TranslationVariant,
};
pub use observability::{AppLogging, AppObservability, AppTracing};
pub use registry::{
    AppArchitecture, AppBinding, AppCapability, AppCommunication, AppEnvVar, AppIntegration,
    AppIntegrationCredentialBinding, AppIntegrationCredentials, AppPack, AppPackProvide,
    AppPackUse, AppProfile, AppProfileDeploy, AppProfileIntegration, AppProfileUrl, AppRegistry,
    AppService, AppServiceExposure, AppUrl, ERROR_PAGE_STATUS_CATALOG, ErrorPage,
    RegistryToolEntry, WebhookEvent, WebhookEventField, WebhookEventRegistry,
};
pub use runtime::{AppDeploy, AppRuntimeUnit, DeployCheckpoint};
pub use security::{
    AppCookie, AppCors, AppCorsOriginRule, AppHeaders, AppHsts, AppLimits, AppProxy, CookieProfile,
    SecretRotation,
};
use serde::{Deserialize, Serialize};

use crate::SpanRef;
use crate::encryption::EncryptionBinding;
use crate::nodes::experience::RouteGuardDefaults;

/// Lowered shape of `app.lzi` + `registry.lzi`.
///
/// Observability bucket cycle row 36 — `AppManifest` no longer
/// derives `Eq` because the new `logging.sample_rate` /
/// `tracing.sample_rate` fields are `Option<f64>`. `f64` is
/// intentionally non-`Eq` due to NaN; `PartialEq` is sufficient for
/// the snapshot / fixture-equality assertions that depend on this
/// struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Pinned Lazuli runtime version. Format: "<major>.<minor>" string,
    /// e.g. "0.12". Compared against LZIR_SCHEMA at doctor time.
    /// Missing pin is warning in 0.x, error in 1.0+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lazuli_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_failed_redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_found: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_pages: Vec<ErrorPage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<AppPackUse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<AppArchitecture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<AppService>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<AppCommunication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<AppUrl>,
    /// Cut A.11 — CORS allowlist per environment. The runtime
    /// materialises browser-side middleware from this declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cors: Option<AppCors>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<AppEnvVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppIntegration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AppCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime: Vec<AppRuntimeUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<AppDeploy>,
    /// Observability bucket cycle row 36 — `app.logging` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<AppLogging>,
    /// Observability bucket cycle row 36 — `app.tracing` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracing: Option<AppTracing>,
    /// App observability policy for panic recovery and typed error
    /// projection. Optional; runtime defaults apply when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<AppObservability>,
    /// i18n bucket cycle — typed `locale` block. Supersedes the bare
    /// scalar `default_locale` when both are present; the analyzer
    /// copies `locale.default` into `default_locale` for back-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<AppLocale>,
    /// Encryption bucket cycle — typed `encryption` block. One
    /// `EncryptionBinding` per `@key.<scope>` referenced by any
    /// `@cap.Encrypted` / `@cap.E2ee` field site in the capsule.
    /// See `docs/proposals/encryption-vocab.md` §Lowering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encryption_bindings: Vec<EncryptionBinding>,
    /// Roadmap §1.2 — typed `cookie` block (CL.C.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<AppCookie>,
    /// Roadmap §1.2 — typed `proxy` block (CL.C.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<AppProxy>,
    /// Roadmap §1.2 — typed `limits` block (CL.C.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<AppLimits>,
    /// Roadmap §1.10 — typed security `headers` block (CL.C.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<AppHeaders>,
    /// `ir-route-guards` §3.6 — app-level route-guard defaults block.
    /// Layer 3 of the resolution chain. When `None`, runtime falls back
    /// to built-in framework defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_guard: Option<RouteGuardDefaults>,
    /// `ir-route-guards` §3.7 — `actor_query <feature>.query.<name>`.
    /// Declares which query the runtime calls to resolve the current
    /// actor. Required (doctor ROUTE-GUARD-003) when any non-public
    /// route exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_manifest_minimal_round_trip() {
        let json = "{\"name\":\"myapp\"}";
        let v: AppManifest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(v.name, "myapp");
        assert!(v.title.is_none());
        assert!(v.targets.is_empty());
        let s = serde_json::to_string(&v).expect("serialize");
        // Sanity: the minimal envelope omits every optional slot.
        assert_eq!(s, "{\"name\":\"myapp\"}");
    }

    #[test]
    fn app_manifest_carries_runtime_and_locale_together() {
        let v = AppManifest {
            name: "shop".into(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: vec![],
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: vec![],
            uses: vec![],
            packs: vec![],
            bindings: vec![],
            architecture: None,
            services: vec![],
            communication: None,
            environments: vec![],
            urls: vec![],
            cors: None,
            env: vec![],
            integrations: vec![],
            capabilities: vec![],
            runtime: vec![AppRuntimeUnit {
                name: "web".into(),
                serves: vec![],
                runs: vec![],
                healthcheck: None,
                readiness: None,
                locale_negotiate: None,
            }],
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: Some(AppLocale {
                default: "pt-BR".into(),
                supported: vec!["pt-BR".into()],
                fallbacks: vec![],
            }),
            encryption_bindings: vec![],
            cookie: None,
            proxy: None,
            limits: None,
            headers: None,
            route_guard: None,
            actor_query: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppManifest = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
