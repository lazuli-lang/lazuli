//! Diagnostics for the `app` operational contract family.
//!
//! The top-level `app <Name>` block declares the operational shape of a
//! single deployable Lazuli app: what it uses (features), targets (web /
//! mobile / api), environments, runtime units, deploy policy,
//! architecture, services, communication, integrations, packs, and
//! capabilities. This module owns the file-local shape check on that
//! surface.
//!
//! ## Producers
//!
//! * [`app_operational_contract_diagnostics`] — single-pass dispatcher
//!   that walks the source, accumulates [`AppOperationalFacts`] for the
//!   open app block, and flushes block-level checks via
//!   [`app_operational_block_diagnostics`]. Both live in
//!   [`operational`] so this `mod.rs` stays a slim Rails-style hub.
//!
//! ## Sub-modules
//!
//! Concern-shaped helpers live next door:
//!
//! * [`child`] — block-header recognition + scalar children
//! * [`target`] — `targets` / `urls` / `bindings` / `packs`
//! * [`env`] — `env` lines + `PUBLIC`/`EXPO_PUBLIC_` exposure rules
//! * [`integration`] — `integrations` + adapter-source provenance
//! * [`capability`] — `capabilities` line catalog
//! * [`architecture`] — `architecture` + `communication`
//! * [`service`] — `services` block + per-service facts
//! * [`runtime`] — `runtime unit` block + per-unit facts
//! * [`deploy`] — `deploy` block (mutates the parent facts)
//! * [`operational`] — the orchestrator + whole-app invariants
//!
//! Everything is `pub(crate)` and re-exported from this module so the
//! crate-root `pub(crate) use diagnostics::app::*;` in `lib.rs`
//! continues to satisfy `crate::*` resolution in
//! `diagnostics/registry.rs` and `diagnostics/profile.rs`.

mod architecture;
mod capability;
mod child;
mod deploy;
mod env;
mod integration;
mod invariants;
mod operational;
mod runtime;
mod service;
mod target;

// ABI: every `pub(crate)` helper from the sub-modules is re-exported
// here so the crate-root `pub(crate) use diagnostics::app::*;` in
// `lib.rs` keeps satisfying `crate::*` resolution from
// `diagnostics/registry.rs`, `diagnostics/profile.rs`, and any future
// consumer. The `#[allow(unused_imports)]` covers helpers that are
// currently only consumed via the lib-root glob (not from inside
// `app/mod.rs` itself).
#[allow(unused_imports)]
pub(crate) use architecture::{validate_app_architecture_line, validate_app_communication_line};
#[allow(unused_imports)]
pub(crate) use capability::validate_app_capability_line;
#[allow(unused_imports)]
pub(crate) use child::{
    app_child_block, command_name_if, is_app_scalar_child, named_block_name,
    validate_app_child_header, validate_app_scalar_child,
};
#[allow(unused_imports)]
pub(crate) use deploy::validate_app_deploy_line;
#[allow(unused_imports)]
pub(crate) use env::{
    has_public_token, parse_env_group_name, valid_env_declaration_parts, validate_app_env_line,
};
#[allow(unused_imports)]
pub(crate) use integration::{
    adapter_source_provenance, parse_app_integration_header, valid_path_segment,
    valid_pathish_tail, valid_plugin_tail, validate_app_integration_child,
    validate_app_integration_credential_line, validate_app_integration_header,
};
#[allow(unused_imports)]
pub(crate) use invariants::app_operational_block_diagnostics;
#[allow(unused_imports)]
pub(crate) use operational::{VALIDATED_APP_BLOCKS, app_operational_contract_diagnostics};
#[allow(unused_imports)]
pub(crate) use runtime::{AppRuntimeUnitFacts, validate_app_runtime_unit_child};
#[allow(unused_imports)]
pub(crate) use service::{
    AppServiceFacts, validate_app_service_child, validate_app_service_exposure_line,
};
#[allow(unused_imports)]
pub(crate) use target::{
    parse_app_binding_line, validate_app_binding_line, validate_app_pack_use_line,
    validate_app_target_line, validate_app_url_line, validate_registry_pack_child,
    validate_registry_pack_header,
};

#[derive(Debug)]
pub(crate) struct AppOperationalFacts {
    pub(crate) line_index: usize,
    pub(crate) line: String,
    pub(crate) has_uses: bool,
    pub(crate) has_targets: bool,
    pub(crate) has_environments: bool,
    pub(crate) has_runtime: bool,
    pub(crate) has_deploy: bool,
    pub(crate) has_architecture: bool,
    pub(crate) has_services: bool,
    pub(crate) has_communication: bool,
    pub(crate) deploy_has_migrations: bool,
    pub(crate) deploy_has_rollback: bool,
    pub(crate) runtime_units: Vec<AppRuntimeUnitFacts>,
    pub(crate) services: Vec<AppServiceFacts>,
}

impl AppOperationalFacts {
    pub(crate) fn new(line_index: usize, line: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            has_uses: false,
            has_targets: false,
            has_environments: false,
            has_runtime: false,
            has_deploy: false,
            has_architecture: false,
            has_services: false,
            has_communication: false,
            deploy_has_migrations: false,
            deploy_has_rollback: false,
            runtime_units: Vec::new(),
            services: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppIntegrationFacts;

impl AppIntegrationFacts {
    pub(crate) fn new() -> Self {
        Self
    }
}
