// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
use std::collections::{HashMap, HashSet};

use lazuli_doctor_config::ResolvedDoctorConfig;
use lazuli_syntax::Span;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CompletionItemKind, Diagnostic, DiagnosticSeverity, Position,
    Range, TextEdit, Url, WorkspaceEdit,
};

mod backend;
mod catalogs;
mod code_actions;
mod completion;
mod completion_items;
mod conventions;
mod diagnostics;
mod dispatch;
mod doctor_engine;
mod format;
mod handlers;
mod hover;
mod keywords;
mod lzx_completion;
mod rate_limit;
mod security_profile;
mod semantic_tokens;
mod source_diagnostics;
mod source_scan;
mod test_blocks;
mod text_utils;
mod types;

pub use backend::serve_stdio;
pub use catalogs::*;
pub use code_actions::auth_refresh::auth_refresh_code_actions;
pub use code_actions::error_vocab::error_vocab_code_actions;
pub use code_actions::lifecycle_gate::lifecycle_gate_code_actions;
pub use code_actions::route_guard::route_guard_code_actions;
pub use completion::auth_refresh::auth_refresh_completions;
pub(crate) use completion::auth_refresh::{
    AuthRotationBlock, AuthSessionsBlock, after_keyword_value_prefix,
    auth_refresh_rotation_clause_completion_items, auth_refresh_theft_action_completion_items,
    auth_rotation_has_children, auth_sessions_has_child, block_end_line,
    duration_literal_completion_items, enclosing_auth_rotation_block,
    enclosing_auth_sessions_block, has_auth_parent, is_rotation_line, is_sessions_line,
    rotation_block_snippet_completion,
};
pub(crate) use completion::cap_file::cap_file_value_completions;
pub(crate) use completion::context::{
    EFFECT_VERBS, KIND_CHILD_COMPLETIONS, RATE_LIMIT_AXES, block_kind_at,
    context_aware_completions, convention_bundle_hover, is_inside_conventions_list,
    rate_limit_axis_completions,
};
pub(crate) use completion::error_page::error_page_value_completions;
pub use completion::error_vocab::{
    error_vocab_code_resolved_hover, error_vocab_completions, error_vocab_resolved_text,
};
pub(crate) use completion::error_vocab::{
    in_feature_errors_block, lookup_feature_error_key, lookup_translation_first_variant,
};
pub use completion::input_field::input_field_completions;
pub(crate) use completion::input_field::{
    collect_command_input_and_route_params, input_dot_trigger,
};
pub(crate) use completion::namespace::{collect_namespace_names, namespace_prefix_completions};
pub(crate) use completion::owner_axis::owner_axis_through_completions;
pub use completion::transition::triggers_transition_completions;
pub(crate) use completion::transition::collect_transition_names;
pub(crate) use completion_items::{completion_items_for_uri, make_symbol, merge_completion_items};
pub use conventions::conventions_list_completions;
// Per-catalog diagnostic producers (Rails-style layout). Each
// `pub(crate) use diagnostics::<catalog>::*` line preserves the
// pre-extraction ABI: producers continue to be reachable at
// `crate::<fn>` so `dispatch.rs` and out-of-process callers see no
// change. See `diagnostics/mod.rs` for the layout contract.
pub(crate) use diagnostics::agent::*;
pub(crate) use diagnostics::api::*;
pub(crate) use diagnostics::app::*;
pub(crate) use diagnostics::auth::*;
pub(crate) use diagnostics::cache::*;
pub(crate) use diagnostics::canonical_kinds::*;
pub(crate) use diagnostics::capability::*;
pub(crate) use diagnostics::command::*;
pub(crate) use diagnostics::crypto::*;
pub(crate) use diagnostics::doctor_local::*;
pub(crate) use diagnostics::env::*;
pub(crate) use diagnostics::error::*;
pub(crate) use diagnostics::event::*;
pub(crate) use diagnostics::external::*;
pub(crate) use diagnostics::field::*;
pub(crate) use diagnostics::http_headers::*;
pub(crate) use diagnostics::lifecycle::*;
pub use diagnostics::lifecycle::{lifecycle_gate_completions, lifecycle_gate_hover};
pub(crate) use diagnostics::lifecycle_block::*;
pub use diagnostics::lifecycle_block::{lifecycle_block_completions, lifecycle_block_hover};
pub(crate) use diagnostics::lzx::*;
pub(crate) use diagnostics::notification::*;
pub(crate) use diagnostics::policy::*;
pub(crate) use diagnostics::profile::*;
pub(crate) use diagnostics::query::*;
pub(crate) use diagnostics::registry::*;
pub(crate) use diagnostics::route_guard::*;
pub use diagnostics::route_guard::{route_guard_completions, route_guard_hover};
pub(crate) use diagnostics::security::*;
pub(crate) use diagnostics::vocab::*;
pub(crate) use diagnostics::webhook::*;
pub(crate) use diagnostics::workspace::*;
pub(crate) use dispatch::{DiagnosticMode, diagnostics_for_with_profile_inner};
pub(crate) use format::canonical::*;
pub use hover::*;
pub(crate) use keywords::{DESIGN_KEYWORDS, KEYWORDS, design_keyword_description};
pub use rate_limit::rate_limit_env_completions;
// Wave R9-B extract — security-profile narrowing moved into
// `security_profile.rs`. Re-exported so `dispatch.rs` and the
// `diagnostics_for_source_with_profile` entry point keep their
// `crate::*` paths.
#[allow(unused_imports)]
pub(crate) use security_profile::{
    apply_security_profile, diagnostic_code, is_security_enforcement_code, is_security_opt_out_code,
};
// Wave R9-B extract — small crate-root source-walk diagnostics
// (`approval`, `event.trace`, `extensible_by` whitelist) moved into
// `source_diagnostics.rs`. Re-exported so `dispatch.rs` keeps its
// `crate::*` paths.
#[allow(unused_imports)]
pub(crate) use source_diagnostics::{
    AnchorWhitelistEntry, anchor_whitelist_diagnostics, approval_contract_diagnostics,
    reserved_trace_event_diagnostics,
};
pub use source_scan::*;
// Wave R7-3 extract — `tests` block producer + stack/anchor helpers
// moved into `test_blocks.rs`. Re-exported so the anchor-whitelist
// producer, the source-walk dispatch, and the LSP test suite keep their
// existing `crate::*` call paths.
pub(crate) use test_blocks::{
    extends_anchor, extensible_by_features, is_transition_line, is_valid_test_assertion,
    stack_kind, test_block_diagnostics, test_context, view_anchor,
};
// Wave R7-3 extract — text + position utilities moved into
// `text_utils.rs`. Re-exported so existing `crate::*` call paths inside
// `lib.rs`, `diagnostics/*`, `completion/*`, `hover.rs`, and the test
// suite keep compiling.
pub(crate) use text_utils::{
    byte_index_for_utf16_position, feature_name, first_line_range, full_document_range,
    is_design_lzi_uri, is_lzx_uri, is_trivia_line, is_word_byte, leading_spaces,
    line_prefix_at_position, position_at_line_start, position_for_offset, range_from_span,
    simple_canonical_diagnostic, simple_edit_action, word_at_position,
};
pub use types::SecurityProfile;

/// Hidden test surface — the **real** LSP-side severity bridge + the D3
/// ownership partition, re-exported so the anti-drift parity integration
/// test (`tests/doctor_severity_parity.rs`) can call the genuine
/// production functions rather than a reconstruction. Hardcoding a
/// severity on the LSP side (instead of routing through the shared
/// `lazuli_doctor_config` resolver) would then fail that test at build /
/// assert time.
///
/// `#[doc(hidden)]`: not part of the LSP's public API contract — it
/// exists only so the cross-consumer parity proof can mechanically
/// reference the exact code path the editor publishes. Production callers
/// reach these through the crate-internal paths unchanged.
#[doc(hidden)]
pub mod test_surface {
    pub use crate::diagnostics::doctor_local::{doctor_class_lsp_severity, lsp_severity};
    pub use crate::doctor_engine::{is_doctor_owned, is_lsp_owned};
    // H4 — the genuine semantic-token production functions the
    // `semantic_tokens_full` trait method calls. Re-exported so the
    // end-to-end parity test exercises the exact legend + delta-encoding
    // code path the editor publishes, not a reconstruction.
    pub use crate::semantic_tokens::{
        SEMANTIC_TOKEN_ORDER, encode_delta, legend, semantic_tokens_full,
    };
}

/// The stable server identifier surfaced to LSP clients during
/// initialization handshake. Hard-coded so the value can never drift
/// between releases — VS Code / Helix bind their config keys to this
/// exact string.
///
/// ## Examples
///
/// ```
/// assert_eq!(lazuli_lsp::server_name(), "lazuli-lsp");
/// ```
pub fn server_name() -> &'static str {
    "lazuli-lsp"
}

/// Run the full LSP diagnostic pass against a single source string at
/// the default [`SecurityProfile::Strict`] level.
///
/// This is the entry point external consumers (CLI doctor, scripts,
/// integration tests) hit when they want "what would the editor
/// underline?" without standing up a `tower-lsp` server. See
/// [`diagnostics_for_source_with_profile`] for the profile-aware
/// counterpart.
///
/// ## Examples
///
/// ```
/// let diags = lazuli_lsp::diagnostics_for_source("");
/// assert!(diags.is_empty(), "empty source has no diagnostics");
/// ```
pub fn diagnostics_for_source(source: &str) -> Vec<Diagnostic> {
    diagnostics_for_source_with_profile(source, SecurityProfile::Strict)
}

/// Public diagnostic entry point used by `lazuli_cli::doctor` and other
/// out-of-process consumers.
///
/// Intentionally **excludes** the file-local diagnostics wired in from
/// `lazuli_doctor` (R2.F): the CLI doctor invokes those checks itself
/// against the same lowered IR, and duplicating them here would double-fire
/// every catalog code into both the LSP-pulled stream and the CLI's own
/// dispatch. The LSP backend (`Backend::did_open` / `did_change`) calls
/// `diagnostics_for_uri_with_config` → `diagnostics_for_with_config`
/// which DOES include them so editor squiggles still surface live, with
/// severity resolved through the workspace-loaded `ResolvedDoctorConfig`.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::{SecurityProfile, diagnostics_for_source_with_profile};
///
/// let diags =
///     diagnostics_for_source_with_profile("", SecurityProfile::Prototype);
/// assert!(diags.is_empty());
/// ```
pub fn diagnostics_for_source_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    // CLI / out-of-process callers pass only a profile (no workspace
    // manifest). Build a profile-only resolved config so the shared
    // resolver path is identical to the LSP's.
    let config = ResolvedDoctorConfig::from_doctor(None, security_profile);
    diagnostics_for_with_profile_inner(source, &config, DiagnosticMode::Editor)
}

/// CLI/batch diagnostic entry point used by `lazuli check` (and any other
/// out-of-process consumer that wants the full *batch* surface, not the
/// editor's per-keystroke subset).
///
/// Identical to [`diagnostics_for_source_with_profile`] except it runs the
/// real parser + analyzer lower as a backstop over canonical `.lzi`
/// sources (BUG 2 Part B). The text-pattern producers the editor pass runs
/// do not cover every STRICT block — `job`/`poller`/`notification` and
/// other producer-less blocks would otherwise let a genuine syntax error
/// slip through with a 0 exit. This entry surfaces it; the editor's
/// synchronous Layer-1 deliberately does NOT (it gets parse/lower failures
/// from the debounced Layer-2 `run_package` stream instead, avoiding
/// per-keystroke double-fire / flicker).
///
/// ## Examples
///
/// ```no_run
/// use lazuli_lsp::{diagnostics_for_source_with_profile_cli, SecurityProfile};
///
/// let diagnostics =
///     diagnostics_for_source_with_profile_cli("feature billing\n", SecurityProfile::Strict);
/// let _ = diagnostics;
/// ```
pub fn diagnostics_for_source_with_profile_cli(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    let config = ResolvedDoctorConfig::from_doctor(None, security_profile);
    diagnostics_for_with_profile_inner(source, &config, DiagnosticMode::Cli)
}

/// Test-only convenience: resolve at the default `Strict` profile with no
/// workspace manifest. Production paths go through
/// [`diagnostics_for_uri_with_config`] with the backend-loaded config.
#[cfg(test)]
pub(crate) fn diagnostics_for_uri(uri: &Url, source: &str) -> Vec<Diagnostic> {
    diagnostics_for_uri_with_config(uri, source, &ResolvedDoctorConfig::default())
}

/// In-LSP entry point that resolves doctor-class diagnostic severity
/// through a workspace-loaded [`ResolvedDoctorConfig`] (W2). The backend
/// builds + caches the config from the workspace `Lazurite.toml`
/// (`[doctor] profile` + presets / overrides) and hands it in here so
/// editor severities are mode-aware and match `lazuli doctor`.
pub(crate) fn diagnostics_for_uri_with_config(
    uri: &Url,
    source: &str,
    config: &ResolvedDoctorConfig,
) -> Vec<Diagnostic> {
    let mut diagnostics = diagnostics_for_with_config(source, config);

    if is_lzx_source(source) {
        diagnostics.extend(lzx_filename_diagnostics(uri, source));
    }

    diagnostics
}

/// Test-only convenience: full in-LSP pass at the default `Strict`
/// profile with no workspace manifest.
#[cfg(test)]
pub(crate) fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    diagnostics_for_with_config(source, &ResolvedDoctorConfig::default())
}

/// Internal in-LSP entry point — always includes the doctor file-local
/// diagnostics wired in R2.F so editor squiggles fire live. Severity is
/// resolved through the supplied [`ResolvedDoctorConfig`].
pub(crate) fn diagnostics_for_with_config(
    source: &str,
    config: &ResolvedDoctorConfig,
) -> Vec<Diagnostic> {
    diagnostics_for_with_profile_inner(source, config, DiagnosticMode::Editor)
}

/// Internal in-LSP entry point keyed on a bare profile (no manifest
/// presets / overrides). Retained for tests + callers that only vary the
/// profile; builds a profile-only resolved config.
#[cfg(test)]
pub(crate) fn diagnostics_for_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    let config = ResolvedDoctorConfig::from_doctor(None, security_profile);
    diagnostics_for_with_config(source, &config)
}

pub(crate) fn is_canonical_source(source: &str) -> bool {
    if has_lzx_top_level_contract(source) {
        return false;
    }

    source.lines().any(|line| {
        leading_spaces(line) == 0
            && (line.trim_start().starts_with("feature ") || line.trim_start() == "env")
    }) || has_canonical_app_block(source)
        || has_canonical_registry_block(source)
        || has_canonical_profile_block(source)
        || has_canonical_workspace_block(source)
        || has_canonical_contract_block(source)
        || has_canonical_design_block(source)
}

pub(crate) fn has_canonical_app_block(source: &str) -> bool {
    let lines: Vec<_> = source.lines().collect();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 0 || !trimmed.starts_with("app ") {
            continue;
        }

        for next in lines.iter().skip(index + 1) {
            let next_trimmed = next.trim_start();
            if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                continue;
            }
            return leading_spaces(next) > 0;
        }
    }

    false
}

pub(crate) fn has_canonical_registry_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start() == "registry")
}

pub(crate) fn has_canonical_profile_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("profile "))
}

pub(crate) fn has_canonical_workspace_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("workspace "))
}

pub(crate) fn has_canonical_contract_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("contract "))
}

/// `design.lzi` is a separate sub-grammar (parsed by
/// `parse_design_document`); marking it canonical short-circuits the
/// generic feature parser path.
pub(crate) fn has_canonical_design_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("design "))
}

pub(crate) fn is_lzx_source(source: &str) -> bool {
    has_lzx_top_level_contract(source)
}

pub(crate) fn has_lzx_top_level_contract(source: &str) -> bool {
    source.lines().any(|line| {
        leading_spaces(line) == 0
            && matches!(
                line.split_whitespace().next(),
                Some("route" | "experience" | "surface")
            )
    })
}

pub(crate) fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(crate) fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// `lower_snake` identifier: ASCII letters / digits / underscores, must
/// not start with a digit, must be non-empty.
pub(crate) fn is_lower_ident(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
#[path = "lib_tests/mod.rs"]
mod tests;
