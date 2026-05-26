use std::collections::{HashMap, HashSet};

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
mod format;
mod handlers;
mod hover;
mod keywords;
mod lzx_completion;
mod rate_limit;
mod security_profile;
mod source_diagnostics;
mod source_scan;
mod test_blocks;
mod text_utils;
mod types;

pub use backend::serve_stdio;

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

// Wave R7-3 extract — `tests` block producer + stack/anchor helpers
// moved into `test_blocks.rs`. Re-exported so the anchor-whitelist
// producer, the source-walk dispatch, and the LSP test suite keep their
// existing `crate::*` call paths.
pub(crate) use test_blocks::{
    extends_anchor, extensible_by_features, is_transition_line, is_valid_test_assertion,
    stack_kind, test_block_diagnostics, test_context, view_anchor,
};

pub(crate) use dispatch::diagnostics_for_with_profile_inner;

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

// Wave R9-B extract — small crate-root source-walk diagnostics
// (`approval`, `event.trace`, `extensible_by` whitelist) moved into
// `source_diagnostics.rs`. Re-exported so `dispatch.rs` keeps its
// `crate::*` paths.
#[allow(unused_imports)]
pub(crate) use source_diagnostics::{
    anchor_whitelist_diagnostics, approval_contract_diagnostics,
    reserved_trace_event_diagnostics, AnchorWhitelistEntry,
};

// Wave R9-B extract — security-profile narrowing moved into
// `security_profile.rs`. Re-exported so `dispatch.rs` and the
// `diagnostics_for_source_with_profile` entry point keep their
// `crate::*` paths.
#[allow(unused_imports)]
pub(crate) use security_profile::{
    apply_security_profile, diagnostic_code, is_security_enforcement_code,
    is_security_opt_out_code,
};

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
pub(crate) use completion_items::{completion_items_for_uri, make_symbol, merge_completion_items};
pub use conventions::conventions_list_completions;
pub(crate) use format::canonical::*;
pub use hover::*;
pub(crate) use keywords::{DESIGN_KEYWORDS, KEYWORDS, design_keyword_description};
pub use rate_limit::rate_limit_env_completions;
pub use source_scan::*;
pub use types::SecurityProfile;

pub fn server_name() -> &'static str {
    "lazuli-lsp"
}

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
/// `diagnostics_for_uri` → `diagnostics_for` which DOES include them so
/// editor squiggles still surface live.
pub fn diagnostics_for_source_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    diagnostics_for_with_profile_inner(source, security_profile, false)
}

pub(crate) fn diagnostics_for_uri(uri: &Url, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = diagnostics_for(source);

    if is_lzx_source(source) {
        diagnostics.extend(lzx_filename_diagnostics(uri, source));
    }

    diagnostics
}

pub(crate) fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    diagnostics_for_with_profile(source, SecurityProfile::Strict)
}

/// Internal in-LSP entry point — always includes the doctor file-local
/// diagnostics wired in R2.F so editor squiggles fire live.
pub(crate) fn diagnostics_for_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    diagnostics_for_with_profile_inner(source, security_profile, true)
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
                line.trim_start().split_whitespace().next(),
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
#[path = "lib_tests.rs"]
mod tests;
