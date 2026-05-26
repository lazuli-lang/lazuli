//! `command_is_pure_read` — the `ir-returns-list-2026-05-22 §2.2`
//! gate that lowers reads to `defineQuery` and writes to
//! `defineCommand`.
//!
//! Lifted out of the `types` god-file in the rails-style R9 split.

/// Wave 0 (ir-returns-list-2026-05-22 §2.2): a command is a *pure read*
/// when its sole declared effect is `Returns(_)`, carries no declared
/// side-effects (no event emits, no lifecycle triggers, no invalidations,
/// no external calls), is NOT synthesized from `@cap.File` (those are
/// upload-protocol commands with implicit side effects the analyzer
/// doesn't surface as `emits`/`triggers`), AND its name starts with a
/// read-verb prefix (`list_`, `get_`, `lookup_`, `search_`, `find_`,
/// `count_`).
///
/// Pure-read commands lower to `defineQuery<I, O>` on the TS side
/// (consumable via `useLazuliQuery`) so the React app gets cache +
/// refetch + suspense semantics for free, instead of `defineCommand`
/// (which forces `useLazuliCommand` and imperative call sites). The
/// wire payload is identical — only the client-side factory differs.
///
/// The name-prefix gate exists because pilots and the analyzer leave
/// the IR side-effect surface empty for many side-effecting commands —
/// e.g. `account.login` (mints a session but has no `emits` because the
/// session table is private), `request_profile_photo_upload` (mints a
/// presigned URL but has no `triggers`). Trusting only the IR's empty
/// side-effect set produced false positives (W0-5 surfaced this:
/// hostpoint app failed to typecheck because login + photo-upload
/// commands shipped as `defineQuery`, breaking existing
/// `useLazuliCommand` callsites). The name-prefix gate makes the
/// classification conservative — false negatives (a read that doesn't
/// follow the naming convention) ship as `defineCommand`, which still
/// works; false positives ship a wire mismatch, which doesn't.
pub(crate) fn command_is_pure_read(command: &lazuli_ir::Command) -> bool {
    if !matches!(command.effect, lazuli_ir::CommandEffect::Returns(_)) {
        return false;
    }
    if !command.emits.is_empty()
        || !command.triggers.is_empty()
        || !command.invalidates.is_empty()
        || !command.external_calls.is_empty()
    {
        return false;
    }
    // cap_file synth: Request/Confirm/Clear are upload-protocol writes;
    // only GetUrl is a pure read (mints a signed download URL, no
    // mutation). c-2 worker surfaced this nuance; integrated 2026-05-22.
    if command
        .synthesized_from_cap_file
        .as_ref()
        .is_some_and(|marker| marker.role != lazuli_ir::AutoPhotoCommandRole::GetUrl)
    {
        return false;
    }
    const READ_VERB_PREFIXES: &[&str] = &["list_", "get_", "lookup_", "search_", "find_", "count_"];
    READ_VERB_PREFIXES
        .iter()
        .any(|prefix| command.name.starts_with(prefix))
}
