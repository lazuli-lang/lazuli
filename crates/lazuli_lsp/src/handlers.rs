//! Pure functions backing the `Backend` LSP method handlers. The
//! `Backend` impl in `lib.rs` stays a thin dispatch table; the actual
//! work each method does lives here.
//!
//! Currently extracted:
//!
//! * `document_symbols_for_source` — builds the nested `DocumentSymbol`
//!   tree (`feature -> resource/command/query/aggregate -> field`) the
//!   editor's outline pane consumes.
//! * `hover_markdown_for_position` — computes the hover string for a
//!   cursor position, walking the IR-vocab → route-guard → error-vocab
//!   → convention-bundle → keyword fall-through stack.
//! * `code_actions_for_position` — flattens the four families
//!   (`error_vocab`, `auth_refresh`, `route_guard`, `lifecycle_gate`)
//!   into a single response and wires the default `quickfix` kind.
//!
//! Pending extraction:
//!
//! * `initialize`, `completion`, `publish_diagnostics` — currently
//!   still inlined in `impl LanguageServer for Backend` in `lib.rs`.
//!
//! ## See also
//! * `lib.rs::Backend::document_symbol` — call site.
//! * `crate::completion_items::make_symbol` — the per-node envelope
//!   builder this module composes.
//! * `crate::range_from_span` — span → LSP `Range` converter.

use lazuli_syntax::parse_feature_skeletons;
use tower_lsp::lsp_types::{
    CodeActionKind, CodeActionOrCommand, CodeActionResponse, DocumentSymbol, Position, SymbolKind,
    Url,
};

use crate::{
    auth_refresh_code_actions, convention_bundle_hover, design_keyword_description,
    error_vocab_code_actions, error_vocab_code_resolved_hover, is_design_lzi_uri,
    keyword_description, lifecycle_gate_code_actions, lifecycle_gate_hover, make_symbol,
    range_from_span, rich_keyword_hover, route_guard_code_actions, route_guard_hover,
    word_at_position,
};

/// Compute the hover markdown for `position` against `source`. Returns
/// `None` when no provider fires. The provider order is intentional:
/// lifecycle-gate / route-guard / error-vocab / convention-bundle each
/// own a narrow context; only after all of them miss do we fall back to
/// `rich_keyword_hover` (the prose-heavy generic table) and finally to
/// the one-liner `keyword_description`. Design-token `.design.lzi`
/// sources branch off to a separate fall-through.
pub(crate) fn hover_markdown_for_position(
    source: &str,
    uri: &Url,
    position: Position,
) -> Option<String> {
    let word = word_at_position(source, position);
    if !is_design_lzi_uri(uri) {
        if let Some(markdown) = lifecycle_gate_hover(source, position, word.as_deref()) {
            return Some(markdown);
        }
        if let Some(markdown) = word
            .as_deref()
            .and_then(|word| route_guard_hover(source, position, word))
        {
            return Some(markdown);
        }
        if let Some(markdown) = word
            .as_deref()
            .and_then(|word| error_vocab_code_resolved_hover(source, position, word))
        {
            return Some(markdown);
        }
        if let Some(markdown) = word
            .as_deref()
            .and_then(|word| convention_bundle_hover(source, position, word))
        {
            return Some(markdown);
        }
        if let Some(markdown) = word.as_deref().and_then(rich_keyword_hover) {
            return Some(markdown);
        }
        return word
            .as_deref()
            .and_then(|word| keyword_description(word).map(|d| format!("`{word}`\n\n{d}")));
    }
    word.as_deref().and_then(|word| {
        design_keyword_description(word)
            .or_else(|| keyword_description(word))
            .map(|d| format!("`{word}`\n\n{d}"))
    })
}

/// Collect code actions for `position` from every family, default the
/// `kind` to `QUICKFIX` for any action that didn't set one, and return
/// `None` when no action fires. The default-kind step is required
/// because some editor clients filter the lightbulb list by `kind` even
/// though we advertised `CodeActionProviderCapability::Simple(true)`.
pub(crate) fn code_actions_for_position(
    source: &str,
    uri: &Url,
    position: Position,
) -> Option<CodeActionResponse> {
    let mut actions = error_vocab_code_actions(source, uri, position);
    actions.extend(auth_refresh_code_actions(source, uri, position));
    actions.extend(route_guard_code_actions(source, uri, position));
    actions.extend(lifecycle_gate_code_actions(source, uri, position));
    if actions.is_empty() {
        return None;
    }
    for action in &mut actions {
        if let CodeActionOrCommand::CodeAction(ca) = action {
            if ca.kind.is_none() {
                ca.kind = Some(CodeActionKind::QUICKFIX);
            }
        }
    }
    Some(actions)
}

/// Build the nested `DocumentSymbol` tree the editor's outline pane
/// consumes. Returns `None` when `parse_feature_skeletons` fails — the
/// caller should treat that as "no symbols", same as the empty Vec case.
pub(crate) fn document_symbols_for_source(source: &str) -> Option<Vec<DocumentSymbol>> {
    let features = parse_feature_skeletons(source).ok()?;
    let symbols = features
        .iter()
        .map(|feature| {
            let resource_symbols = feature.resources.iter().map(|resource| {
                make_symbol(
                    resource.name.clone(),
                    Some("resource".to_owned()),
                    SymbolKind::STRUCT,
                    range_from_span(source, resource.span),
                    Some(
                        resource
                            .fields
                            .iter()
                            .map(|field| {
                                make_symbol(
                                    field.name.clone(),
                                    Some(field.type_text.clone()),
                                    SymbolKind::PROPERTY,
                                    range_from_span(source, field.span),
                                    None,
                                )
                            })
                            .collect(),
                    ),
                )
            });
            let command_symbols = feature.commands.iter().map(|command| {
                make_symbol(
                    command.name.clone(),
                    Some("command".to_owned()),
                    SymbolKind::METHOD,
                    range_from_span(source, command.span),
                    None,
                )
            });
            let query_symbols = feature.queries.iter().map(|query| {
                let (name, span) = match query {
                    lazuli_syntax::QueryDecl::List(query) => (&query.name, query.span),
                    lazuli_syntax::QueryDecl::Lookup(query) => (&query.name, query.span),
                    lazuli_syntax::QueryDecl::Sql(query) => (&query.name, query.span),
                };
                make_symbol(
                    name.clone(),
                    Some("query".to_owned()),
                    SymbolKind::FUNCTION,
                    range_from_span(source, span),
                    None,
                )
            });
            let aggregate_symbols = feature.aggregates.iter().map(|aggregate| {
                make_symbol(
                    aggregate.name.clone(),
                    Some("aggregate".to_owned()),
                    SymbolKind::STRUCT,
                    range_from_span(source, aggregate.span),
                    None,
                )
            });

            make_symbol(
                feature.name.clone(),
                Some("feature".to_owned()),
                SymbolKind::MODULE,
                range_from_span(source, feature.span),
                Some(
                    resource_symbols
                        .chain(command_symbols)
                        .chain(query_symbols)
                        .chain(aggregate_symbols)
                        .collect(),
                ),
            )
        })
        .collect();
    Some(symbols)
}
