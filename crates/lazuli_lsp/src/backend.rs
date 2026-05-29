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

use lazuli_doctor_config::ResolvedDoctorConfig;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, MarkupContent, MarkupKind, MessageType, OneOf, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
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

pub(crate) struct Backend {
    pub(crate) client: Client,
    pub(crate) documents: Arc<RwLock<HashMap<Url, String>>>,
    /// Workspace root captured from the `initialize` handshake
    /// (`root_uri` / first workspace folder). Used as the starting point
    /// for `Lazurite.toml` discovery when a document's own path can't be
    /// resolved.
    pub(crate) workspace_root: Arc<RwLock<Option<PathBuf>>>,
}

/// Walk up from `start` looking for the workspace `Lazurite.toml`,
/// returning its path. Bounded by the filesystem root.
fn find_lazurite_manifest(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("Lazurite.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Build a [`ResolvedDoctorConfig`] for the workspace that owns `doc_uri`.
///
/// W2 — this is where the LSP loads the workspace doctor config:
/// 1. locate `Lazurite.toml` (from the document's own directory, falling
///    back to the `initialize` workspace root);
/// 2. read `[doctor] profile` FOR REAL — `DoctorProfile::parse`, default
///    `Strict` — fixing the bug where the LSP ignored the authored
///    profile entirely;
/// 3. resolve the full `[doctor]` config (presets + per-rule overrides)
///    via the shared `lazuli_doctor_config` resolver.
///
/// Falls back to a profile-only `Strict` config when no manifest is
/// reachable (single-file edits, scratch dirs) — matching the CLI's
/// behavior on manifest-less invocations.
fn resolve_workspace_config(doc_uri: &Url, workspace_root: Option<&Path>) -> ResolvedDoctorConfig {
    let manifest_path = doc_uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .and_then(|dir| find_lazurite_manifest(&dir))
        .or_else(|| workspace_root.and_then(find_lazurite_manifest));

    let Some(manifest_path) = manifest_path else {
        return ResolvedDoctorConfig::default();
    };
    let Ok(body) = std::fs::read_to_string(&manifest_path) else {
        return ResolvedDoctorConfig::default();
    };

    // Read `[doctor] profile` for real (default `Strict`) and resolve the
    // full `[doctor]` config in one shot via the shared resolver.
    ResolvedDoctorConfig::resolve_reading_profile(Some(&body))
        .unwrap_or_else(|_| ResolvedDoctorConfig::default())
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // W2 — capture the workspace root from the handshake so
        // `Lazurite.toml` (and its `[doctor] profile`) can be discovered
        // for documents whose own path doesn't resolve a manifest. Prefer
        // the first workspace folder, else `root_uri`.
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .map(|folder| folder.uri.clone())
            .or(params.root_uri)
            .and_then(|uri| uri.to_file_path().ok());
        if root.is_some() {
            *self.workspace_root.write().await = root;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        " ".to_owned(),
                        ":".to_owned(),
                        "{".to_owned(),
                        ",".to_owned(),
                    ]),
                    ..CompletionOptions::default()
                }),
                // IR Error-Vocab — three code actions per proposal §7.4:
                // scaffold the `errors` block, add `when_denied` to a
                // `policies.<category>:` line, add `when_denied` to a
                // `command.policy` line. Auth-refresh rotation contributes
                // text-edit scaffolds for `auth.sessions.rotation`.
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(tower_lsp::lsp_types::ServerInfo {
                name: server_name().to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Lazuli language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };

        self.documents
            .write()
            .await
            .insert(uri.clone(), change.text.clone());
        self.publish_diagnostics(uri, change.text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&uri) else {
            return Ok(None);
        };
        let Some(value) = handlers::hover_markdown_for_position(source, &uri, position) else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        // Row 30 — context-aware completions for `@cap.File(...)`
        // closed-catalog argument values fire first. Outside
        // `@cap.File(...)` falls back to the keyword list + Row 27
        // auth catalog values (argon2id/bcrypt/google/etc).
        let uri = params.text_document_position.text_document.uri;
        if is_design_lzi_uri(&uri) {
            return Ok(Some(CompletionResponse::Array(completion_items_for_uri(
                &uri,
            ))));
        }

        let position = params.text_document_position.position;

        // L0 #6 — `.lzx` v2 grammar completions (indent-aware: cells/
        // drawer/filters/search/sort/selection/bulk_actions/settings).
        if is_lzx_uri(&uri) {
            let documents = self.documents.read().await;
            if let Some(source) = documents.get(&uri) {
                let lifecycle_items = lifecycle_gate_completions(source, position);
                let route_guard_items = route_guard_completions(source, position);
                let lifecycle_block_items = lifecycle_block_completions(source, position);
                if lifecycle_items.is_some()
                    || route_guard_items.is_some()
                    || lifecycle_block_items.is_some()
                {
                    let merged = merge_completion_items(lifecycle_items, route_guard_items);
                    return Ok(Some(CompletionResponse::Array(merge_completion_items(
                        Some(merged),
                        lifecycle_block_items,
                    ))));
                }
                return Ok(Some(CompletionResponse::Array(
                    lzx_completion::completions_for_lzx(source, position),
                )));
            }
            return Ok(Some(CompletionResponse::Array(vec![])));
        }

        let documents = self.documents.read().await;
        if let Some(source) = documents.get(&uri) {
            // Lifecycle-block child / closed-invariant-catalog
            // completions inside a `lifecycle <field>` resource child.
            // Fires only when the cursor sits inside an enclosing
            // lifecycle block, so it's safe to check first.
            if let Some(items) = lifecycle_block_completions(source, position) {
                return Ok(Some(CompletionResponse::Array(items)));
            }
            // `@cap.File(...)` value completion fires first because
            // it is the narrowest context (cursor inside the
            // capability parenthesised body on a single line).
            if let Some(items) = cap_file_value_completions(source, position) {
                return Ok(Some(CompletionResponse::Array(items)));
            }
            // Cell O3 — `@owner_axis(through: ...)` FK column completion.
            // Same narrowness criterion as `@cap.File(...)`: cursor must
            // sit inside the parenthesised body after `through:`. Spec:
            // `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5.
            if let Some(items) = owner_axis_through_completions(source, position) {
                return Ok(Some(CompletionResponse::Array(items)));
            }
            // Cell A4 — `input.<field>` inside a `command` block surfaces
            // both `command.route` slots and `command.input` fields so
            // authors hit "no completion offered" at edit time instead
            // of "field not found" at codegen time. Route params lead.
            if let Some(items) = input_field_completions(source, position) {
                return Ok(Some(CompletionResponse::Array(items)));
            }
            // Wave B — context-aware kind-child / namespace-prefix /
            // rate-limit axis completion for `command`/`query.*`/
            // `api`/`agent`/`policy`/`effect`/`audit`/`rate_limit`.
            // Returns `None` to fall back to the global keyword list.
            if let Some(items) = context_aware_completions(source, position) {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }
        drop(documents);
        Ok(Some(CompletionResponse::Array(completion_items_for_uri(
            &uri,
        ))))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(symbols) = handlers::document_symbols_for_source(source) else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(formatted) = format_canonical_source(source) else {
            return Ok(None);
        };

        if formatted == *source {
            return Ok(Some(Vec::new()));
        }

        Ok(Some(vec![TextEdit::new(
            full_document_range(source),
            formatted,
        )]))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let position = params.range.start;
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&uri) else {
            return Ok(None);
        };
        Ok(handlers::code_actions_for_position(source, &uri, position))
    }
}

impl Backend {
    pub(crate) async fn publish_diagnostics(&self, uri: Url, source: String) {
        // W2 — resolve the workspace `[doctor]` config (profile + presets
        // + overrides) per publish so manifest edits take effect without a
        // server restart, and so single-file opens still discover the
        // nearest `Lazurite.toml`. Cheap: one small TOML parse.
        let workspace_root = self.workspace_root.read().await.clone();
        let config = resolve_workspace_config(&uri, workspace_root.as_deref());
        let diagnostics = diagnostics_for_uri_with_config(&uri, &source, &config);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

/// Run the LSP server over stdin/stdout. Blocks until the client
/// closes the stream — the canonical wiring used by `lazuli lsp`
/// (the CLI entry point) and by VS Code / Helix integrations that
/// launch the server as a subprocess.
///
/// Owns the live `Backend` instance and the tower-lsp `Server` plumbing;
/// no public API beyond this function — every diagnostic / completion /
/// hover surface routes through the trait impl on `Backend`.
///
/// ## Examples
///
/// ```no_run
/// // Run the LSP server over stdin/stdout. Blocks forever until the
/// // client disconnects.
/// # async fn run() {
/// lazuli_lsp::serve_stdio().await;
/// # }
/// ```
pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
        workspace_root: Arc::new(RwLock::new(None)),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
