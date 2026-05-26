//! Cell I2 — Root module-level emission. Walks the module once and
//! emits the two singletons that live at the root of the generated
//! Go tree (alongside `go.mod`):
//!
//! - `main.go` — `func main()` entry point. Side-effect imports for
//!   every feature package so each feature's `init()` registers its
//!   resources/commands/queries with the Lazuli Go runtime registry.
//!   Calls `lazuli.Boot(ctx, dbURL)` with the same shape the spike
//!   already proves in `dist/go/main.go`, then gives the runtime a
//!   single registry-driven mount point before serving HTTP.
//! - `lazuli_app.gen.go` — App-level contract values. Lowers
//!   `module.app: Option<AppManifest>` into per-bucket contract
//!   declarations using the contract types the Lazuli Go lib already
//!   exposes:
//!     - `app.locale` → `i18n.LocaleContract`
//!     - `app.logging` → `observability.LoggingContract`
//!     - `app.tracing` → `observability.TracingContract`
//!     - `app.encryption` → `[]encryption.Binding` + `init()` register loop
//!     - `app.cors` → `lazuli.AppCors` + `init()` middleware register
//!
//! Proposal references:
//! - §3.13 — Root-level files table.
//! - §3.13.1 — `app.routes` lowering (deferred; `lazuli.AppContract`
//!   wrapper + `lazuli.AppRoute` / `lazuli.AppCors` types do not exist
//!   on the Lazuli Go lib yet, so we emit TODO comments and skip the
//!   missing fields gracefully per the cell I2 brief). The shape the
//!   proposal sketches lands here once the runtime team adds the
//!   `lazuli.AppContract` umbrella.
//! - §5.1 — root layout (`go.mod`, `main.go`, `lazuli_app.gen.go`,
//!   per-feature dirs).
//! - §11 — boundary discipline: emitter never edits
//!   `runtime/go/lazuli/**`; missing types are surfaced as TODO
//!   comments, never silently faked.
//!
//! ## Layout (Rails-style split — wave R6)
//!
//! - `main_go`     — `emit_main_go` + `emit_main_imports`
//! - `app_gen`     — `emit_lazuli_app_gen` + locale/logging/tracing/cors
//! - `encryption`  — `EncryptionBindings` + `init()` registration loop
//! - `helpers`     — duration parser, log/format token catalogs, render
//!                   helpers shared across the file emitters.

mod app_gen;
mod encryption;
mod helpers;
mod main_go;

pub use app_gen::emit_lazuli_app_gen;
pub use main_go::emit_main_go;

/// File path emitted at the root for `main.go`.
pub(crate) const MAIN_GO_PATH: &str = "main.go";

/// File path emitted at the root for `lazuli_app.gen.go`.
pub(crate) const LAZULI_APP_PATH: &str = "lazuli_app.gen.go";


#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod app_gen_tests;
