//! Completion / resolved-hover provider for the IR Error-Vocab family
//! (`feature.errors`, `policy when_denied @translation.<key>`,
//! `expose client 4xx/5xx`, `default hide/expose`).
//!
//! Mirrors `diagnostics/error.rs` (validation backend) and
//! `code_actions/error_vocab.rs` (refactor backend). This module owns
//! the **completion + resolved-hover** backend.
//!
//! Public entry points re-exported as `lazuli_lsp::*` through `lib.rs`:
//!
//! - [`error_vocab_completions`] — 6 trigger-position completion
//!   provider (proposal §7.1).
//! - [`error_vocab_resolved_text`] — resolution-chain text lookup
//!   (proposal §7.2).
//! - [`error_vocab_code_resolved_hover`] — markdown hover that wraps
//!   the resolved text with a source-of-resolution label
//!   (proposal §7.2 / §3.6).
//!
//! Two crate-private indent-walking lookups support the public
//! entry points:
//!
//! - [`lookup_feature_error_key`] — walks `feature.errors.<code>
//!   message @translation.<key>` and returns the key.
//! - [`lookup_translation_first_variant`] — walks the same feature's
//!   `translation` block to resolve the first locale variant's text.
//!
//! Plus the indent-walk gate [`in_feature_errors_block`] used by both
//! the completion provider here and `code_actions::error_vocab`.
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::leading_spaces`] — indent-aware block scanning.
//! - [`crate::enclosing_feature_name`] /
//!   [`crate::collect_translation_keys_for_feature`] — feature-scope
//!   resolution and `@translation.<key>` enumeration.
//! - [`crate::ERROR_VOCAB_CODES`] / `ERROR_VOCAB_DEFAULT_VALUES` /
//!   `ERROR_VOCAB_EXPOSE_4XX_FIELDS` / `ERROR_VOCAB_EXPOSE_5XX_FIELDS`
//!   — closed catalogs from `catalogs.rs`.
//! - [`crate::error_vocab_code_builtin_en_us`] /
//!   [`crate::error_vocab_code_detail`] — runtime fallback + detail
//!   strings from `catalogs.rs`.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::{
    ERROR_VOCAB_CODES, ERROR_VOCAB_DEFAULT_VALUES, ERROR_VOCAB_EXPOSE_4XX_FIELDS,
    ERROR_VOCAB_EXPOSE_5XX_FIELDS, collect_translation_keys_for_feature, enclosing_feature_name,
    error_vocab_code_builtin_en_us, error_vocab_code_detail, leading_spaces,
};

include!("error_vocab_p1.rs");
include!("error_vocab_p2.rs");
