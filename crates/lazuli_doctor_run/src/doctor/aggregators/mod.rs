//! Per-category doctor aggregators.
//!
//! Each submodule here turns a family of per-rule findings (the leaf
//! diagnostic crates under `lazuli_doctor::*` and the per-rule files
//! under sibling `doctor/<topic>/` directories) into the canonical
//! `DoctorDiagnostic` envelope. The `diagnostics()` dispatcher in
//! `doctor/mod.rs` walks each aggregator in turn and concatenates the
//! result.
//!
//! Rationale (Wave 4.3 R2): the historical `doctor/mod.rs` god-file
//! mixed three concerns into 25k lines of source — DoctorPackage's
//! field set, the per-rule plumbing, and the dispatcher itself. The
//! aggregators here separate the second concern from the other two so
//! each family can grow new rules without touching the dispatcher and
//! without the dispatcher growing per-rule mapping noise.
//!
//! Visibility rule: every aggregator function is `pub(crate)` and
//! accepts already-loaded refs (project root, security profile, Tier 3
//! facts, app/registry handles, …). They never touch `DoctorPackage`
//! directly — that keeps each aggregator unit-testable against a
//! synthetic input set without spinning up the full package load.
//!
//! See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5 for the
//! envelope contract every aggregator pushes onto the diagnostic vec.

pub(crate) mod agent;
pub(crate) mod app_contract;
pub(crate) mod app_manifest;
pub(crate) mod approval;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod auth_actor_subject;
pub(crate) mod cache;
pub(crate) mod command_routing;
pub(crate) mod correctness;
pub(crate) mod cors;
pub(crate) mod cross_feature;
pub(crate) mod deprecated;
pub(crate) mod design;
pub(crate) mod domain;
pub(crate) mod env_manifest;
pub(crate) mod error_handling_handlers;
pub(crate) mod error_vocab;
pub(crate) mod event_group;
pub(crate) mod external;
pub(crate) mod field_health;
pub(crate) mod folder;
pub(crate) mod headers_secrets;
pub(crate) mod http_hygiene;
pub(crate) mod i18n;
pub(crate) mod job_runtime_gap;
pub(crate) mod lazurite_manifest;
pub(crate) mod lifecycle;
pub(crate) mod lzi_hygiene;
pub(crate) mod lzx_ux;
pub(crate) mod migrations;
pub(crate) mod observability;
pub(crate) mod rbac_catalog;
pub(crate) mod report_storage;
pub(crate) mod runtime_version;
pub(crate) mod semantic_type;
pub(crate) mod test_discipline;
pub(crate) mod tier3;
pub(crate) mod ts_consumers;
pub(crate) mod vocab;
pub(crate) mod webhook_event_registry;
