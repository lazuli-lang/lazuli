//! Error-handling discipline — iron-hand 4th dimension.
//!
//! Lazuli already enforces three dimensions of discipline on the
//! framework + apps: coverage (per-IR-layer test ratios), test_discipline
//! (per-handler test pairing in `.lzi`/`.lzx`), and internal_hygiene
//! (file size, rustdoc, examples, test pairing on the framework's own
//! Rust source). `error_handling` is the 4th: every layer Lazuli owns
//! must declare and propagate errors deliberately.
//!
//! ## Layers covered
//!
//! - **Framework Rust source** (`crates/lazuli_*/src/`) — no `.unwrap()`
//!   / `.expect()` / `panic!` in non-test code; pub error enums named
//!   with `Error` suffix; `#[non_exhaustive]` on pub error enums; every
//!   variant documented + carries `#[error("...")]`.
//! - **`.lzi` contract** — every command/query/api that can fail must
//!   declare exhaustive `errors:` with messageKey, HTTP status, retriable
//!   classification, and audit-emit metadata for mutative paths.
//! - **`.lzx` UX** — every view with a `submit` must declare `on_error:`;
//!   every list view must declare empty + error slots; field validation
//!   must map to the command's declared error variants.
//! - **Go handlers** (`handlers/<fn>.go`, `domain/<fn>.go`,
//!   `jobs/<fn>.go`, `integrations/<fn>.go`) — no raw `panic(...)`,
//!   no `errors.New("...")` (use declared variants), `fmt.Errorf`
//!   must use `%w` to preserve the error chain.
//!
//! ## What this module is NOT for
//!
//! - The emitted `dist/go/` Go code — that's covered by emitter test
//!   coverage in [`crates/lazuli_codegen_go`], not by lint rules.
//! - Frontend `.tsx` / `.ts` in arbitrary slots — surface is too narrow
//!   for a generic rule.
//!
//! ## Catalog (planned)
//!
//! **Framework Rust** (file walker, no `syn` dep):
//! - `INTERNAL-PANIC-UNWRAP-001` — `.unwrap()`/`.expect()`/`panic!`/
//!   `todo!`/`unimplemented!`/`unreachable!` in non-test code.
//! - `INTERNAL-ERROR-NAMING-001` — types implementing `std::error::Error`
//!   should end in `Error`.
//! - `INTERNAL-ERROR-NON-EXHAUSTIVE-001` — pub error enums need
//!   `#[non_exhaustive]` for SemVer safety.
//! - `INTERNAL-ERROR-VARIANT-DOC-001` — each variant of a pub error enum
//!   needs `#[error("...")]` and `///` docstring.
//!
//! **`.lzi` contract** (IR walker, walks user source via doctor's
//! standard `DoctorPackage` like other user rules):
//! - `ERROR-DECLARED-EXHAUSTIVE-001` — handlers with `Result` return
//!   must declare each variant in the `errors:` block.
//! - `ERROR-MESSAGE-KEY-001` — every declared error variant carries
//!   `messageKey:`.
//! - `ERROR-HTTP-STATUS-MAP-001` — every variant maps to `status: 4xx/5xx`.
//! - `ERROR-RETRIABLE-CLASS-001` — `job` / `webhook` variants declare
//!   `retriable: true/false`.
//! - `ERROR-AUDIT-EMIT-001` — mutative command errors emit audit events.
//!
//! **`.lzx` UX**:
//! - `ERROR-VIEW-ON-ERROR-001` — `view create` / `view detail` with
//!   `submit` must declare `on_error:`.
//! - `ERROR-VIEW-EMPTY-STATE-001` — `view list` declares `empty:` and
//!   `error:` slots.
//! - `ERROR-FIELD-VALIDATION-001` — form fields with `validate:` map
//!   to the command's declared error variants.
//!
//! **Go handlers** (text walker, no `go/ast` dep — same posture as
//! the Rust walker; deeper bridge-validation is a Wave-3 follow-up):
//! - `HANDLER-NO-PANIC-001` — literal `panic(...)` calls.
//! - `HANDLER-NO-STRING-ERROR-001` — `errors.New("...")` outside
//!   `var Err... = errors.New(...)` package-level declarations.
//! - `HANDLER-ERROR-WRAP-001` — `fmt.Errorf` without `%w` verb.
//!
//! ## Preset interaction
//!
//! `[doctor.error_handling].preset` mirrors the other three preset
//! mechanisms. Under `tdd-iron-hand`, every `INTERNAL-PANIC-*` /
//! `INTERNAL-ERROR-*` / `ERROR-*` / `HANDLER-*` rule fires at `Error`.

pub mod preset;
pub mod walker;

pub mod error_naming_001;
pub mod error_non_exhaustive_001;
pub mod error_variant_doc_001;
pub mod handler_no_panic_001;
pub mod handler_no_string_error_001;
pub mod panic_unwrap_001;

pub use error_naming_001::Finding as ErrorNamingFinding;
pub use error_non_exhaustive_001::Finding as ErrorNonExhaustiveFinding;
pub use error_variant_doc_001::Finding as ErrorVariantDocFinding;
pub use handler_no_panic_001::Finding as HandlerNoPanicFinding;
pub use handler_no_string_error_001::Finding as HandlerNoStringErrorFinding;
pub use panic_unwrap_001::Finding as PanicUnwrapFinding;
pub use preset::{ErrorHandlingPreset, preset_rule_severity};
pub use walker::{GoHandlerSourceFile, walk_workspace_go_handlers};
