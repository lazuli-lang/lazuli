//! Inline test suite for the LSP crate.
//!
//! Hosted out-of-line via `#[path = "lib_tests/mod.rs"] mod tests;` in
//! `lib.rs`. The body of the original `mod tests { ... }` block is
//! split into sibling `group_NN_<first_fn>.rs` files (≤ 500 LOC each)
//! so editors and `clippy` stay responsive on the largest test crate
//! in the workspace. Every `crate::<fn>` / `super::<fn>` path that
//! resolved against the old single-file `mod tests` continues to
//! resolve here because all shared imports + helpers live in this
//! module file and are re-exported through `pub(super) use` for
//! children to consume via `use super::*`.

#![allow(unused_imports)]

pub(super) use super::{
    DESIGN_KEYWORDS, EFFECT_VERBS, KEYWORDS, KIND_CHILD_COMPLETIONS,
    NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES, RATE_LIMIT_AXES, SecurityProfile, block_kind_at,
    cap_file_value_completions, completion_items_for_uri, context_aware_completions,
    convention_bundle_hover, conventions_list_completions, design_keyword_description,
    diagnostics_for, diagnostics_for_uri, diagnostics_for_with_profile, format_canonical_source,
    keyword_description, notification_digest_template_strategy_detail,
    owner_axis_through_completions, position_for_offset, rate_limit_env_completions,
    rich_keyword_hover,
};
pub(super) use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, Position, Url,
};

/// Per-LSP-test helper: strip `lazuli-doctor` diagnostics so legacy
/// tests that assert exact LSP-shape diagnostic counts keep passing
/// after the R2.F doctor wire-up. Doctor wiring has its own tests
/// further down — see `doctor_*` test cases.
pub(super) fn diagnostics_for_lsp_only(source: &str) -> Vec<Diagnostic> {
    diagnostics_for(source)
        .into_iter()
        .filter(|d| d.source.as_deref() != Some("lazuli-doctor"))
        .collect()
}

pub(super) fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|d| match d.code.as_ref()? {
            tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.clone()),
            tower_lsp::lsp_types::NumberOrString::Number(n) => Some(n.to_string()),
        })
        .collect()
}

/// Helper: drive `context_aware_completions` and unwrap the returned
/// items. Panics with a helpful message when the completion context
/// isn't recognised so test failures point at the unrecognised path
/// immediately.
pub(super) fn completions_at(source: &str, line: u32, character: u32) -> Vec<CompletionItem> {
    context_aware_completions(source, Position { line, character })
        .unwrap_or_else(|| panic!("expected context-aware completion at line {line}:{character}"))
}

pub(super) fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

pub(super) fn diagnostics_with_code<'a>(
    diagnostics: &'a [Diagnostic],
    target: &str,
) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code.as_ref().and_then(|c| match c {
                tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                _ => None,
            }) == Some(target)
        })
        .collect()
}

pub(super) fn doctor_diagnostics_with_code<'a>(
    diagnostics: &'a [Diagnostic],
    code: &str,
) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(tower_lsp::lsp_types::NumberOrString::String(c)) if c == code
            )
        })
        .collect()
}

/// Helper: assert the rich Markdown hover for `keyword` exists and
/// contains every snippet in `expected_fragments`. Fragments double
/// as a smoke test that required-children / optional-children /
/// example / doc anchor all land in the output.
pub(super) fn assert_rich_hover_contains(keyword: &str, expected_fragments: &[&str]) {
    let rendered = rich_keyword_hover(keyword)
        .unwrap_or_else(|| panic!("rich hover for `{keyword}` must be present"));
    for fragment in expected_fragments {
        assert!(
            rendered.contains(fragment),
            "rich hover for `{keyword}` must contain `{fragment}`; got:\n{rendered}"
        );
    }
}

// Sub-modules — line-range buckets, ≤ 500 LOC each.
mod group_00_diagnostics_for_lsp_only;
mod group_01_query_statement_unknown_flags_ty;
mod group_02_canonical_warns_for_invalid_work;
mod group_03_canonical_events_payload_warns_f;
mod group_04_canonical_warns_for_incomplete_c;
mod group_05_agent_rejects_non_llm_model_refe;
mod group_06_approval_well_formed_emits_nothi;
mod group_07_canonical_warns_when_trace_event;
mod group_08_canonical_warns_for_legacy_ergon;
mod group_09_canonical_order_reports_late_use;
mod group_10_rich_keyword_hover_describes_con;
mod group_11_rich_hover_for_query_lookup_docu;
mod group_12_doctor_vocab_audit_001_surfaces_;
