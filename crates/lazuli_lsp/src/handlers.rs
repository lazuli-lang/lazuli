//! Pure functions backing the `Backend` LSP method handlers. The
//! `Backend` impl in `lib.rs` stays a thin dispatch table; the actual
//! work each method does lives here.
//!
//! Currently extracted:
//!
//! * `document_symbols_for_source` — builds the nested `DocumentSymbol`
//!   tree (`feature -> resource/command/query/aggregate -> field`) the
//!   editor's outline pane consumes.
//!
//! Pending extraction:
//!
//! * `initialize`, `hover`, `completion`, `code_action`,
//!   `publish_diagnostics` — currently still inlined in
//!   `impl LanguageServer for Backend` in `lib.rs`.
//!
//! ## See also
//! * `lib.rs::Backend::document_symbol` — call site.
//! * `crate::completion_items::make_symbol` — the per-node envelope
//!   builder this module composes.
//! * `crate::range_from_span` — span → LSP `Range` converter.

use lazuli_syntax::parse_feature_skeletons;
use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

use crate::{make_symbol, range_from_span};

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
