//! Cell E3 — `Command` kind emission. Walks every `Command` declared
//! on a feature and emits the typed `<Verb><Resource>Input` struct (when
//! `CommandInput::Typed`) plus the `lazuli.Command[I, O]` value into
//! `<feature>/command.gen.go`.
//!
//! Proposal references:
//! - §3.2 — `lazuli.Command[I,O]` value shape (proven by the runtime
//!   spike at `dist/go/customer/customer.gen.go:48-122`).
//! - §4.1 — Tier 4 slots (`Command.approval`, `Command.external_calls`,
//!   `Command.timeout`, `Command.retry`, `Command.idempotency`,
//!   `Command.deprecated`) lower to the Lazuli Go lib backfill on
//!   `Command[I, O]`.
//! - §11 — boundary discipline: every `lazuli.*` reference flows
//!   through `types::go_type_for` so `imports::ImportSet` records
//!   `lazuli.dev/runtime/lazuli` once for the whole file.
//!
//! ## Effect / Output type axis
//!
//! `CommandEffect` decides the `Command[I, O]` `O` parameter:
//! - `Creates(resource, ...)` → resource pascal name (e.g. `Customer`).
//! - `Updates(resource, ...)` → resource pascal name (Lazuli Go lib's
//!   `UpdatesEffect` returns the loaded row, mirroring `Creates`).
//! - `Deletes(resource)` → resource pascal name (same — the runtime
//!   returns the row the soft-delete touched).
//! - `Returns(typeref)` → emitter resolves the typeref via
//!   `types::go_type_for` (e.g. `Customer`, `[]Tag`, `lazuli.Money`).
//!   This is Returns from §3.2 — pure request/response commands.
//! - `None` → `struct{}` (no effect declared; legacy lowering path).
//!
//! ## Bindings axis
//!
//! `Command.lets` carries `let <name> = <expr>` lines; the spike used
//! these to populate the `Bindings` body. For E3 we lower the simpler
//! and exact source: `CommandEffect::Creates.assignments` /
//! `UpdateEffect.assignments` already carry the structural form
//! `<col> = <expr>`. Each Assignment's `value: Expr::Path(...)`
//! becomes a `lazuli.FromInput("...")` / `lazuli.FromCtx("...")`
//! call; literals fall back to `lazuli.FromConst(<lit>)`.
//!
//! ## Determinism
//!
//! Commands are sorted by `Command.name` before emission. The IR `Vec`
//! ordering mirrors the source `.lzi` lexical order which is already
//! stable per-feature; sorting keeps cross-feature byte-equivalence
//! intact even when IR reordering happens elsewhere.
//!
//! ## Layout (Rails-style split — wave R6 / R8-2)
//!
//! - `file_emit` — file-level walker (`emit_command_file`) +
//!                 `command_wrap_buckets`. Re-exported as
//!                 `super::command::emit_command_file` so external
//!                 callers don't notice the split.
//! - `format`    — `format_expr`, `format_path`, `format_args_key`,
//!                 `sorted_arg_strings`, `format_qname`, `pascal_case`,
//!                 `lower_camel`, `escape_string`, `write_section_banner`,
//!                 `register_imports_for_type`. Shared with sibling
//!                 submodules (`effects`, `lifecycle`, `policy`, `scope`,
//!                 `semantic`, `tier4`).
//! - `naming`    — Go-identifier resolution for the command tuple
//!                 (input struct, var, handler) + Output type +
//!                 `zero_value_for_go_type`.
//! - `emit`      — per-command emission (`emit_command`,
//!                 `emit_command_handler_wrapper`, `partition_gates`,
//!                 `emit_command_gate_prelude`, `emit_input_struct`).

mod effects;
mod emit;
mod file_emit;
mod format;
mod lifecycle;
mod naming;
mod policy;
mod scope;
mod semantic;
#[cfg(test)]
mod test_support;
mod tier4;
mod wrap;

// Test-host siblings — each owns a coherent sub-cluster of one
// production submodule's tests. Wired here (not inside the parent
// submodule) so the `#[path]`-implicit child resolution picks up the
// sibling `.rs` files at `command/<name>.rs` instead of
// `command/<parent>/<name>.rs`. See each sibling's `//!` header for
// the per-file coverage map.
#[cfg(test)]
mod emit_effect_dispatch_tests;
#[cfg(test)]
mod owner_scope_sql_tests;
#[cfg(test)]
mod scope_owner_tests;
#[cfg(test)]
mod scope_where_keys_tests;

// Sibling emitters address command helpers via `super::command::<name>`.
// The re-exports below keep that surface stable after the Rails-style
// split moved each helper into a `command/*.rs` submodule. Visibility is
// `pub(crate)` to mirror the original `pub(super)` reach (cross-emitter
// consumers in `handlers`, `api`, `query/header`, `register`,
// `error_resolver`, `auto_photo`).
pub use file_emit::emit_command_file;
pub(crate) use format::{
    escape_string, format_args_key, format_expr, format_path, lower_camel, pascal_case,
    sorted_arg_strings,
};
pub(crate) use naming::{
    command_input_struct_name, command_var_name, effect_resource_pascal, resource_var_for_qname,
    zero_value_for_go_type,
};
pub(in crate::emitter) use policy::format_policy_with_expr_public;
pub(in crate::emitter) use tier4::{format_deprecation_replacement, format_rate_limit_struct};
