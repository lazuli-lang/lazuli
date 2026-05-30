//! Parser for `registry.lzi` — the cross-app vocabulary registry.
//!
//! The registry catalogs the building blocks shared by the apps in a
//! workspace: env vars (`env`), external integrations (`integrations` /
//! `bindings`), capability declarations (`capabilities`), packs
//! (`packs`), LLM tools (`tools`), inbound webhook envelopes
//! (`webhook_events`), and secret-rotation policies (`secret_rotation`).
//!
//! Two entry points:
//!
//! - [`parse_app_registry`] returns just the well-formed `AppRegistry`.
//!   The rest of Lazuli uses this when it does not need defect signal.
//! - [`parse_app_registry_with_defects`] additionally returns a
//!   side-channel of [`RegistryToolEntryDefect`] for `tool <name>`
//!   entries that lack a valid `effect` (the IR cannot express the
//!   defective shape, so it would otherwise be lost). Doctor consumes
//!   this side channel via `tool_registry_effect_required_diagnostics`.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppRegistry`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`,
//!      `super::types::RegistryToolEntryDefect`.

use lazuli_ir::{
    AppCapability, AppIntegration, AppIntegrationCredentialBinding, AppIntegrationCredentials,
    AppPack, AppRegistry, QualifiedName, RegistryToolEntry, SecretRotation, ToolEffect,
    WebhookEvent,
};

use super::parsers::{
    adapter_source_provenance, leading_spaces, parse_app_env_var, parse_bindings_sugar_line,
    parse_bool, parse_credential_binding, parse_env_group_name, parse_integration_header,
    parse_pack_header, parse_pack_provide, parse_pack_requirement, parse_webhook_event_field,
    registry_child, split_items, unquote, webhook_event_name,
};
use super::types::{RegistryParseOutput, RegistryToolDefectReason, RegistryToolEntryDefect};

include!("registry_p1.rs");
include!("registry_p2.rs");
