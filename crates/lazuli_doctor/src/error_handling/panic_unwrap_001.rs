//! `INTERNAL-PANIC-UNWRAP-001` — flag panic-prone constructs in
//! framework non-test code.
//!
//! Scans every `crates/lazuli_*/src/*.rs` library file (skipping
//! `tests/`, `examples/`, `benches/`, and inside `#[cfg(test)] mod`
//! blocks) for any of:
//!
//! - `.unwrap()` / `.expect("...")` on `Option` / `Result`
//! - `panic!(...)`, `todo!(...)`, `unimplemented!(...)`, `unreachable!(...)`
//!
//! Most `lazuli_doctor` consumers are LSP-side live-squiggle paths or
//! `cargo run -- doctor --self` in CI. Both want a non-`syn` line-based
//! heuristic so cold start stays sub-second. The rule walks the file
//! line-by-line tracking a single piece of state: depth inside a
//! `#[cfg(test)]`-attributed `mod ... { ... }` block. Inside such
//! blocks, the rule is silent — tests are expected to panic on
//! assertion failure.
//!
//! ## Why
//!
//! `.unwrap()` in framework library code is a latent panic that ships
//! to the user as a runtime crash with `panicked at 'called Option::unwrap()
//! on a None value'`. Every framework panic should either be:
//!
//! - propagated via `?` with `.context(...)` (the anyhow path), OR
//! - a typed error from `thiserror::Error` returned via `Result<_, E>`, OR
//! - a documented invariant the caller already guarantees (in which
//!   case `severity_override` with `reason` is the escape hatch).
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## What's NOT flagged
//!
//! - Code inside `#[cfg(test)] mod tests { ... }` (test panics are
//!   normal).
//! - Code in files under `tests/` / `examples/` / `benches/` (the
//!   walker tags `is_library_src = false`; this rule respects it).
//! - `Result::ok()` / `Result::err()` (not panic-prone).
//! - Macros in strings or comments — the heuristic uses simple substring
//!   matching, not lexical analysis. False positives in `r#"..."#` fixture
//!   content are flagged; author tags via `severity_override`.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::panic_unwrap_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! for f in &findings {
//!     println!("{}:{} {}", f.path.display(), f.line, f.construct);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::preset`] — preset wiring; iron-hand
//!   escalates this rule to `Error`.

use std::path::{Path, PathBuf};

use crate::internal_hygiene::walker::RustSourceFile;

include!("panic_unwrap_001_p1.rs");
include!("panic_unwrap_001_p2.rs");
