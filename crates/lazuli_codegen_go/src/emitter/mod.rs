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

pub mod cross_feature;
pub mod enums;
pub mod imports;
pub mod module;
pub mod printer;
pub mod resource;
pub mod types;

pub use module::emit_module;
