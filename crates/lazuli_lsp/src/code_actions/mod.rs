//! LSP code-action providers, grouped by the contract they back.
//!
//! Each submodule owns one **family** of quickfixes — one proposal /
//! one IR slice — so the dispatch wiring in `Backend::code_action`
//! stays a flat list of `actions.extend(family::family_code_actions(...))`
//! calls. That contract is the only thing the LSP runtime knows about
//! these modules; the rest is provider-specific.
//!
//! Families currently:
//!
//! * `auth_refresh` — auth.sessions rotation scaffolds (proposal
//!   `docs/proposals/auth-refresh-rotation.md`).
//! * `error_vocab` — IR Error-Vocab scaffolds for `errors` blocks and
//!   `when_denied` lines (proposal
//!   `docs/proposals/ir-error-messages-vocab.md` §7.4).
//! * `route_guard` — view-level policy scaffolds + `app.route_guard`
//!   defaults / `actor_query` stubs (proposal
//!   `docs/proposals/route-guards-and-redirects.md`).
//! * `lifecycle_gate` — view-level `requires_lifecycle` scaffolds + resume
//!   block fixes (proposal `docs/proposals/ir-lifecycle-route-gates.md`).
//!
//! ## See also
//! * `lib.rs::Backend::code_action` — dispatch table; appends per-family
//!   action vectors and sets the default `quickfix` kind.
//! * `lib.rs::simple_edit_action` — shared single-edit builder used by
//!   `route_guard` and `lifecycle_gate`.

pub mod auth_refresh;
pub mod error_vocab;
pub mod lifecycle_gate;
pub mod route_guard;
