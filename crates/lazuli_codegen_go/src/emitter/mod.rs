//! Lazuli → Go emitter scaffold (proposal §2.3, cell E1). Module layout:
//!
//! - `printer` — handwritten Go printer (mirrors
//!   `lazuli_openapi::YamlEmitter`); knows nothing about IR shapes.
//! - `imports` — three-bucket import accumulator (stdlib / Lazuli
//!   runtime / third-party).
//! - `types` — closed-catalog `TypeRef` → Go type mapping.
//! - `module` — top-level walker that drives per-feature emission
//!   and the root `go.mod`.
//!
//! Per-kind walkers (Resource, Command, Query, Api, Auth, Job,
//! Webhook, Notification, Storage, Translation, TenantMigration,
//! EventGroup) land in subsequent cells (E2-E4, G1-G7); their
//! modules will live as sibling files inside this directory.

pub mod api;
pub mod app_integration;
pub mod audit;
pub mod auth;
pub mod auth_refresh;
pub mod auto_photo;
pub mod casing;
pub mod check;
pub mod command;
pub mod cross_feature;
pub mod deps;
pub mod enums;
pub mod error_envelope;
pub mod error_resolver;
pub mod events;
pub mod handlers;
pub mod imports;
pub mod job;
pub mod lint;
pub mod mcp_server;
pub mod migration;
pub mod migration_ddl;
pub mod module;
pub mod notification;
pub mod patterns;
pub mod plan;
pub mod poller;
pub mod printer;
pub mod query;
pub mod rbac;
pub mod register;
pub mod report;
pub mod resource;
pub mod root;
pub mod schema_diff;
pub mod storage;
pub mod translation;
pub mod types;
pub mod validator_tags;
pub mod webhook;

pub use module::emit_module;
pub use validator_tags::validator_tag_body;
