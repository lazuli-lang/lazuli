//! Diagnostics for the `command` family.
//!
//! Commands are the write-side primitive of Lazuli features. This module
//! owns the file-local checks that operate on canonical command blocks:
//!
//! | Producer | Concern |
//! |---|---|
//! | [`command_contract_diagnostics`] | per-command structural checks (policy, route, short-input inference, `creates ... from input` consumption). Walks the source once, accumulating [`CanonicalCommandFacts`] per command block, and flushes via [`command_diagnostics`]. |
//! | [`command_diagnostics`] | pure facts-to-diagnostics dispatch — called by `command_contract_diagnostics` and exported for any caller that has already collected facts (the LSP doctor pipeline). |
//! | [`command_validator_diagnostics`] | flags `let result = @validator.X` bindings that are never consumed by a downstream `validate` or `requires`, so the command can continue silently after a failed validator. |
//!
//! ## Helpers exposed at `crate::*`
//!
//! The diagnostic builders and small parsers (`command_name`,
//! `command_route_slot`, `command_write_effect`, `command_short_input_fields`,
//! `route_references`, `input_references`, `command_policy_diagnostic`,
//! `command_route_diagnostic`, `command_default_route_diagnostic`,
//! `command_short_input_diagnostic`,
//! `command_short_input_without_resource_diagnostic`,
//! `command_short_input_ambiguous_resource_diagnostic`,
//! `command_from_input_unconsumed_diagnostic`) are re-exported at the crate
//! root via `pub(crate) use diagnostics::command::*;` in `lib.rs`, so the
//! existing `crate::command_write_effect` etc. paths (used by
//! `diagnostics/policy.rs`, `diagnostics/app/child.rs`,
//! `diagnostics/external.rs`, `diagnostics/lzx.rs`, and the code-actions
//! layer) keep working.

// Sibling modules — concerns split per the doc comment above. The
// `pub(crate) use` blocks below re-export through `mod.rs` so the
// crate-root `pub(crate) use diagnostics::command::*;` glob in
// `lib.rs` continues to surface every symbol at its original
// `crate::<name>` path. `#[allow(unused_imports)]` silences the
// false-positive "unused" warning that fires when a `pub(crate) use`
// is consumed only via a downstream re-export.
mod builders;
mod contract;
mod validator;

#[allow(unused_imports)]
pub(crate) use builders::{
    command_default_route_diagnostic, command_from_input_unconsumed_diagnostic,
    command_policy_diagnostic, command_route_diagnostic,
    command_short_input_ambiguous_resource_diagnostic, command_short_input_diagnostic,
    command_short_input_without_resource_diagnostic,
};
#[allow(unused_imports)]
pub(crate) use contract::{
    CanonicalCommandFacts, command_contract_diagnostics, command_diagnostics, command_name,
    command_route_slot, command_short_input_fields, command_write_effect, input_references,
    route_references,
};
#[allow(unused_imports)]
pub(crate) use validator::command_validator_diagnostics;
