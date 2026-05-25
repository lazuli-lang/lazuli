//! Internal-hygiene rules — Lazuli framework dogfooding its own doctor.
//!
//! Every rule under `lazuli_doctor::*` (except this module) audits user
//! `.lzi` / `.lzx` source. `internal_hygiene` is the counterpart: it
//! audits the framework's own Rust source — file size, missing rustdoc,
//! absent `## Examples` blocks, unpaired tests. The same enforcement
//! discipline Lazuli applies to its users is applied to itself.
//!
//! ## When this module fires
//!
//! Rules in this module are invoked only when `lazuli doctor` runs with
//! the new `--self` flag (Wave 3). The default `lazuli doctor <path>`
//! mode never walks Rust source — it remains IR-only, by design. The
//! `--self` posture is for the framework's CI pipeline + the developer
//! running `cargo run -- doctor --self` locally before pushing.
//!
//! ## Catalog
//!
//! - [`file_size_001`] (`INTERNAL-FILE-SIZE-001`) — flags `.rs` files
//!   above the configured threshold. Default 2000 LOC warn / 5000 LOC
//!   error; configurable via clippy.toml-style settings later.
//! - [`undoc_pub_001`] (`INTERNAL-UNDOC-PUB-001`) — flags `pub fn` /
//!   `pub struct` / `pub enum` / `pub trait` without a leading `///`
//!   doc comment.
//! - [`no_example_001`] (`INTERNAL-NO-EXAMPLE-001`) — flags `pub fn`
//!   whose docstring lacks a `## Examples` section (or `/// ```rust`
//!   block). Compilable examples enforced via `cargo test --doc` in CI.
//! - [`test_pairing_001`] (`INTERNAL-TEST-PAIRING-001`) — flags `.rs`
//!   files that neither contain `#[cfg(test)] mod tests` inline nor
//!   have a sibling `<name>_test.rs` / `tests/<name>.rs`.
//!
//! ## Preset interaction
//!
//! `[doctor.internal_hygiene].preset` mirrors the test_discipline
//! preset mechanism. Under `tdd-iron-hand`, all four rules fire at
//! `Error` regardless of profile — workspace-wide editorial veto for
//! the framework's own CI.
//!
//! ## See also
//!
//! - [`walker`] — shared filesystem walker used by every rule here
//!   (std-only `fs::read_dir`-based; no `walkdir` dep).
//! - [`preset`] — `InternalHygienePreset` enum, parsing, and severity
//!   resolver. Mirrors [`crate::test_discipline::preset`].
//! - `Lazurite.toml` at workspace root sets
//!   `[doctor.internal_hygiene].preset = "tdd-iron-hand"`.
//! - `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 3 — design.

pub mod file_size_001;
pub mod no_example_001;
pub mod preset;
pub mod test_pairing_001;
pub mod undoc_pub_001;
pub mod walker;

pub use file_size_001::Finding as FileSizeFinding;
pub use no_example_001::Finding as NoExampleFinding;
pub use preset::{InternalHygienePreset, preset_rule_severity};
pub use test_pairing_001::Finding as TestPairingFinding;
pub use undoc_pub_001::Finding as UndocPubFinding;
pub use walker::{RustSourceFile, walk_workspace_rust_sources};
