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

/// One `feature.<feat>.<slot> from <source>` binding inside a profile.
/// Wires a feature-level slot (typically an integration or env var) to
/// a concrete source per environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppBinding {
    pub target_feature: String,
    pub target_slot: String,
    pub source: String,
}

/// `pack <name> from <source>` — opt-in pre-built feature pack
/// imported into the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackUse {
    pub name: String,
    pub source: String,
}

/// Root for the app-level `registry { ... }` block. Holds every cross-
/// feature declaration: env vars, integrations, capabilities, opt-in
/// packs, tool adapters, webhook event schemas, and secret-rotation
/// profiles.
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

/// One `@tool.<name>` adapter declaration inside the registry. Pins
/// the tool's effect (read vs write — required), declares any PII
/// classes it can return, and binds an optional adapter implementation.
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

/// `pack <name> from <source>` declaration body. Declares the version
/// pin, what the pack provides (features, integrations, etc.), and the
/// feature-level capability requirements consumers must satisfy.
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

/// One `provides <kind> <name>` entry under an [`AppPack`]. Free-form
/// `kind` so future pack shapes can land without IR churn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackProvide {
    pub kind: String,
    pub name: String,
}

/// `profile <name> { ... }` — one named environment profile (`dev`,
/// `staging`, `prod`). Carries URL bindings, integration overrides,
/// and deploy knobs that vary across environments.
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

/// `urls.<target> "<url>"` entry on a profile — per-environment URL
/// for a deploy target (api, frontend, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfileUrl {
    pub target: String,
    pub url: String,
}

/// Per-profile integration override (`profile.<env>.integrations.<name>`).
/// Pins the environment-specific adapter + provenance for an integration
/// that's declared globally in the registry.
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

/// Deploy knobs declared under a profile's `deploy { ... }` block.
/// All slots optional — defaults are baked into the deploy runtime.
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

/// App-level `architecture { ... }` block — chooses between
/// monolith/services and pins the boundary-enforcement strictness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppArchitecture {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforce_service_boundaries: Option<bool>,
}

/// One `service <name> { ... }` declaration in services-mode apps.
/// Names the resources/events the service owns, its outward exposures,
/// and its event publish/consume topology.
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

/// One `exposes <kind> <target>` entry on an [`AppService`] (e.g.
/// `http /api`, `grpc Hosts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServiceExposure {
    pub kind: String,
    pub target: String,
}

/// App-level `communication { ... }` block — pins the default
/// internal/external transports, async dispatch shape, propagation
/// headers, and global timeout/retry defaults.
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

/// One `url <target> <env> "<url>"` entry in the registry — pins a
/// specific deploy target's URL for a given environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppUrl {
    pub target: String,
    pub environment: String,
    pub url: String,
}

/// One `env <NAME>: <Type> required|optional` entry in the registry.
/// Pins the type, requiredness, scope (build/runtime/both), the
/// authoring group (used for organising secrets), and the
/// environments where the variable is consumed.
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

/// One `integration <name> { ... }` entry in the registry. Names the
/// adapter, the environments it applies to, the credentials block,
/// and the data-classification tag (used by doctor).
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

/// `credentials { scope ... bindings ... }` block on an integration.
/// Scope names how the credentials are keyed (per-tenant, per-user, etc.);
/// `bindings` lists the named credential slots and their env sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentials {
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppIntegrationCredentialBinding>,
}

/// One `<name> from <env-var-or-secret>` binding inside an
/// integration's credentials block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentialBinding {
    pub name: String,
    pub source: String,
}

/// One `capability <name> <value>` entry — feature-level static
/// capability declaration (free-form name/value pair).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCapability {
    pub name: String,
    pub value: String,
}

/// Closed catalog of HTTP status codes admissible in app-level error
/// pages. Doctor rejects `error_page <status>` declarations whose
/// status is not in this list.
pub const ERROR_PAGE_STATUS_CATALOG: &[u16] =
    &[400, 401, 403, 404, 405, 410, 422, 429, 500, 502, 503, 504];

/// One `error_page <status> "<template>" [audience ...]` entry under
/// the app block. Audience scopes the page to a specific authenticated
/// surface (operator vs end-user, etc.).
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
