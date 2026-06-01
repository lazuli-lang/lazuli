//! Escape-hatch visibility rules (spec 0010).
//!
//! Lazuli's promise is that a cold read of the `.lzi` is the source of
//! truth for a feature's effects, reads, policies, and tenancy. These
//! three rules close the accidental holes that break that promise — each
//! pinned by a live pilot instance:
//!
//! - [`rawsql_in_handler_001`] (`ESC-RAWSQL-IN-HANDLER-001`) — raw SQL
//!   hidden in a `@fn` Go handler for a read the `.lzi` declares only as
//!   an opaque `fn ...: Function[...]` (no `query.sql`, no `returns`).
//!   Waivable-to-convert, not waivable-to-silence (ADR 0010, Decision 1).
//! - [`sql_tenancy_contract_001`] (`ESC-SQL-TENANCY-CONTRACT-001`) — a
//!   `query.sql` mixing `:named` and `$N` binding, or referencing a param
//!   the `.lzi` block doesn't declare. The regression net for spec 0011.
//! - [`scope_override_unguarded_001`] (`ESC-SCOPE-OVERRIDE-UNGUARDED-001`)
//!   — a `query.sql` with no tenant predicate AND no `@actor.<privileged>`
//!   policy guard (a SQL comment is not a guard).
//!
//! All three reuse the existing surfaces: [`crate::handler_walker`] for Go
//! handler sites and the lowered [`lazuli_ir::Feature`] for the `.lzi` IR.
//! The `lazuli_doctor` crate carries no runtime `lazuli_analyzer`
//! dependency, so the caller (the `lazuli_doctor_run` aggregator) lowers
//! each `.lzi` and passes it in — mirroring how
//! [`crate::lzi_hygiene::feature_cohesion_002`] is wired.
//!
//! ## Preset interaction
//!
//! Governed by `[doctor.escape_hatch].preset` in `Lazurite.toml`; see
//! [`preset::preset_rule_severity`].

pub mod preset;
pub mod rawsql_in_handler_001;
pub mod scope_override_unguarded_001;
pub mod sql_tenancy_contract_001;

pub use preset::{EscapeHatchPreset, preset_rule_severity};
pub use rawsql_in_handler_001::Finding as RawSqlInHandlerFinding;
pub use scope_override_unguarded_001::Finding as ScopeOverrideUnguardedFinding;
pub use sql_tenancy_contract_001::Finding as SqlTenancyContractFinding;
