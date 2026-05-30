//! Completion provider for the IR Error-Page family
//! (`error_page <status>` and `audience <name>` inside an
//! `error_page` block).
//!
//! Surfaces the two closed catalogs from `catalogs.rs`:
//!
//! - `crate::ERROR_PAGE_STATUS_VALUES` (3xx/4xx/5xx codes) for
//!   `error_page <partial-status>`.
//! - `crate::ERROR_PAGE_AUDIENCE_VALUES` (common app audiences) for
//!   `audience <partial>` inside an `error_page` block.
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::block_kind_at`] — detect whether the cursor sits inside
//!   an `error_page` block before offering the audience catalog.
//! - [`crate::error_page_status_detail`] — per-status detail strings
//!   (HTTP semantics) from `catalogs.rs`.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::{
    ERROR_PAGE_AUDIENCE_VALUES, ERROR_PAGE_STATUS_VALUES, block_kind_at, error_page_status_detail,
};

pub(crate) fn error_page_value_completions(
    source: &str,
    position: Position,
    before_cursor: &str,
) -> Option<Vec<CompletionItem>> {
    let trimmed = before_cursor.trim_start();
    if let Some(rest) = trimmed.strip_prefix("error_page ")
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return Some(
            ERROR_PAGE_STATUS_VALUES
                .iter()
                .map(|value| CompletionItem {
                    label: (*value).to_owned(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: error_page_status_detail(value).map(str::to_owned),
                    ..CompletionItem::default()
                })
                .collect(),
        );
    }

    if block_kind_at(source, position) == Some("error_page")
        && let Some(rest) = trimmed.strip_prefix("audience ")
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(
            ERROR_PAGE_AUDIENCE_VALUES
                .iter()
                .map(|value| CompletionItem {
                    label: (*value).to_owned(),
                    kind: Some(CompletionItemKind::VALUE),
                    detail: Some("Common app route audience.".to_owned()),
                    ..CompletionItem::default()
                })
                .collect(),
        );
    }

    None
}
