//! IR node families — one submodule per declarative slot of the language.
//!
//! Historically every IR type lived in `lazuli_ir/src/lib.rs`. As the language
//! grew (Phase 1 → Phase L Tier 4 → Cut A) the file crossed 7,800 LOC and
//! lost the per-family locality that makes a Rails-style codebase legible:
//! you could no longer ask *"what shape does `auth` have?"* without scrolling
//! past a dozen unrelated types.
//!
//! Wave 4.1 of the rails-style refactor (see
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` §W4.1, and the wave plan at
//! `C:/Users/lucas/.claude/plans/investiga-lazuli-e-bora-hashed-bengio.md`)
//! splits the IR by **type family**, each family living in its own file with
//! Rails-style prose at the top. The split is **purely organizational** — the
//! ABI surface is unchanged because [`crate::lib`] re-exports every public
//! type via `pub use nodes::<family>::*`.
//!
//! ## Wire-thin guarantee
//!
//! Every file in this module is a **pure data declaration** — `pub struct` /
//! `pub enum` plus tiny inherent impls that compute values from the struct's
//! own fields. Cross-family logic (lowering, validation, codegen) lives in
//! `lazuli_analyzer`, `lazuli_doctor`, `lazuli_codegen_*` — never here.
//!
//! ## Catalog
//!
//! Wave 4.1 starts with [`auth`] as the proof-of-concept extraction; the
//! remaining families (`command`, `query`, `event`, `workflow`, `resource`,
//! `agent`, …) follow as separate cells once the auth split lands cleanly.
//!
//! ## See also
//!
//! - [`crate::LZIR_SCHEMA`] — JSON ABI version pin
//! - [`crate::encryption`] — encryption capability vocabulary (pre-existing
//!   submodule; uses the same pattern this module generalizes)

pub mod async_work;
pub mod auth;
pub mod capability;
pub mod error_vocab;
pub mod plan_and_gate;
pub mod report;
