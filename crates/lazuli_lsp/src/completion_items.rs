//! Global keyword + closed-catalog completion item builders surfaced
//! through `Backend::completion`.
//!
//! Three functions:
//!
//! * `completion_items_for_uri` — branches on `is_design_lzi_uri` and
//!   returns either the canonical Lazuli keyword list or the
//!   design-only catalog.
//! * `lazuli_keyword_completion_items` — global completion list for
//!   `.lzi` sources. Wraps the `KEYWORDS` catalog plus the five
//!   closed-value catalogs (`AUTH_CATALOG_VALUES`,
//!   `DEPLOY_STRATEGY_VALUES`,
//!   `NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES`,
//!   `ERROR_PAGE_STATUS_VALUES`, `RESOURCE_LOCK_STRATEGY_VALUES`)
//!   so authors hit any reserved word inline.
//! * `design_keyword_completion_items` — same shape, driven by
//!   `DESIGN_KEYWORDS`. Includes a richer `Markdown` documentation
//!   payload because design tokens authors lean harder on hover.
//! * `make_symbol` / `merge_completion_items` — supporting utilities.
//!
//! ## See also
//! * `crate::keywords::KEYWORDS`, `crate::keywords::DESIGN_KEYWORDS` —
//!   source-of-truth catalogs.
//! * `crate::catalogs` — closed-value catalogs piped into the same
//!   completion list.
//! * `crate::hover::keyword_description` — hover detail used to
//!   populate `CompletionItem::detail`.

use std::collections::HashSet;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, DocumentSymbol, MarkupContent, MarkupKind,
    Range, SymbolKind, Url,
};

use crate::{
    AUTH_CATALOG_VALUES, DEPLOY_STRATEGY_VALUES, DESIGN_KEYWORDS, ERROR_PAGE_STATUS_VALUES,
    KEYWORDS, NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES, RESOURCE_LOCK_STRATEGY_VALUES,
    auth_catalog_detail, deploy_strategy_detail, design_keyword_description,
    error_page_status_detail, is_design_lzi_uri, keyword_description,
    notification_digest_template_strategy_detail, resource_lock_strategy_detail,
};

pub(crate) fn completion_items_for_uri(uri: &Url) -> Vec<CompletionItem> {
    if is_design_lzi_uri(uri) {
        return design_keyword_completion_items();
    }

    lazuli_keyword_completion_items()
}

pub(crate) fn merge_completion_items(
    primary: Option<Vec<CompletionItem>>,
    secondary: Option<Vec<CompletionItem>>,
) -> Vec<CompletionItem> {
    let mut items = primary.unwrap_or_default();
    let mut labels: HashSet<String> = items.iter().map(|item| item.label.clone()).collect();
    for item in secondary.unwrap_or_default() {
        if labels.insert(item.label.clone()) {
            items.push(item);
        }
    }
    items
}

pub(crate) fn lazuli_keyword_completion_items() -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = KEYWORDS
        .iter()
        .map(|keyword| CompletionItem {
            label: (*keyword).to_owned(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: keyword_description(keyword).map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect();
    items.extend(AUTH_CATALOG_VALUES.iter().map(|value| CompletionItem {
        label: (*value).to_owned(),
        kind: Some(CompletionItemKind::VALUE),
        detail: auth_catalog_detail(value).map(str::to_owned),
        ..CompletionItem::default()
    }));
    // Migrations bucket cycle Route C — closed `deploy.strategy`
    // catalog. Hovers/completions surface the three rollout patterns.
    items.extend(DEPLOY_STRATEGY_VALUES.iter().map(|value| CompletionItem {
        label: (*value).to_owned(),
        kind: Some(CompletionItemKind::VALUE),
        detail: deploy_strategy_detail(value).map(str::to_owned),
        ..CompletionItem::default()
    }));
    // Notifications expanded bucket cycle — closed
    // `notification.digest.template_strategy` catalog. Two
    // strategies; LSP completion narrows authoring before doctor
    // surfaces an unknown value.
    items.extend(
        NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES
            .iter()
            .map(|value| CompletionItem {
                label: (*value).to_owned(),
                kind: Some(CompletionItemKind::VALUE),
                detail: notification_digest_template_strategy_detail(value).map(str::to_owned),
                ..CompletionItem::default()
            }),
    );
    items.extend(ERROR_PAGE_STATUS_VALUES.iter().map(|value| CompletionItem {
        label: (*value).to_owned(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: error_page_status_detail(value).map(str::to_owned),
        ..CompletionItem::default()
    }));
    // Roadmap §1.5 (CL.C.2) — closed `lock` strategy catalog.
    items.extend(
        RESOURCE_LOCK_STRATEGY_VALUES
            .iter()
            .map(|value| CompletionItem {
                label: (*value).to_owned(),
                kind: Some(CompletionItemKind::VALUE),
                detail: resource_lock_strategy_detail(value).map(str::to_owned),
                ..CompletionItem::default()
            }),
    );
    items
}

pub(crate) fn design_keyword_completion_items() -> Vec<CompletionItem> {
    DESIGN_KEYWORDS
        .iter()
        .map(|keyword| CompletionItem {
            label: (*keyword).to_owned(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: design_keyword_description(keyword).map(str::to_owned),
            documentation: design_keyword_description(keyword).map(|description| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "`{keyword}`\n\n{description}\n\nSee [design tokens](docs/proposals/design-tokens.md)."
                    ),
                })
            }),
            ..CompletionItem::default()
        })
        .collect()
}

#[allow(deprecated)]
pub(crate) fn make_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children,
    }
}
