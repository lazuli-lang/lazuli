//! The `app <Name>` operational-contract dispatcher.
//!
//! Walks the source once, accumulates [`AppOperationalFacts`] per open
//! `app` block, and flushes whole-app invariants via
//! [`app_operational_block_diagnostics`] when the block ends or a new
//! one starts. Per-line shape checks are delegated to the sibling
//! sub-modules (`child`, `target`, `env`, `integration`, `capability`,
//! `architecture`, `service`, `runtime`, `deploy`) via the
//! crate-private re-exports in `app/mod.rs`.

use lazuli_keywords::manifest_child_keys;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::{
    AppIntegrationFacts, AppOperationalFacts, AppRuntimeUnitFacts, AppServiceFacts,
    app_child_block, app_operational_block_diagnostics, is_app_scalar_child, parse_env_group_name,
    validate_app_architecture_line, validate_app_binding_line, validate_app_capability_line,
    validate_app_child_header, validate_app_communication_line, validate_app_deploy_line,
    validate_app_env_line, validate_app_integration_child,
    validate_app_integration_credential_line, validate_app_integration_header,
    validate_app_pack_use_line, validate_app_runtime_unit_child, validate_app_scalar_child,
    validate_app_service_child, validate_app_service_exposure_line, validate_app_target_line,
    validate_app_url_line,
};
use crate::diagnostics::canonical_kinds::closest_kind;
use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

/// App-manifest block headers whose indent-4 (and indent-6 body) child
/// keys the walker validates against the closed catalog
/// [`manifest_child_keys`] returns for them. This is the **single source**
/// the indent-4 / indent-6 match arms below build FROM: every block here is
/// dispatched into [`validate_app_block_child`]; the anti-drift gate
/// `every_manifest_block_has_child_key_validation` asserts (1) every block
/// here has ≥1 registry child row (so it never silently no-ops), and (2)
/// every registry context that maps to a block name via
/// `manifest_block_name` is listed here (so the registry and the walker can
/// never diverge).
///
/// Before BUG-1, each of these arms was an EMPTY skip (`Some("locale") =>
/// {}`), so a misspelled / unknown child key (`fallbacks` instead of
/// `fallback`) was silently dropped by the parser with no diagnostic at
/// `lazuli check` or `lazuli doctor` time. Routing them through the shared
/// helper turns an unknown child into an `app-block-child-contract` ERROR.
pub(crate) const VALIDATED_APP_BLOCKS: &[&str] = &[
    "locale",
    "cors",
    "headers",
    "cookie",
    "proxy",
    "limits",
    "logging",
    "tracing",
    "route_guard",
    "encryption",
    "error_page",
];

/// Validate one indent-4 / indent-6 child line under an app-manifest
/// `block` against the closed child-key catalog the registry carries for
/// that block ([`manifest_child_keys`]).
///
/// Skips lines that are not a plain `<head-key> ...` shape so the contract
/// never false-fires on the legitimate non-keyword body forms the parser
/// accepts:
///   * `@`-prefixed lines (`key @key.tenant` inside `encryption`);
///   * lines containing `:` (`pt-BR: en-US` fallback bodies);
///   * lines containing `=` (`x = y` style bindings).
/// This mirrors the head-token guard in
/// `app_unknown_kind_diagnostics` (`canonical_kinds/sections/blocks.rs`).
///
/// When the head token is not in the (non-empty) catalog it pushes an
/// `app-block-child-contract` ERROR, suggesting the closest catalog key
/// (`closest_kind(head, &catalog, 2)`) when one is within edit distance 2,
/// else listing the whole catalog. A block whose catalog is empty is a
/// no-op (the caller should never route such a block here — the gate
/// enforces that), so an un-cataloged block can never false-fire.
fn validate_app_block_child(
    diagnostics: &mut Vec<Diagnostic>,
    block: &str,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    // Non-keyword body shapes the parser accepts verbatim — never a
    // misspelled child key, so they must not fire the contract.
    if trimmed.starts_with('@') || trimmed.contains(':') || trimmed.contains('=') {
        return;
    }
    // Every caller passes a block from `VALIDATED_APP_BLOCKS` — the const
    // is the single source the dispatch match arms mirror, and the
    // anti-drift gate `every_manifest_block_has_child_key_validation`
    // asserts the two stay in lockstep with the registry.
    debug_assert!(
        VALIDATED_APP_BLOCKS.contains(&block),
        "validate_app_block_child called with unlisted block `{block}` — add it to VALIDATED_APP_BLOCKS"
    );
    let Some(head) = trimmed.split_whitespace().next() else {
        return;
    };

    let catalog: Vec<&'static str> = manifest_child_keys(block).collect();
    if catalog.is_empty() {
        // No closed catalog to validate against → cannot know what is
        // valid, so stay silent (and the gate forbids routing such a
        // block here in the first place).
        return;
    }
    if catalog.contains(&head) {
        return;
    }

    let message = match closest_kind(head, &catalog, 2) {
        Some(suggested) => {
            format!("unknown `{block}` child key `{head}`. Did you mean `{suggested}`?")
        }
        None => {
            let mut valid = catalog.clone();
            valid.sort_unstable();
            format!(
                "unknown `{block}` child key `{head}`. Valid: {}.",
                valid.join(" / ")
            )
        }
    };
    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::ERROR,
        "app-block-child-contract",
        &message,
    ));
}

include!("operational_p1.rs");
