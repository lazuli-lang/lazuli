//! App-level registry IR — env vars, integrations, capabilities, packs,
//! tools, webhook event schemas, and secret-rotation profiles.
//!
//! `registry.lzi` is the catalog the rest of the language references via
//! the `@cap.*` / `@adapter.*` / `@tool.*` / `webhook_events.*` namespaces.
//! Lifting these declarations out of feature files into one app-scoped
//! manifest keeps the per-feature surface focused on intent ("agent uses
//! `@tool.web_search`") while the registry holds the deployment-relevant
//! detail (which provider, which env-var scope, which rotation cadence).
//!
//! ## Catalog
//!
//! - [`AppRegistry`] — root container.
//! - [`WebhookEventRegistry`] + [`WebhookEventField`] — outbound webhook
//!   schemas (with version pinning + deprecation flag); also serves the
//!   `webhook_events.<name>` named-envelope namespace for inbound shapes.
//! - [`RegistryToolEntry`] — `@tool.<name>` adapter declarations with
//!   required `effect: read | write` + optional `pii_classes`.
//! - [`AppPack`] + [`AppPackProvide`] — declarative pack imports.
//! - [`AppProfile`] + [`AppProfileUrl`] + [`AppProfileIntegration`] +
//!   [`AppProfileDeploy`] — per-environment overrides.
//! - [`AppCapability`] — registry capability slot binding (e.g.
//!   `database: @adapter.postgres`).
//! - [`AppEnvVar`] — typed environment variable declaration.
//! - [`AppIntegration`] + [`AppIntegrationCredentials`] +
//!   [`AppIntegrationCredentialBinding`] — third-party integrations.
//! - [`AppUrl`] / [`AppService`] / [`AppServiceExposure`] / [`AppCommunication`] /
//!   [`AppArchitecture`] / [`AppBinding`] / [`AppPackUse`] / [`ErrorPage`] —
//!   smaller scattered registry-adjacent shapes lifted here for locality.

use serde::{Deserialize, Serialize};

use crate::nodes::ai_primitives::ToolEffect;
use crate::nodes::app_manifest::security::SecretRotation;
use crate::{FeatureRequirement, QualifiedName, SpanRef, is_false};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppBinding {
    pub target_feature: String,
    pub target_slot: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackUse {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRegistry {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<AppEnvVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppIntegration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AppCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<AppPack>,
    /// Cut A — `@tool.<name>` adapter declarations. Each entry pins the
    /// tool's `effect: read | write` (required) and optional
    /// `pii_classes`. Doctor diagnostic
    /// `tool_registry_effect_required_diagnostics` rejects entries that
    /// omit `effect`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<RegistryToolEntry>,
    /// Webhook event registry — canonical event schemas emitted to
    /// consumers. Legacy inbound `webhook ... payload from
    /// webhook_events.<name>` references also resolve here for named
    /// provider envelopes, but `webhook_event <name>` itself describes
    /// outbound contracts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhook_events: Vec<WebhookEventRegistry>,
    /// Roadmap §1.10 — `secret_rotation <name>` policy profiles
    /// (CL.C.5). Bound by `app.encryption.key @key.<scope>
    /// rotation_profile <name>` via `EncryptionBinding.rotation_profile`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_rotations: Vec<SecretRotation>,
}

/// Roadmap §1.11 — canonical outbound webhook event schema declared in
/// `registry.lzi` via `webhook_event <name>`. The payload is the public
/// contract consumers receive. Existing inbound `webhook ... payload from
/// webhook_events.<name>` references use this same catalog entry when a
/// provider envelope needs to be named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEventRegistry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<WebhookEventField>,
    #[serde(default = "webhook_event_default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

pub type WebhookEvent = WebhookEventRegistry;

fn webhook_event_default_version() -> u32 {
    1
}

/// Webhooks expanded cycle — one declared field inside a
/// `webhook_events.<name>` envelope. The `type_text` is kept verbatim
/// because the envelope is provider-side; `capabilities` capture any
/// `@semantic.*` / `@pii.*` decorators authored on the line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEventField {
    pub name: String,
    /// `Text`, `ID`, `Timestamp`, `Money`, ... — captured verbatim.
    pub type_text: String,
    pub required: bool,
    /// `@semantic.Email`, `@pii.contact`, ... — kept as authored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryToolEntry {
    /// Dotted path under `@tool.`, e.g. `web_search`, `calendar.create_event`.
    pub name: String,
    pub effect: ToolEffect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pii_classes: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPack {
    pub name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<AppPackProvide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<FeatureRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackProvide {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<AppProfileUrl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppProfileIntegration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<AppProfileDeploy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfileUrl {
    pub target: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfileIntegration {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppProfileDeploy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_lock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppArchitecture {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforce_service_boundaries: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppService {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposes: Vec<AppServiceExposure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServiceExposure {
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppCommunication {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asynchronous: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppUrl {
    pub target: String,
    pub environment: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEnvVar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub scope: String,
    pub name: String,
    pub type_name: String,
    pub requiredness: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegration {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_provenance: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<AppIntegrationCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_classification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentials {
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppIntegrationCredentialBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentialBinding {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCapability {
    pub name: String,
    pub value: String,
}

pub const ERROR_PAGE_STATUS_CATALOG: &[u16] =
    &[400, 401, 403, 404, 405, 410, 422, 429, 500, 502, 503, 504];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorPage {
    pub status: u16,
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_event_default_version_is_one() {
        let json = "{\"name\":\"x\",\"payload\":[]}";
        let v: WebhookEventRegistry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(v.version, 1);
        assert!(!v.deprecated);
    }

    #[test]
    fn webhook_event_field_round_trips() {
        let v = WebhookEventField {
            name: "email".into(),
            type_text: "Text".into(),
            required: true,
            capabilities: vec!["@semantic.Email".into()],
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: WebhookEventField = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn registry_tool_entry_round_trips_with_effect() {
        let v = RegistryToolEntry {
            name: "web_search".into(),
            effect: ToolEffect::Read,
            pii_classes: vec![],
            adapter: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: RegistryToolEntry = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn app_env_var_omits_optional_group() {
        let v = AppEnvVar {
            group: None,
            scope: "runtime".into(),
            name: "DATABASE_URL".into(),
            type_name: "Text".into(),
            requiredness: "required".into(),
            environments: vec!["dev".into(), "prod".into()],
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(!s.contains("\"group\""));
        let back: AppEnvVar = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn error_page_status_catalog_includes_429() {
        assert!(ERROR_PAGE_STATUS_CATALOG.contains(&429));
        assert!(ERROR_PAGE_STATUS_CATALOG.contains(&500));
    }
}
