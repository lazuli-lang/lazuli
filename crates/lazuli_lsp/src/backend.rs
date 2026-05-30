//! `Backend` — the tower-lsp `LanguageServer` implementation.
//!
//! Owns the live document store (`documents: RwLock<HashMap<Url,
//! String>>`) and routes every LSP request (`hover`, `completion`,
//! `document_symbol`, `formatting`, `code_action`, plus the
//! `did_open` / `did_change` / `did_close` lifecycle) to the matching
//! sub-module under `lazuli_lsp::*` (handlers / completion / format /
//! diagnostics).
//!
//! Wave R7-3 extract: lifted out of `lib.rs`. Public surface
//! preserved verbatim — `serve_stdio` and the `Backend` struct itself
//! stay reachable via the crate root re-exports in `lib.rs`.
//!
//! Boundary: this module never owns diagnostic generation, just
//! the LSP-protocol plumbing. Any rule change lives under
//! `diagnostics/*`, not here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use lazuli_doctor_config::ResolvedDoctorConfig;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, MarkupContent, MarkupKind, MessageType, OneOf, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WorkDoneProgressOptions,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

use crate::completion::cap_file::cap_file_value_completions;
use crate::completion::context::context_aware_completions;
use crate::completion::input_field::input_field_completions;
use crate::completion::owner_axis::owner_axis_through_completions;
use crate::completion_items::{completion_items_for_uri, merge_completion_items};
use crate::diagnostics::lifecycle::lifecycle_gate_completions;
use crate::diagnostics::lifecycle_block::lifecycle_block_completions;
use crate::diagnostics::route_guard::route_guard_completions;
use crate::format::canonical::format_canonical_source;
use crate::{
    diagnostics_for_uri_with_config, full_document_range, handlers, is_design_lzi_uri, is_lzx_uri,
    lzx_completion, server_name,
};

include!("backend_p1.rs");
include!("backend_p2.rs");
