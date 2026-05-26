//! Cell E3 — wrap-helper bucket discovery for `command.gen.go`.
//!
//! Lifted out of `file_emit.rs` (wave R8-2b) so the orchestrator stays
//! a thin walker. `command_wrap_buckets` decides which sentinel
//! buckets (auth, billing, ...) the generated `wrapErrorForHandler`
//! helper has to know about — derived purely from each command's
//! `external_calls` list.

use lazuli_ir::{Command, ReturnsEffect};

use super::super::error_envelope::{bucket_names_for_external_calls, sentinel_buckets};

/// Collect the union of sentinel error buckets every command in the
/// feature touches via `external_calls`. The result drives both the
/// `errors` / `auth` imports added by the orchestrator and the
/// `emit_wrap_helper` call site.
pub(super) fn command_wrap_buckets(
    commands: &[&Command],
) -> std::collections::BTreeSet<&'static str> {
    let referenced: std::collections::BTreeSet<&str> = commands
        .iter()
        .flat_map(|command| bucket_names_for_external_calls(&command.external_calls))
        .collect();
    sentinel_buckets(&referenced)
}

// Unused (today) but kept so the Returns/None branches can graduate
// to a typed `ReturnsEffect` codepath without re-importing the symbol.
#[allow(dead_code)]
pub(super) fn _returns_effect_compiles(_: ReturnsEffect) {}
