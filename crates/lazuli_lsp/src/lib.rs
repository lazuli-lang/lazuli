use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lazuli_syntax::{Span, parse_feature_skeletons};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionItemKind,
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    Documentation, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, InsertTextFormat, MarkupContent, MarkupKind, MessageType, OneOf, Position,
    Range, ServerCapabilities, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Url, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

mod catalogs;
mod code_actions;
mod completion_items;
mod conventions;
mod diagnostics;
mod dispatch;
mod handlers;
mod hover;
mod keywords;
mod lzx_completion;
mod rate_limit;
mod source_scan;
mod types;

pub(crate) use dispatch::diagnostics_for_with_profile_inner;

// Per-catalog diagnostic producers (Rails-style layout). Each
// `pub(crate) use diagnostics::<catalog>::*` line preserves the
// pre-extraction ABI: producers continue to be reachable at
// `crate::<fn>` so `dispatch.rs` and out-of-process callers see no
// change. See `diagnostics/mod.rs` for the layout contract.
pub(crate) use diagnostics::agent::*;
pub(crate) use diagnostics::api::*;
pub(crate) use diagnostics::auth::*;
pub(crate) use diagnostics::cache::*;
pub(crate) use diagnostics::canonical_kinds::*;
pub(crate) use diagnostics::command::*;
pub(crate) use diagnostics::crypto::*;
pub(crate) use diagnostics::doctor_local::*;
pub(crate) use diagnostics::error::*;
pub(crate) use diagnostics::http_headers::*;
pub(crate) use diagnostics::notification::*;
pub(crate) use diagnostics::policy::*;
pub(crate) use diagnostics::query::*;
pub(crate) use diagnostics::webhook::*;

pub use catalogs::*;
pub(crate) use completion_items::{completion_items_for_uri, make_symbol, merge_completion_items};
pub(crate) use keywords::{DESIGN_KEYWORDS, KEYWORDS};
pub use code_actions::auth_refresh::auth_refresh_code_actions;
pub use code_actions::error_vocab::error_vocab_code_actions;
pub use code_actions::lifecycle_gate::lifecycle_gate_code_actions;
pub use code_actions::route_guard::route_guard_code_actions;
pub use conventions::conventions_list_completions;
pub use hover::*;
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

pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

pub(crate) struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
                if lifecycle_items.is_some() || route_guard_items.is_some() {
                    return Ok(Some(CompletionResponse::Array(merge_completion_items(
                        lifecycle_items,
                        route_guard_items,
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
    async fn publish_diagnostics(&self, uri: Url, source: String) {
        let diagnostics = diagnostics_for_uri(&uri, &source);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
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

pub(crate) fn lzx_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_surface: Option<(usize, String, bool, Option<String>)> = None;
    let mut in_audience = false;
    let mut in_view_extension = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-no-partial-override",
                "`.lzx` forbids partial overrides such as `+=`/`-=`. Redeclare the whole view for this audience/tenant so the block remains a local truth.",
            ));
        }

        if leading_spaces(line) == 4
            && let Some(target) = trimmed.strip_prefix("opens ")
            && !target.contains('(')
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-open-binding",
                "view navigation should bind route arguments explicitly, e.g. `opens detail(id: row.id)`, so generators do not infer row identity.",
            ));
        }

        if leading_spaces(line) == 6
            && let Some(target) = trimmed.strip_prefix("submit ")
            && is_identifier(target.trim())
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-submit-target",
                "platform form submits should use an explicit command reference such as `command.create` or `customer.command.capture_lead`, not a bare verb.",
            ));
        }

        if leading_spaces(line) == 0 {
            in_view_extension = false;

            if let Some((surface_line, surface_header, has_uses_experience, _)) =
                current_surface.take()
                && !has_uses_experience
            {
                diagnostics.push(simple_canonical_diagnostic(
                    surface_line,
                    &surface_header,
                    DiagnosticSeverity::ERROR,
                    "lzx-surface-dependency",
                    "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
                ));
            }

            in_audience = false;

            if trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() == 2 && matches!(parts.get(1), Some(&"web" | &"mobile")) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                    ));
                } else if parts.len() < 3 {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "concrete `.lzx` surfaces use `surface <experience> <platform>`, with protected platforms `web` or `mobile`.",
                    ));
                } else {
                    if matches!(parts.get(1), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-surface-header",
                            "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                        ));
                    }
                    if !matches!(parts.get(2), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-platform",
                            "canonical `.lzx` platform suffixes are protected: use `web` or `mobile` in the surface header; product axes belong in `audience`/`tenant` blocks.",
                        ));
                    }
                }
                current_surface = Some((
                    line_index,
                    line.to_owned(),
                    false,
                    parts.get(2).map(|platform| (*platform).to_owned()),
                ));
            }

            continue;
        }

        if leading_spaces(line) == 2 {
            in_view_extension = trimmed.starts_with("extends @anchor.");
        } else if leading_spaces(line) < 2 {
            in_view_extension = false;
        }

        if in_view_extension && leading_spaces(line) == 4 && trimmed.starts_with("block ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-extension-slot",
                "view extensions should place blocks under an explicit slot, e.g. `slot aside` then `block @client.tag_editor`, so composition order and placement are deterministic.",
            ));
        }

        if let Some((_, _, has_uses_experience, platform)) = current_surface.as_mut() {
            if leading_spaces(line) == 2 {
                if trimmed.starts_with("uses experience ") {
                    *has_uses_experience = true;
                    in_audience = false;
                    continue;
                }

                if trimmed.starts_with("audience ") {
                    in_audience = true;
                    continue;
                }

                if trimmed.starts_with("view ") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-audience-required",
                        "concrete platform views live under `audience ...` blocks. Product axes are source syntax, not filename-only convention.",
                    ));
                }
            } else if leading_spaces(line) <= 2 {
                in_audience = false;
            } else if trimmed.starts_with("view ") && !in_audience {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-audience-required",
                    "concrete platform views live under `audience ...` blocks.",
                ));
            } else if in_audience
                && leading_spaces(line) == 4
                && platform.as_deref() == Some("mobile")
                && let Some(view_type) = trimmed.split_whitespace().nth(2)
                && matches!(view_type, "Table" | "SidePanel")
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "lzx-mobile-primitive",
                    "mobile projections should use mobile-native primitives such as `List`, `Screen`, or `Sheet` instead of web-oriented `Table`/`SidePanel`.",
                ));
            }
        }
    }

    if let Some((surface_line, surface_header, has_uses_experience, _)) = current_surface
        && !has_uses_experience
    {
        diagnostics.push(simple_canonical_diagnostic(
            surface_line,
            &surface_header,
            DiagnosticSeverity::ERROR,
            "lzx-surface-dependency",
            "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
        ));
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct LzxRouteViewFacts {
    routes: HashSet<String>,
    references: Vec<(usize, String, String)>,
    unbound_target_actions: Vec<(usize, String)>,
}

#[derive(Debug)]
pub(crate) struct LzxAppRouteFacts {
    line_index: usize,
    line: String,
    has_path: bool,
    has_to: bool,
    has_surface: bool,
    has_audience: bool,
    declared_routes: HashSet<String>,
    path_params: Vec<String>,
    route_references: Vec<(usize, String, String)>,
}

pub(crate) fn lzx_route_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_experience = false;
    let mut current_view: Option<LzxRouteViewFacts> = None;
    let mut current_route: Option<LzxAppRouteFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            if let Some(route) = current_route.take() {
                diagnostics.extend(lzx_app_route_diagnostics(route));
            }
            if trimmed.starts_with("route ") {
                current_route = Some(LzxAppRouteFacts {
                    line_index,
                    line: line.to_owned(),
                    has_path: false,
                    has_to: false,
                    has_surface: false,
                    has_audience: false,
                    declared_routes: HashSet::new(),
                    path_params: Vec::new(),
                    route_references: Vec::new(),
                });
            }
            in_experience = trimmed.starts_with("experience ");
            continue;
        }

        if let Some(route) = current_route.as_mut() {
            if leading_spaces(line) == 2 {
                if let Some(path) = trimmed.strip_prefix("path ") {
                    route.has_path = true;
                    route
                        .path_params
                        .extend(lzx_declared_path_params(unquote_lzx_literal(path.trim())));
                } else if let Some(routes) = trimmed.strip_prefix("route ") {
                    for slot in routes
                        .split(',')
                        .filter_map(|part| route_slot_name(part.trim()))
                    {
                        route.declared_routes.insert(slot.to_owned());
                    }
                } else if let Some(target) = trimmed.strip_prefix("to ") {
                    route.has_to = true;
                    for reference in path_references(target, "route.") {
                        route.route_references.push((
                            line_index,
                            line.to_owned(),
                            reference.to_owned(),
                        ));
                    }
                } else if trimmed.starts_with("surface ") {
                    route.has_surface = true;
                } else if trimmed.starts_with("audience ") {
                    route.has_audience = true;
                }
            }
            continue;
        }

        if !in_experience {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("view ") {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            current_view = Some(LzxRouteViewFacts {
                routes: HashSet::new(),
                references: Vec::new(),
                unbound_target_actions: Vec::new(),
            });
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            continue;
        }

        let Some(view) = current_view.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4
            && let Some(route) = trimmed.strip_prefix("route ")
            && let Some(name) = route_slot_name(route)
        {
            view.routes.insert(name.to_owned());
        }

        if leading_spaces(line) == 4
            && let Some((_, target)) = trimmed
                .strip_prefix("action ")
                .and_then(|rest| rest.split_once(" -> "))
            && !target.contains('(')
            && (target.contains(".command.") || target.contains(".workflow."))
        {
            view.unbound_target_actions
                .push((line_index, line.to_owned()));
        }

        for reference in lzx_route_references(trimmed) {
            view.references
                .push((line_index, line.to_owned(), reference.to_owned()));
        }
    }

    if let Some(view) = current_view {
        diagnostics.extend(lzx_route_view_diagnostics(view));
    }
    if let Some(route) = current_route {
        diagnostics.extend(lzx_app_route_diagnostics(route));
    }

    diagnostics
}

pub(crate) fn lzx_app_route_diagnostics(route: LzxAppRouteFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !route.has_path {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should declare a concrete `path`; `surface <name> web|mobile` decides whether it is a web URL path or mobile route pattern.",
        ));
    }
    if !route.has_to {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should declare `to <experience>.view.<view>(...)` so generated links and navigation have a typed target.",
        ));
    }
    if !route.has_surface || !route.has_audience {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should bind `surface <name> web|mobile` and `audience <name>` so authorization and platform routing are explicit.",
        ));
    }

    for path_param in route.path_params {
        if !route.declared_routes.contains(&path_param) {
            diagnostics.push(simple_canonical_diagnostic(
                route.line_index,
                &route.line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route path parameter `{path_param}` should be declared with `route {path_param}: <Type>` so route builders are type-safe.",
                ),
            ));
        }
    }

    for (line_index, line, reference) in route.route_references {
        if !route.declared_routes.contains(&reference) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route target references `route.{reference}` but the route does not declare `route {reference}: ...`.",
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn lzx_declared_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b':' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_'))
            {
                end += 1;
            }
            if end > start {
                params.push(path[start..end].to_owned());
            }
            index = end;
            continue;
        }

        if bytes[index] == b'[' {
            let start = index + 1;
            if let Some(close_offset) = path[start..].find(']') {
                let raw = &path[start..start + close_offset];
                let name = raw.trim_start_matches("...");
                if is_identifier(name) {
                    params.push(name.to_owned());
                }
                index = start + close_offset + 1;
                continue;
            }
        }

        index += 1;
    }

    params
}

pub(crate) fn unquote_lzx_literal(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

pub(crate) fn is_quoted_lzx_literal(value: &str) -> bool {
    value.starts_with('"') && value.ends_with('"') && value.len() >= 2
}

pub(crate) fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn lzx_route_view_diagnostics(view: LzxRouteViewFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line, route) in view.references {
        if !view.routes.contains(&route) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-route-contract",
                &format!(
                    "view references `route.{route}` but does not declare `route {route}: ...`; route bindings should be explicit in the abstract experience."
                ),
            ));
        }
    }

    if !view.routes.is_empty() {
        for (line_index, line) in view.unbound_target_actions {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-action-route-binding",
                "actions in routed views should pass route arguments explicitly, e.g. `action archive -> feature.workflow.name.transition(id: route.id)`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

pub(crate) fn lzx_route_references(source: &str) -> Vec<&str> {
    path_references(source, "route.")
}

pub(crate) fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after = &rest[start + prefix.len()..];
        let len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if len > 0 {
            references.push(&after[..len]);
        }
        rest = &after[len..];
    }

    references
}

pub(crate) fn lzx_filename_diagnostics(uri: &Url, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some(file_name) = uri
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::to_owned)
    else {
        return diagnostics;
    };

    let expected_platform = lzx_platform_from_file_name(&file_name);
    let surface_header = first_lzx_surface_header(source);

    match (expected_platform, surface_header) {
        (Some(platform), Some((line_index, line, header_platform))) => {
            if header_platform != platform {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-file-header-mismatch",
                    &format!(
                        "`{file_name}` is a `{platform}` projection, so its header should use `surface <experience> {platform}`."
                    ),
                ));
            }
        }
        (Some(platform), None) => {
            diagnostics.push(simple_canonical_diagnostic(
                0,
                source.lines().next().unwrap_or(""),
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                &format!(
                    "`{file_name}` is a `{platform}` projection, so it should declare `surface <experience> {platform}`."
                ),
            ));
        }
        (None, Some((line_index, line, _))) if file_name.ends_with(".lzx") => {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                "abstract `.lzx` files declare `experience <name>`; concrete surfaces belong in `.web.lzx` or `.mobile.lzx` files.",
            ));
        }
        _ => {}
    }

    diagnostics
}

pub(crate) fn lzx_platform_from_file_name(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(".web.lzx") {
        Some("web")
    } else if file_name.ends_with(".mobile.lzx") {
        Some("mobile")
    } else {
        None
    }
}

pub(crate) fn first_lzx_surface_header(source: &str) -> Option<(usize, &str, &str)> {
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 0 || !trimmed.starts_with("surface ") {
            continue;
        }
        let platform = trimmed.split_whitespace().nth(2)?;
        return Some((line_index, line, platform));
    }

    None
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

pub(crate) fn generated_summary_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed == "summary" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "summary-generated",
                "`summary` is generated by `lazuli inspect --expand=summary`; do not author it in canonical source.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn non_goals_shape_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_non_goals = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            in_non_goals = trimmed == "non_goals";
            continue;
        }

        if !in_non_goals {
            continue;
        }

        if leading_spaces(line) == 4 {
            if matches!(trimmed, "delegated_to" | "out_of_scope") {
                continue;
            }
            // Iron-hand context vocabulary — the flat quoted-string
            // form is the preferred canonical shape for new features
            // (see `docs/canonical-semantics.md#feature-context-vocabulary`).
            // Partitioned `delegated_to` / `out_of_scope` blocks stay
            // valid for the existing fixture but are no longer the only
            // shape worth surfacing. Only flag bareword `key: value`
            // entries that match neither the flat shape nor the
            // partitioned shape.
            if trimmed.starts_with('"') {
                continue;
            }

            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "non-goals-shape",
                "`non_goals` entries must be either bare quoted strings (flat shape) or grouped under `delegated_to` / `out_of_scope` (partitioned shape). See docs/canonical-semantics.md#feature-context-vocabulary.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn defaults_policy_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_defaults = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }

        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("policy ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "defaults-policy-for",
                "feature-level policy defaults should use `policy_for jobs, webhooks: @actor.system` so the fallback scope is explicit.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn namespace_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        for namespace in namespace_references(line) {
            if !is_allowed_reference_namespace(namespace) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "namespace-catalog",
                    "unknown `@...` namespace. Allowed namespaces are `@role`, `@scope`, `@actor`, `@policy`, `@semantic`, `@cap`, `@pii`, `@key`, `@fn`, `@hook`, `@validator`, `@adapter`, `@client`, `@query_modifier`, `@anchor`, `@llm`, `@tool`, and `@trace`.",
                ));
                break;
            }
        }
    }

    diagnostics
}

pub(crate) fn namespace_references(line: &str) -> Vec<&str> {
    // Mask the spans inside double-quoted string literals so e.g.
    // `requires customer.email = "ada@example.com"` does not surface
    // `@example` as a stray namespace. The scan walks the original byte
    // slice; the mask just decides which `@` positions are eligible.
    let bytes = line.as_bytes();
    let mut in_quote = false;
    let mut quote_ranges: Vec<(usize, usize)> = Vec::new();
    let mut quote_start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'"' {
            if in_quote {
                quote_ranges.push((quote_start, i + 1));
                in_quote = false;
            } else {
                quote_start = i;
                in_quote = true;
            }
        }
    }
    let in_string = |pos: usize| {
        quote_ranges
            .iter()
            .any(|(start, end)| pos >= *start && pos < *end)
    };

    let mut namespaces = Vec::new();
    let mut cursor = 0;

    while cursor < line.len() {
        let Some(rel) = line[cursor..].find('@') else {
            break;
        };
        let at_pos = cursor + rel;
        if in_string(at_pos) {
            cursor = at_pos + 1;
            continue;
        }
        let after_at = &line[at_pos + 1..];
        let Some(dot) = after_at.find('.') else {
            cursor = at_pos + 1;
            continue;
        };

        let namespace = &after_at[..dot];
        if !namespace.is_empty()
            && namespace
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            namespaces.push(namespace);
        }

        cursor = at_pos + 1 + dot + 1;
    }

    namespaces
}

pub(crate) fn is_allowed_reference_namespace(namespace: &str) -> bool {
    matches!(
        namespace,
        "role"
            | "scope"
            | "actor"
            | "policy"
            | "semantic"
            | "cap"
            | "pii"
            | "key"
            | "fn"
            | "hook"
            | "validator"
            | "adapter"
            | "client"
            | "query_modifier"
            | "anchor"
            | "llm"
            | "tool"
            // Observability bucket cycle row 35 — reference-only namespace
            // for `trigger @trace.<name>` on subscriber jobs. Reserved
            // names live in `lazuli_ir::built_in_trace_events()`; LSP
            // checks resolution in `trigger_trace_unknown_diagnostics`.
            | "trace"
            // i18n bucket cycle row 54 — reference-only namespace for
            // `rule message @translation.<key>` (and post-pilot surface
            // labels). Keys are declared in feature `translation` blocks;
            // doctor's `rule_message_ref_unresolved` validates resolution.
            | "translation"
    )
}


pub(crate) fn file_capability_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || !line.contains("@cap.File") {
            continue;
        }

        let Some(args) = capability_args(line, "File") else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `max_size:<size>` and `accept:<mime>` so generated upload APIs and validators have an explicit contract.",
            ));
            continue;
        };

        warn_unknown_capability_args(
            &mut diagnostics,
            line_index,
            line,
            "@cap.File",
            &args,
            // Row 30 — `visibility` + `signed_ttl` are typed arguments
            // recognised by the analyzer (`type_ref_from_syntax`); add
            // them to the canonical set so the LSP no longer warns on
            // the canonical authoring form.
            &["max_size", "accept", "visibility", "signed_ttl"],
        );

        if !args.iter().any(|(key, _)| key == "max_size") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `max_size:<size>` such as `25mb`.",
            ));
        }

        if !args.iter().any(|(key, _)| key == "accept") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `accept:<mime>` such as `text/csv`.",
            ));
        }

        if let Some(max_size) = capability_arg(&args, "max_size")
            && !is_file_size_literal(max_size)
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` max_size should use a positive size literal such as `500kb`, `25mb`, or `1gb`.",
            ));
        }

        if let Some(accept) = capability_arg(&args, "accept")
            && accept.trim().is_empty()
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` accept should name a MIME type such as `text/csv` or `image/png`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn capability_args(line: &str, capability: &str) -> Option<Vec<(String, String)>> {
    let marker = format!("@cap.{capability}(");
    let start = line.find(&marker)? + marker.len();
    let args = line[start..].split_once(')')?.0;

    Some(
        args.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.split_once(':')
                    .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                    .unwrap_or_else(|| (part.to_owned(), String::new()))
            })
            .collect(),
    )
}

pub(crate) fn capability_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(arg_key, _)| arg_key == key)
        .map(|(_, value)| value.as_str())
}

pub(crate) fn warn_unknown_capability_args(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    capability: &str,
    args: &[(String, String)],
    allowed: &[&str],
) {
    for (key, _) in args {
        if !allowed.contains(&key.as_str()) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "capability-arguments",
                &format!(
                    "{capability} only accepts canonical arguments: {}.",
                    allowed.join(", ")
                ),
            ));
        }
    }
}

pub(crate) fn is_duration_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "s" | "m" | "h" | "d")
}

pub(crate) fn is_file_size_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "b" | "kb" | "mb" | "gb")
}

pub(crate) fn is_retention_duration_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "h" | "d" | "w" | "mo" | "y")
}

pub(crate) fn is_key_scope(value: &str) -> bool {
    value
        .strip_prefix("@key.")
        .is_some_and(|scope| is_identifier(scope))
}

pub(crate) fn type_namespace_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_env = false;
    let mut in_app = false;
    let mut in_registry = false;
    let mut app_child: Option<&str> = None;
    let mut registry_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            in_env = trimmed == "env";
            in_app = trimmed.starts_with("app ");
            in_registry = trimmed
                .split_whitespace()
                .next()
                .is_some_and(|keyword| keyword == "registry");
            app_child = None;
            registry_child = None;
            continue;
        }

        if in_env {
            continue;
        }

        if in_app {
            if leading_spaces(line) == 2 {
                app_child = trimmed.split_whitespace().next();
            }
            if app_child == Some("env") {
                continue;
            }
        }

        if in_registry {
            if leading_spaces(line) == 2 {
                registry_child = trimmed.split_whitespace().next();
            }
            if registry_child == Some("env") {
                continue;
            }
        }

        let Some(ty) = typed_line_type(trimmed) else {
            continue;
        };

        if matches!(ty, "Email" | "Money") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "type-namespace",
                "semantic types should use the `@semantic.*` namespace, e.g. `@semantic.Email` or `@semantic.Money`.",
            ));
        } else if matches!(ty, "File" | "Secret") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "type-namespace",
                "capability types should use the `@cap.*` namespace, e.g. `@cap.File`, `@cap.Hashed(...)`, `@cap.Encrypted(...)`, or `@cap.Token(...)`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn sql_return_type_diagnostics(source: &str) -> Vec<Diagnostic> {
    let declared_types = collect_declared_type_names_by_feature(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut in_sql_query = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                in_sql_query = false;
            }
            2 => {
                in_sql_query = false;
            }
            4 => {
                in_sql_query = trimmed.starts_with("query.sql ") || trimmed.starts_with("query.view ");
            }
            6 if in_sql_query && trimmed.starts_with("returns ") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some(return_type) = trimmed
                    .trim_start_matches("returns ")
                    .split_whitespace()
                    .next()
                    .map(canonical_return_type_name)
                else {
                    continue;
                };

                if is_builtin_return_type(return_type) {
                    continue;
                }

                if !declared_types
                    .get(feature)
                    .is_some_and(|types| types.contains(return_type))
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "sql-return-type",
                        &format!(
                            "`query.sql`/`query.view` return type `{return_type}` should resolve to a local `record` or `resource`; SQL result shapes are not inferred from the SQL file."
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    diagnostics
}

pub(crate) fn collect_declared_type_names_by_feature(source: &str) -> HashMap<String, HashSet<String>> {
    let mut types = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                let feature = feature_name(trimmed);
                types.entry(feature.clone()).or_insert_with(HashSet::new);
                current_feature = Some(feature);
                current_top = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
            }
            4 if current_top == Some("domain") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let first = trimmed.split_whitespace().next();
                if matches!(first, Some("resource" | "record" | "enum"))
                    && let Some(name) = trimmed.split_whitespace().nth(1)
                {
                    types
                        .entry(feature.to_owned())
                        .or_insert_with(HashSet::new)
                        .insert(name.to_owned());
                }
            }
            _ => {}
        }
    }

    types
}

pub(crate) fn canonical_return_type_name(return_type: &str) -> &str {
    return_type
        .strip_suffix("[]")
        .unwrap_or(return_type)
        .trim_end_matches('?')
}

pub(crate) fn is_builtin_return_type(return_type: &str) -> bool {
    matches!(
        return_type,
        "Text" | "Integer" | "Decimal" | "Boolean" | "ID" | "DateTime" | "JSON"
    ) || return_type.starts_with("@semantic.")
        || return_type.starts_with("@cap.")
}

pub(crate) fn typed_line_type(trimmed_line: &str) -> Option<&str> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let ty = rhs.trim().split_whitespace().next()?;

    if ty.starts_with('"') || ty.is_empty() {
        None
    } else {
        Some(ty)
    }
}

pub(crate) fn is_float_in_range(value: &str, min: f64, max: f64) -> bool {
    value
        .parse::<f64>()
        .map(|v| v >= min && v <= max)
        .unwrap_or(false)
}

pub(crate) fn derived_field_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(rest) = field_typed_rhs(trimmed) else {
            continue;
        };

        let (before_derived, after_derived) = match split_derived_from(rest) {
            Some(parts) => parts,
            None => continue,
        };

        if after_derived.trim().is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "derived-field-contract",
                "`derived from` requires an expression: `<name>: <Type> derived from <expression>`.",
            ));
            continue;
        }

        let mut emitted_requiredness = false;
        for forbidden in ["required", "optional"] {
            if before_derived
                .split_whitespace()
                .any(|token| token == forbidden)
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "derived-field-contract",
                    "`derived from` fields are computed at read time and must not declare `required` or `optional`.",
                ));
                emitted_requiredness = true;
                break;
            }
        }

        if !emitted_requiredness && contains_top_level_eq(after_derived) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "derived-field-contract",
                "`derived from` fields are computed at read time and must not declare `default` (no trailing `= <value>`).",
            ));
        }
    }

    diagnostics
}

/// Cut A.9 — file-local checks on `approval` blocks declared inside
/// commands. Required children present (`by`, `timeout`, `then`),
/// `then` value in the closed catalog, `by` non-empty. Cross-feature
/// role resolution lives in doctor.
pub(crate) fn approval_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 4 && trimmed == "approval" {
            let header_line = i;
            let mut has_by = false;
            let mut by_nonempty = false;
            let mut has_timeout = false;
            let mut timeout_nonempty = false;
            let mut has_then = false;
            let mut then_invalid: Option<String> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let body = lines[j];
                let body_trim = body.trim_start();
                if body_trim.is_empty() || body_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(body) <= 4 {
                    break;
                }
                if leading_spaces(body) == 6 {
                    if let Some(rest) = body_trim.strip_prefix("by ") {
                        has_by = true;
                        by_nonempty = rest.split(',').any(|s| !s.trim().is_empty());
                    } else if let Some(rest) = body_trim.strip_prefix("timeout ") {
                        has_timeout = true;
                        timeout_nonempty = !rest.trim().is_empty();
                    } else if let Some(rest) = body_trim.strip_prefix("then ") {
                        has_then = true;
                        let value = rest.trim().to_owned();
                        if !matches!(value.as_str(), "deny" | "proceed") {
                            then_invalid = Some(value);
                        }
                    }
                }
                j += 1;
            }

            let mut missing: Vec<&str> = Vec::new();
            if !has_by || !by_nonempty {
                missing.push("by");
            }
            if !has_timeout || !timeout_nonempty {
                missing.push("timeout");
            }
            if !has_then {
                missing.push("then");
            }
            if !missing.is_empty() {
                diagnostics.push(simple_canonical_diagnostic(
                    header_line,
                    line,
                    DiagnosticSeverity::ERROR,
                    "approval_contract_diagnostics",
                    &format!(
                        "`approval` block is missing required children: {}.",
                        missing.join(", "),
                    ),
                ));
            }
            if let Some(value) = then_invalid {
                diagnostics.push(simple_canonical_diagnostic(
                    header_line,
                    line,
                    DiagnosticSeverity::ERROR,
                    "approval_contract_diagnostics",
                    &format!(
                        "`approval then {value}` is invalid — closed catalog is `deny` or `proceed`."
                    ),
                ));
            }
            i = j;
            continue;
        }
        i += 1;
    }

    diagnostics
}

/// Cut A.8 — flag authored `event.trace <name>` declarations whose
/// `<name>` is reserved by the IR's built-in trace event registry.
/// File-local fast feedback that mirrors doctor's
/// `event_trace_reserved_name_diagnostics`.
pub(crate) fn reserved_trace_event_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("event.trace ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lazuli_ir::is_reserved_trace_event_name(name) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "event_trace_reserved_name_diagnostics",
                    &format!(
                        "`event.trace {name}` is reserved by the IR as a built-in trace event; the runtime emits it automatically. Authoring this declaration is rejected — subscribe via `job ... trigger event.trace {name}` instead."
                    ),
                ));
            }
        }
    }
    diagnostics
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

pub(crate) fn emits_derived_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Match `emits <event> from creates|updates|deletes`. The header is
        // the only place where this clause is canonical.
        let Some(rest) = trimmed.strip_prefix("emits ") else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        let Some(_event_token) = tokens.next() else {
            continue;
        };
        let Some(from_keyword) = tokens.next() else {
            continue;
        };
        if from_keyword != "from" {
            continue;
        }
        let Some(effect) = tokens.next() else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "emits-derived-contract",
                "`emits <event> from <effect>` requires the effect block name (`creates`, `updates`, or `deletes`).",
            ));
            continue;
        };
        if !matches!(effect, "creates" | "updates" | "deletes") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "emits-derived-contract",
                "`emits <event> from <effect>` requires `creates`, `updates`, or `deletes`. the runtime derives the payload by name match against that effect's bindings.",
            ));
            continue;
        }

        // The body, if present, must be empty. Inline bindings duplicate what
        // the cited effect already declares, defeating the point of `from`.
        let header_indent = leading_spaces(line);
        let child_indent = header_indent + 2;
        for next in lines.iter().skip(line_index + 1) {
            let next_trimmed = next.trim_start();
            if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                continue;
            }
            let next_indent = leading_spaces(next);
            if next_indent <= header_indent {
                break;
            }
            if next_indent == child_indent && next_trimmed.contains(" = ") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "emits-derived-contract",
                    "`emits <event> from <effect>` derives the payload from the cited effect's bindings; inline `<field> = <value>` children duplicate that mapping. Remove the body or drop `from <effect>`.",
                ));
                break;
            }
        }
    }

    diagnostics
}

pub(crate) fn has_many_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("has_many ") else {
            continue;
        };

        let Some((name_part, type_part)) = rest.split_once(':') else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` collections use `has_many <name>: <Type> [inverse <field>]`.",
            ));
            continue;
        };

        let name = name_part.trim();
        if name.is_empty() || name.contains(' ') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` requires a single identifier before `:`.",
            ));
            continue;
        }

        let mut tokens = type_part.split_whitespace();
        let Some(_type_token) = tokens.next() else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` requires a target type after `:`.",
            ));
            continue;
        };

        match tokens.next() {
            None => {}
            Some("inverse") => {
                if tokens.next().is_none() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "has-many-contract",
                        "`inverse` requires a field name on the target resource.",
                    ));
                }
            }
            Some(unexpected) => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "has-many-contract",
                    &format!(
                        "unexpected `{unexpected}` after `has_many <name>: <Type>`. Only `inverse <field>` is allowed.",
                    ),
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn split_derived_from(rhs: &str) -> Option<(&str, &str)> {
    if let Some(pos) = rhs.find(" derived from ") {
        return Some((&rhs[..pos], &rhs[pos + " derived from ".len()..]));
    }
    if let Some(stripped) = rhs.strip_suffix(" derived from") {
        return Some((stripped, ""));
    }
    None
}

pub(crate) fn contains_top_level_eq(expr: &str) -> bool {
    let mut depth_paren: i32 = 0;
    let mut in_string = false;
    let mut prev = ' ';
    for ch in expr.chars() {
        match ch {
            '"' if prev != '\\' => in_string = !in_string,
            '(' if !in_string => depth_paren += 1,
            ')' if !in_string => depth_paren -= 1,
            '=' if !in_string && depth_paren == 0 && prev == ' ' => return true,
            _ => {}
        }
        prev = ch;
    }
    false
}

pub(crate) fn field_typed_rhs(trimmed: &str) -> Option<&str> {
    let (lhs, rhs) = trimmed.split_once(':')?;
    if lhs.contains(' ') || lhs.is_empty() {
        return None;
    }
    let rhs = rhs.trim_start();
    if rhs.is_empty() || rhs.starts_with('"') {
        return None;
    }
    Some(rhs)
}

pub(crate) fn validation_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("validate ") && !trimmed.starts_with("validate @validator.") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validators are referenced through `validates @validator.<name>`; the scope (field or resource) is declared by the validator's `Validator[<scope>]` type in `extensions`.",
            ));
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("validates ") else {
            continue;
        };

        let argument = rest.trim();

        // Canonical: `validates @validator.<name>`
        if argument.starts_with("@validator.") {
            continue;
        }

        // Legacy with explicit scope: `validates field <name> @validator.<name>`
        // or `validates resource @validator.<name>`. Both forms still parse but
        // warn — the validator's `Validator[<scope>]` type already carries the
        // scope, so repeating it at the call site is redundant.
        let (legacy_form, target) = if let Some(field_rest) = argument.strip_prefix("field ") {
            let target = field_rest.split_whitespace().skip(1).next().unwrap_or("");
            ("legacy-scoped-field", target)
        } else if let Some(resource_rest) = argument.strip_prefix("resource") {
            ("legacy-scoped-resource", resource_rest.trim())
        } else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validators are referenced through `validates @validator.<name>`; the scope (field or resource) is declared by the validator's `Validator[<scope>]` type in `extensions`.",
            ));
            continue;
        };

        if target.starts_with('"') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "inline `\"./path.go\"` validator references are legacy. Declare the validator under `extensions.validator <name>: Validator[<scope>] at \"./path.go\"` and reference it as `validates @validator.<name>`.",
            ));
        } else if target.starts_with("@validator.") {
            // Legacy scope keyword present but otherwise canonical — warn that
            // the scope is redundant.
            let hint = match legacy_form {
                "legacy-scoped-field" => {
                    "drop the `field <name>` prefix; the validator's `Validator[<scope>]` type already names the field."
                }
                _ => {
                    "drop the `resource` prefix; the validator's `Validator[<scope>]` type already targets the resource."
                }
            };
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                &format!("`validates @validator.<name>` is the canonical form: {hint}"),
            ));
        } else if !target.is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validator references should use the `@validator.<name>` namespace. Declare the validator under `extensions.validator <name>` first.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn extension_declaration_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            continue;
        }

        if current_top != Some("extensions") || leading_spaces(line) != 4 {
            continue;
        }

        let Some((keyword, contract)) = extension_declaration(trimmed) else {
            continue;
        };
        let expected = expected_extension_keyword(contract);

        if keyword == "server" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "extension-declaration-namespace",
                "extension declarations should use the same namespace keyword as their call site, e.g. `fn`, `hook`, `validator`, `adapter`, `query_modifier`, or `client`, not `server`.",
            ));
        } else if let Some(expected) = expected {
            if keyword != expected {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "extension-declaration-namespace",
                    "extension declaration keyword should match the contract namespace used at call sites.",
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn extension_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
    let mut parts = trimmed_line.split_whitespace();
    let keyword = parts.next()?;
    if !matches!(
        keyword,
        "client" | "server" | "fn" | "hook" | "validator" | "adapter" | "query_modifier"
    ) {
        return None;
    }

    let after_colon = trimmed_line.split_once(':')?.1.trim();
    let contract = after_colon.split(['[', ' ']).next()?;
    Some((keyword, contract))
}

pub(crate) fn expected_extension_keyword(contract: &str) -> Option<&'static str> {
    match contract {
        "CellRenderer" | "ViewBlock" | "FormField" => Some("client"),
        "Function" => Some("fn"),
        "Hook" => Some("hook"),
        "Validator" => Some("validator"),
        "IntegrationAdapter" => Some("adapter"),
        "QueryModifier" => Some("query_modifier"),
        _ => None,
    }
}

pub(crate) fn event_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed == "observability_only" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-kind",
                "observability-only events should use `event.trace <name>` instead of the `observability_only` modifier.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn event_trace_trigger_diagnostics(source: &str) -> Vec<Diagnostic> {
    let trace_events = collect_trace_events(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        let Some(event_ref) = trimmed.strip_prefix("trigger event ") else {
            continue;
        };
        let event_ref = event_ref.split_whitespace().next().unwrap_or(event_ref);
        let is_trace = if event_ref.contains('.') {
            trace_events.contains(event_ref)
        } else {
            current_feature
                .as_deref()
                .map(|feature| trace_events.contains(&format!("{feature}.{event_ref}")))
                .unwrap_or(false)
        };

        if is_trace {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-trace-trigger",
                "`event.trace` declarations are outside the reaction graph and should not be used as job triggers; promote the event to `event` first.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_trace_events(source: &str) -> HashSet<String> {
    let mut events = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_group_prefix: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            current_group_prefix = None;
            continue;
        }

        if leading_spaces(line) == 4 {
            current_group_prefix = event_group_prefix(trimmed).map(str::to_owned);
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("event.trace ") {
            if let (Some(feature), Some(event)) = (
                current_feature.as_deref(),
                trimmed.split_whitespace().nth(1),
            ) {
                events.insert(format!("{feature}.{event}"));
            }
        } else if leading_spaces(line) == 6 && trimmed.starts_with("event.trace ") {
            if let (Some(feature), Some(prefix), Some(event)) = (
                current_feature.as_deref(),
                current_group_prefix.as_deref(),
                trimmed.split_whitespace().nth(1),
            ) {
                events.insert(format!(
                    "{feature}.{}",
                    qualify_group_event_name(prefix, event)
                ));
            }
        }
    }

    events
}

pub(crate) fn event_locator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') || trimmed.starts_with("event.trace ") {
            continue;
        }

        if line.contains("payload = event") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-locator-namespace",
                "do not assign the implicit event object wholesale. Use explicit `payload.<field>` or `envelope.<field>` values.",
            ));
            continue;
        }

        if line.contains("event.") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-locator-namespace",
                "event-triggered jobs should use `envelope.*` for bus metadata and `payload.*` for authored event fields, e.g. `envelope.id` or `payload.customer_id`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn target_binding_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            continue;
        }

        if matches!(current_top, Some("command" | "job"))
            && (line.contains("self.") || line.contains("(self)") || line.contains("= self"))
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "target-binding",
                "commands and declarative jobs should use `target` for the loaded target record; reserve `self` for rules and workflow transitions.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn rule_self_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if !trimmed.starts_with("deny ") {
            continue;
        }

        let Some((_, predicate)) = trimmed.split_once(" when ") else {
            continue;
        };

        if let Some(alias) = legacy_rule_subject_alias(predicate) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "rule-self",
                &format!(
                    "rules should use `self` for the target snapshot, not `{alias}`. Use `self.<field>` in rule predicates."
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn required_field_nil_rule_diagnostics(source: &str) -> Vec<Diagnostic> {
    let required_fields = collect_required_resource_fields(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if !trimmed.starts_with("deny ") {
            continue;
        }

        let Some(feature) = current_feature.as_deref() else {
            continue;
        };
        let Some((operation, predicate)) = trimmed
            .strip_prefix("deny ")
            .and_then(|rest| rest.split_once(" when "))
        else {
            continue;
        };
        let Some(resource) = operation
            .split_once('.')
            .map(|(resource, _)| resource.trim())
        else {
            continue;
        };

        for field in required_fields
            .iter()
            .filter_map(|(field_feature, field_resource, field)| {
                (field_feature == feature && field_resource == resource).then_some(field)
            })
        {
            if predicate_references_nil_self_field(predicate, field) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "required-field-nil-rule",
                    &format!(
                        "rule predicate checks `self.{field}` against `nil`, but `{resource}.{field}` is declared `required`; make the field optional or remove the impossible nil branch.",
                    ),
                ));
            }
        }
    }

    diagnostics
}




pub(crate) fn collect_required_resource_fields(source: &str) -> HashSet<(String, String, String)> {
    let mut fields = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            current_top = None;
            current_resource = None;
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            current_resource = None;
            continue;
        }

        if current_top != Some("domain") {
            continue;
        }

        if leading_spaces(line) == 4 {
            current_resource = trimmed
                .strip_prefix("resource ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_owned);
            continue;
        }

        if leading_spaces(line) == 6
            && trimmed.contains(" required")
            && let (Some(feature), Some(resource), Some(field)) = (
                current_feature.as_deref(),
                current_resource.as_deref(),
                field_name(trimmed),
            )
        {
            fields.insert((feature.to_owned(), resource.to_owned(), field.to_owned()));
        }
    }

    fields
}

pub(crate) fn predicate_references_nil_self_field(predicate: &str, field: &str) -> bool {
    let left = format!("self.{field}");
    predicate.contains(&format!("{left} = nil")) || predicate.contains(&format!("{left} != nil"))
}

pub(crate) fn legacy_rule_subject_alias(predicate: &str) -> Option<&str> {
    let first = predicate.split_whitespace().next()?;
    let (head, _) = first.split_once('.')?;

    if matches!(
        head,
        "self" | "target" | "ctx" | "params" | "payload" | "envelope" | "route" | "input"
    ) {
        None
    } else if head
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
    {
        Some(head)
    } else {
        None
    }
}

#[derive(Debug)]
pub(crate) struct AnchorWhitelistEntry {
    anchor: String,
    feature: String,
    line_index: usize,
    line: String,
}

pub(crate) fn anchor_whitelist_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut whitelisted = Vec::new();
    let mut extensions = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_view_anchor: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_view_anchor = None;
            }
            2 => {
                current_view_anchor = None;
                if let Some(anchor) = extends_anchor(trimmed) {
                    if let Some(feature) = current_feature.as_deref() {
                        extensions.insert((anchor.to_owned(), feature.to_owned()));
                    }
                }
            }
            4 => {
                current_view_anchor = view_anchor(trimmed).map(str::to_owned);
            }
            6 => {
                let Some(anchor) = current_view_anchor.as_deref() else {
                    continue;
                };

                for feature in extensible_by_features(trimmed) {
                    whitelisted.push(AnchorWhitelistEntry {
                        anchor: anchor.to_owned(),
                        feature,
                        line_index,
                        line: line.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    for entry in whitelisted {
        if !extensions.contains(&(entry.anchor.clone(), entry.feature.clone())) {
            diagnostics.push(simple_canonical_diagnostic(
                entry.line_index,
                &entry.line,
                DiagnosticSeverity::WARNING,
                "anchor-whitelist-unused",
                &format!(
                    "`extensible_by` lists feature `{}`, but that feature does not extend `@anchor.{}`.",
                    entry.feature, entry.anchor
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn test_block_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut current_test_context: Option<String> = None;
    // Indent at which the most recent `tests` keyword sat. Assertion
    // lines live STRICTLY DEEPER; any line at this indent or shallower
    // closes the block (the legacy `<= 4` heuristic assumed tests were
    // always two levels deep, which broke when the canonical fixture
    // nested `tests` under `workflow → transition` at indent 10).
    let mut test_block_indent: Option<usize> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = leading_spaces(line);

        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }

        if let Some(block_indent) = test_block_indent {
            if indent <= block_indent {
                current_test_context = None;
                test_block_indent = None;
            }
        }

        if trimmed == "tests" {
            let context = test_context(&stack);
            if let Some(context) = context {
                current_test_context = Some(context);
                test_block_indent = Some(indent);
            } else {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-placement",
                    "`tests` blocks are allowed only as the last child of a command, workflow transition, rule, or view anchor block.",
                ));
                current_test_context = None;
            }
        } else if let Some(context) = current_test_context.as_deref() {
            if indent >= 6 && !is_valid_test_assertion(context, trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-vocabulary",
                    "unknown test assertion for this construct. Use the closed tests vocabulary for command, workflow transition, rule, or view anchor blocks.",
                ));
            } else if indent >= 6
                && context == "command"
                && (trimmed.starts_with("permits @") || trimmed.starts_with("forbids @"))
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-generated-policy-matrix",
                    "command policy actor-matrix tests are generated from `policy @policy.*`; author only predicate tests that add behavior beyond the policy.",
                ));
            }
        }

        if let Some(kind) = stack_kind(trimmed) {
            stack.push((indent, kind.to_owned()));
        }
    }

    diagnostics
}

pub(crate) fn stack_kind(trimmed_line: &str) -> Option<&'static str> {
    let first = trimmed_line.split_whitespace().next()?;

    if first == "command" {
        Some("command")
    } else if first == "rule" {
        Some("rule")
    // Block-form workflow transitions (`transition <name>` followed by
    // indented `from`/`to`/`policy`/`emits`/`tests` children) are the
    // canonical surface today. The legacy inline form
    // (`<state>: <state> -> <name>`) is captured by
    // `is_transition_line` below for back-compat with older fixtures.
    } else if first == "transition" {
        Some("transition")
    } else if view_anchor(trimmed_line).is_some() {
        Some("anchor")
    } else if is_transition_line(trimmed_line) {
        Some("transition")
    } else {
        None
    }
}

pub(crate) fn test_context(stack: &[(usize, String)]) -> Option<String> {
    stack
        .last()
        .filter(|(_, kind)| matches!(kind.as_str(), "command" | "transition" | "rule" | "anchor"))
        .map(|(_, kind)| kind.clone())
}

pub(crate) fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((lhs, rhs)) = trimmed_line.split_once(':') else {
        return false;
    };

    !lhs.trim().is_empty() && rhs.contains("->")
}

pub(crate) fn is_valid_test_assertion(context: &str, trimmed_line: &str) -> bool {
    match context {
        "command" => {
            trimmed_line.starts_with("permits @")
                || trimmed_line.starts_with("forbids @")
                || trimmed_line.starts_with("allows when ")
                || trimmed_line.starts_with("denies when ")
        }
        "transition" => {
            trimmed_line.starts_with("allows from ")
                || trimmed_line.starts_with("denies from ")
                || trimmed_line.starts_with("allows as @")
                || trimmed_line.starts_with("denies as @")
        }
        "rule" => {
            trimmed_line.starts_with("allows when ") || trimmed_line.starts_with("denies when ")
        }
        "anchor" => {
            trimmed_line.starts_with("accepted by ") || trimmed_line.starts_with("rejected by ")
        }
        _ => false,
    }
}

pub(crate) fn view_anchor(trimmed_line: &str) -> Option<&str> {
    let marker = " id @anchor.";
    let (_, rest) = trimmed_line.split_once(marker)?;
    rest.split_whitespace().next()
}

pub(crate) fn extends_anchor(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line.strip_prefix("extends @anchor.")?;
    rest.split_whitespace().next()
}

pub(crate) fn extensible_by_features(trimmed_line: &str) -> Vec<String> {
    let Some(rest) = trimmed_line.strip_prefix("extensible_by ") else {
        return Vec::new();
    };

    rest.split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn extension_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }

        if line.contains("ext.") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "extension-namespace",
                "extension references should use capability namespaces such as `@client.name`, `@fn.name`, `@hook.name`, `@validator.name`, or `@adapter.name` instead of `ext.*`.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct SensitiveFieldFacts {
    feature: String,
    resource: String,
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug, Default)]
pub(crate) struct FieldPolicyFacts {
    read: bool,
    write: bool,
}

pub(crate) fn field_security_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let sensitive_fields = collect_sensitive_fields(source);
    let field_policies = collect_field_policy_facts(source);
    let mut diagnostics = Vec::new();

    for field in sensitive_fields {
        let has_policy = field_policies
            .get(&(
                field.feature.clone(),
                field.resource.clone(),
                field.field.clone(),
            ))
            .is_some_and(|policy| policy.read && policy.write);

        if !has_policy {
            diagnostics.push(simple_canonical_diagnostic(
                field.line_index,
                &field.line,
                DiagnosticSeverity::WARNING,
                "field-security-policy",
                &format!(
                    "sensitive field `{}.{}` uses `@pii.*` or `@cap.*` and must declare field-level `read` and `write` policies under `policies fields {}`.",
                    field.resource, field.field, field.resource
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_sensitive_fields(source: &str) -> Vec<SensitiveFieldFacts> {
    let mut fields = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_resource = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_resource = None;
            }
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
            }
            6 if current_top == Some("domain") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource) = current_resource.as_deref() else {
                    continue;
                };
                let Some((field, _)) = typed_param(trimmed) else {
                    continue;
                };
                if is_sensitive_field_line(line) {
                    fields.push(SensitiveFieldFacts {
                        feature: feature.to_owned(),
                        resource: resource.to_owned(),
                        field: field.to_owned(),
                        line_index,
                        line: line.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    fields
}

pub(crate) fn is_sensitive_field_line(line: &str) -> bool {
    line.contains("@pii.")
        || line.contains("@cap.Encrypted")
        || line.contains("@cap.E2ee")
        || line.contains("@cap.Hashed")
        || line.contains("@cap.Token")
}

#[derive(Debug)]
pub(crate) struct PiiResourceFacts {
    feature: String,
    resource: String,
    line_index: usize,
    line: String,
}

pub(crate) fn retention_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let pii_resources = collect_pii_resource_facts(source);
    let retention = collect_retention_facts(source, &mut diagnostics);

    for resource in pii_resources {
        if !retention.feature_defaults.contains(&resource.feature)
            && !retention
                .resources
                .contains(&(resource.feature.clone(), resource.resource.clone()))
        {
            diagnostics.push(simple_canonical_diagnostic(
                resource.line_index,
                &resource.line,
                DiagnosticSeverity::WARNING,
                "retention-contract",
                &format!(
                    "resource `{}` stores `@pii.*` fields and should declare `retention <duration> then delete|anonymize|archive`, or inherit a feature default retention contract.",
                    resource.resource
                ),
            ));
        }
    }

    diagnostics
}

#[derive(Debug, Default)]
pub(crate) struct RetentionFacts {
    feature_defaults: HashSet<String>,
    resources: HashSet<(String, String)>,
}

pub(crate) fn collect_retention_facts(source: &str, diagnostics: &mut Vec<Diagnostic>) -> RetentionFacts {
    let mut facts = RetentionFacts::default();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_resource = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_resource = None;
            }
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
            }
            _ => {}
        }

        if !trimmed.starts_with("retention ") {
            continue;
        }

        if let Some(message) = retention_contract_error(trimmed) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "retention-contract",
                message,
            ));
            continue;
        }

        match (
            leading_spaces(line),
            current_top,
            current_feature.as_deref(),
        ) {
            (4, Some("defaults"), Some(feature)) => {
                facts.feature_defaults.insert(feature.to_owned());
            }
            (6, Some("domain"), Some(feature)) => {
                if let Some(resource) = current_resource.as_deref() {
                    facts
                        .resources
                        .insert((feature.to_owned(), resource.to_owned()));
                }
            }
            _ => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "retention-contract",
                    "`retention` belongs under `defaults` or as a resource child.",
                ));
            }
        }
    }

    facts
}

pub(crate) fn retention_contract_error(trimmed_line: &str) -> Option<&'static str> {
    let parts: Vec<_> = trimmed_line.split_whitespace().collect();
    if parts.len() != 4 || parts[2] != "then" {
        return Some(
            "retention contracts use `retention <duration|forever> then delete|anonymize|archive`.",
        );
    }

    if parts[1] != "forever" && !is_retention_duration_literal(parts[1]) {
        return Some(
            "retention duration should be `forever` or a positive duration such as `30d`, `24mo`, or `7y`.",
        );
    }

    if !matches!(parts[3], "delete" | "anonymize" | "archive") {
        return Some("retention action should be `delete`, `anonymize`, or `archive`.");
    }

    None
}

pub(crate) fn collect_pii_resource_facts(source: &str) -> Vec<PiiResourceFacts> {
    let mut resources = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<(String, usize, String)> = None;
    let mut current_resource_has_pii = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some((resource, resource_line_index, resource_line)) = current_resource.take()
                && current_resource_has_pii
                && let Some(feature) = current_feature.as_deref()
            {
                resources.push(PiiResourceFacts {
                    feature: feature.to_owned(),
                    resource,
                    line_index: resource_line_index,
                    line: resource_line,
                });
            }
            current_resource_has_pii = false;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(|resource| (resource.to_owned(), line_index, line.to_owned()));
            }
            6 if current_top == Some("domain") && current_resource.is_some() => {
                if line.contains("@pii.") {
                    current_resource_has_pii = true;
                }
            }
            _ => {}
        }
    }

    if let Some((resource, resource_line_index, resource_line)) = current_resource
        && current_resource_has_pii
        && let Some(feature) = current_feature
    {
        resources.push(PiiResourceFacts {
            feature,
            resource,
            line_index: resource_line_index,
            line: resource_line,
        });
    }

    resources
}

pub(crate) fn write_window_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("write_window ") else {
            continue;
        };

        if leading_spaces(line) != 4 {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "write-window-contract",
                "`write_window` belongs as a command child.",
            ));
            continue;
        }

        let parts: Vec<_> = rest.split_whitespace().collect();
        if parts.len() != 4 || parts[0] != "by" || parts[2] != "within" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "write-window-contract",
                "write-window guards use `write_window by <date-expression> within <window-reference>`.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct AppOperationalFacts {
    line_index: usize,
    line: String,
    has_uses: bool,
    has_targets: bool,
    has_environments: bool,
    has_runtime: bool,
    has_deploy: bool,
    has_architecture: bool,
    has_services: bool,
    has_communication: bool,
    deploy_has_migrations: bool,
    deploy_has_rollback: bool,
    runtime_units: Vec<AppRuntimeUnitFacts>,
    services: Vec<AppServiceFacts>,
}

impl AppOperationalFacts {
    fn new(line_index: usize, line: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            has_uses: false,
            has_targets: false,
            has_environments: false,
            has_runtime: false,
            has_deploy: false,
            has_architecture: false,
            has_services: false,
            has_communication: false,
            deploy_has_migrations: false,
            deploy_has_rollback: false,
            runtime_units: Vec::new(),
            services: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppRuntimeUnitFacts {
    line_index: usize,
    line: String,
    name: String,
    has_serves_or_runs: bool,
    has_healthcheck_or_readiness: bool,
}

impl AppRuntimeUnitFacts {
    fn new(line_index: usize, line: &str, name: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            name: name.to_owned(),
            has_serves_or_runs: false,
            has_healthcheck_or_readiness: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppServiceFacts {
    line_index: usize,
    line: String,
    name: String,
    has_owns: bool,
}

impl AppServiceFacts {
    fn new(line_index: usize, line: &str, name: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            name: name.to_owned(),
            has_owns: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppIntegrationFacts;

impl AppIntegrationFacts {
    fn new() -> Self {
        Self
    }
}

pub(crate) fn workspace_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_workspace = false;
    let mut current_child: Option<&'static str> = None;
    let mut in_gateway = false;
    let mut in_gateway_route = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_workspace = trimmed.starts_with("workspace ");
            current_child = None;
            in_gateway = false;
            in_gateway_route = false;
            if in_workspace {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 || !is_type_name(parts[1]) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "workspace-contract",
                        "workspace contracts use `workspace <Name>` as the distributed-system header.",
                    ));
                }
            }
            continue;
        }

        if !in_workspace {
            continue;
        }

        match leading {
            2 => {
                in_gateway = false;
                in_gateway_route = false;
                if let Some(rest) = trimmed.strip_prefix("shared_registry ") {
                    current_child = None;
                    if !is_quoted_lzx_literal(rest.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "workspace-registry-contract",
                            "shared registries use `shared_registry \"./registry.lzi\"`.",
                        ));
                    }
                } else if let Some(name) = trimmed.strip_prefix("gateway ") {
                    current_child = Some("gateway");
                    in_gateway = true;
                    if !is_identifier(name.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "workspace-gateway-contract",
                            "workspace gateways use `gateway <name>`.",
                        ));
                    }
                } else {
                    current_child = match trimmed {
                        "apps" => Some("apps"),
                        "boundaries" => Some("boundaries"),
                        "communication" => Some("communication"),
                        _ => {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "workspace-contract",
                                "workspace blocks use `apps`, `shared_registry`, `boundaries`, `communication`, and `gateway <name>`.",
                            ));
                            None
                        }
                    };
                }
            }
            4 => match current_child {
                Some("apps") => validate_workspace_app_line(&mut diagnostics, line_index, line),
                Some("boundaries") => {
                    validate_workspace_boundary_line(&mut diagnostics, line_index, line)
                }
                Some("communication") => {
                    validate_workspace_communication_line(&mut diagnostics, line_index, line)
                }
                Some("gateway") if in_gateway => {
                    in_gateway_route =
                        validate_workspace_gateway_route_line(&mut diagnostics, line_index, line);
                }
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "workspace-contract",
                    "four-space workspace declarations must belong to `apps`, `boundaries`, `communication`, or a `gateway` route.",
                )),
            },
            6 => {
                if in_gateway && in_gateway_route {
                    validate_workspace_gateway_route_child(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "workspace-gateway-contract",
                        "six-space workspace declarations are only valid under a gateway route.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "workspace-contract",
                "workspace declarations use two, four, or six spaces of indentation.",
            )),
        }
    }

    diagnostics
}

pub(crate) fn validate_workspace_app_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(
        parts.as_slice(),
        [name, "at", path] if is_identifier(name) && is_quoted_lzx_literal(path)
    ) || matches!(
        parts.as_slice(),
        [name, "external", "contract", contract]
            if is_identifier(name) && is_quoted_lzx_literal(contract)
    );

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-app-contract",
            "workspace apps use `<name> at \"./app.lzi\"` or `<name> external contract \"name.version\"`.",
        ));
    }
}

pub(crate) fn validate_workspace_boundary_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !matches!(
        parts.as_slice(),
        [app, direction, pattern]
            if is_identifier(app) && matches!(*direction, "publishes" | "consumes") && !pattern.is_empty()
    ) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-boundary-contract",
            "workspace boundaries use `<app> publishes <event_pattern>` or `<app> consumes <event_pattern>`.",
        ));
    }
}

pub(crate) fn validate_workspace_communication_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(
        parts.as_slice(),
        ["propagate", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" ")).iter().all(|item| {
                    matches!(
                        item.as_str(),
                        "actor" | "tenant" | "trace_id" | "request_id" | "locale"
                    )
                })
    ) || matches!(
        parts.as_slice(),
        ["default", "sync", "internal", value] if matches!(*value, "rpc" | "http" | "in_process")
    ) || matches!(
        parts.as_slice(),
        ["default", "async", value] if matches!(*value, "event_bus" | "in_process")
    );

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-communication-contract",
            "workspace communication uses `propagate ...`, `default sync internal rpc|http|in_process`, or `default async event_bus|in_process`.",
        ));
    }
}

pub(crate) fn validate_workspace_gateway_route_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix("route ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway routes use `route \"/path/*\" to app <name>`.",
        ));
        return false;
    };
    let Some((_path, tail)) = quoted_prefix(rest.trim()) else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway route paths must be quoted.",
        ));
        return false;
    };
    let parts: Vec<_> = tail.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["to", "app", target] if is_identifier(target));
    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway routes currently target apps with `to app <name>`.",
        ));
    }
    valid
}

pub(crate) fn validate_workspace_gateway_route_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["auth", "propagate"])
        || matches!(parts.as_slice(), ["tenant", "propagate"])
        || matches!(parts.as_slice(), ["timeout", value] if is_quoted_lzx_literal(value))
        || matches!(parts.as_slice(), ["rate_limit", value] if is_quoted_lzx_literal(value));

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "gateway route children use `auth propagate`, `tenant propagate`, `timeout \"...\"`, or `rate_limit \"...\"`.",
        ));
    }
}

pub(crate) fn quoted_prefix(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}

pub(crate) fn external_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_contract = false;
    let mut current_child: Option<&'static str> = None;
    let mut in_event_payload = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_contract = trimmed.starts_with("contract ");
            current_child = None;
            in_event_payload = false;
            if in_contract {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 || !is_contract_name(parts[1]) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "contract-header",
                        "external contracts use `contract <namespace.version>`, e.g. `contract acme.ai.v1`.",
                    ));
                }
            }
            continue;
        }

        if !in_contract {
            continue;
        }

        match leading {
            2 => {
                current_child = None;
                in_event_payload = false;
                if let Some(rest) = trimmed.strip_prefix("purpose ") {
                    if !is_quoted_lzx_literal(rest.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-purpose",
                            "contract purpose uses a quoted sentence.",
                        ));
                    }
                } else if let Some(rest) = trimmed.strip_prefix("compatibility ") {
                    if !matches!(rest.trim(), "backward" | "none" | "manual") {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-compatibility",
                            "contract compatibility uses `backward`, `none`, or `manual`.",
                        ));
                    }
                } else if trimmed.starts_with("import ") {
                    validate_contract_import_line(&mut diagnostics, line_index, line, trimmed);
                } else if let Some(name) = named_block_name(trimmed, "record") {
                    current_child = Some("record");
                    if !is_type_name(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-record",
                            "contract records use `record <TypeName>`.",
                        ));
                    }
                } else if let Some(name) = named_block_name(trimmed, "operation") {
                    current_child = Some("operation");
                    if !is_identifier(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-operation",
                            "contract operations use `operation <name>`.",
                        ));
                    }
                } else if let Some(name) = named_block_name(trimmed, "event") {
                    current_child = Some("event");
                    if !is_identifier(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-event",
                            "contract events use `event <name>`.",
                        ));
                    }
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "contract-shape",
                        "contract blocks use `purpose`, `compatibility`, `import`, `record`, `operation`, and `event` children.",
                    ));
                }
            }
            4 => match current_child {
                Some("record") => {
                    validate_contract_field_line(&mut diagnostics, line_index, line)
                }
                Some("operation") => {
                    validate_contract_operation_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("event") => {
                    if let Some(rest) = trimmed.strip_prefix("topic ") {
                        in_event_payload = false;
                        if !is_quoted_lzx_literal(rest.trim()) {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "contract-event-topic",
                                "contract event topics use `topic \"event.name\"`.",
                            ));
                        }
                    } else if trimmed == "payload" {
                        in_event_payload = true;
                    } else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-event",
                            "contract event children use `topic \"...\"` or `payload`.",
                        ));
                    }
                }
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "contract-shape",
                    "four-space contract declarations must belong to `record`, `operation`, or `event` blocks.",
                )),
            },
            6 => {
                if current_child == Some("event") && in_event_payload {
                    validate_contract_field_line(&mut diagnostics, line_index, line);
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "contract-shape",
                        "six-space contract declarations are only valid inside event `payload`.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "contract-shape",
                "contract declarations use two, four, or six spaces of indentation.",
            )),
        }
    }

    diagnostics
}

pub(crate) fn validate_contract_import_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !matches!(
        parts.as_slice(),
        ["import", format, source]
            if matches!(*format, "openapi" | "asyncapi" | "proto" | "json_schema" | "avro")
                && is_quoted_lzx_literal(source)
    ) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-import",
            "contract imports use `import openapi|asyncapi|proto|json_schema|avro \"./path\"`.",
        ));
    }
}

pub(crate) fn validate_contract_operation_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["transport", value] if matches!(*value, "http" | "rpc" | "event"))
        || matches!(parts.as_slice(), ["method", value] if matches!(*value, "GET" | "POST" | "PUT" | "PATCH" | "DELETE"))
        || matches!(parts.as_slice(), ["path", value] if is_quoted_lzx_literal(value))
        || matches!(parts.as_slice(), ["input", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["output", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["output", "stream", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["auth", value] if matches!(*value, "service" | "none" | "propagate"))
        || matches!(parts.as_slice(), ["timeout", value] if is_quoted_lzx_literal(value))
        || is_contract_operation_retry(&parts)
        || is_contract_operation_idempotency(&parts)
        || is_contract_operation_error(&parts);

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-operation",
            "operation children use `transport http|rpc|event`, `method GET|POST|PUT|PATCH|DELETE`, `path \"...\"`, `input Type`, `output [stream] Type`, `auth service|none|propagate`, `timeout \"...\"`, `retry <n> [backoff <strategy>]`, `idempotency by <field>[, <field>...]`, or `error <Name> status <code> [expose <field>...]`.",
        ));
    }
}

pub(crate) fn is_contract_operation_retry(parts: &[&str]) -> bool {
    if parts.first().copied() != Some("retry") {
        return false;
    }
    match parts.len() {
        2 => parts[1].parse::<u32>().is_ok(),
        4 => {
            parts[1].parse::<u32>().is_ok()
                && parts[2] == "backoff"
                && matches!(parts[3], "exponential" | "linear" | "fixed")
        }
        _ => false,
    }
}

pub(crate) fn is_contract_operation_idempotency(parts: &[&str]) -> bool {
    parts.len() >= 3
        && parts[0] == "idempotency"
        && parts[1] == "by"
        && parts.iter().skip(2).all(|t| !t.is_empty())
}

pub(crate) fn is_contract_operation_error(parts: &[&str]) -> bool {
    if parts.first().copied() != Some("error") {
        return false;
    }
    if parts.len() < 2 || !is_type_name(parts[1]) {
        return false;
    }
    // Allow `error <Name>` alone, or with optional `status <code>` and
    // `expose <field>...` clauses in any order.
    let mut iter = parts.iter().skip(2);
    while let Some(token) = iter.next() {
        match *token {
            "status" => {
                let Some(value) = iter.next() else {
                    return false;
                };
                if value.parse::<u16>().is_err() {
                    return false;
                }
            }
            "expose" => {
                if iter.next().is_none() {
                    return false;
                }
                // Consume the rest as expose fields.
                while iter.next().is_some() {}
                return true;
            }
            _ => return false,
        }
    }
    true
}

pub(crate) fn validate_contract_field_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let Some((name, rest)) = trimmed.split_once(':') else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-field",
            "contract fields use `<name>: <Type> required|optional`.",
        ));
        return;
    };

    let parts: Vec<_> = rest.split_whitespace().collect();
    if !is_identifier(name.trim())
        || parts.len() < 2
        || !is_contract_type_token(parts[0])
        || !parts
            .last()
            .is_some_and(|last| matches!(*last, "required" | "optional"))
        || parts[1..parts.len() - 1]
            .iter()
            .any(|part| !part.starts_with('@'))
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-field",
            "contract fields use `<name>: <Type> [@pii.* ...] required|optional`.",
        ));
    }
}

pub(crate) fn is_contract_name(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    is_identifier(first)
        && parts.all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

pub(crate) fn is_contract_type_token(value: &str) -> bool {
    value.starts_with("@semantic.")
        || value.starts_with("@cap.")
        || is_type_name(value)
        || matches!(
            value,
            "ID" | "Text" | "Integer" | "Decimal" | "Float" | "Boolean" | "DateTime" | "Date"
        )
}

pub(crate) fn app_operational_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_app: Option<AppOperationalFacts> = None;
    let mut current_app_child: Option<&'static str> = None;
    let mut current_runtime_unit: Option<usize> = None;
    let mut current_service: Option<usize> = None;
    let mut current_service_child: Option<&'static str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<AppIntegrationFacts> = None;
    let mut current_integration_child: Option<&'static str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            if let Some(app) = current_app.take() {
                diagnostics.extend(app_operational_block_diagnostics(app));
            }
            current_app_child = None;
            current_runtime_unit = None;
            current_service = None;
            current_service_child = None;
            current_env_group = None;
            current_integration = None;
            current_integration_child = None;

            if trimmed.starts_with("app ") {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "app-operational-contract",
                        "app manifests use `app <Name>` as the entrypoint header.",
                    ));
                }
                current_app = Some(AppOperationalFacts::new(line_index, line));
            }
            continue;
        }

        let Some(app) = current_app.as_mut() else {
            continue;
        };

        match leading_spaces(line) {
            2 => {
                current_runtime_unit = None;
                current_service = None;
                current_service_child = None;
                current_env_group = None;
                current_integration = None;
                current_integration_child = None;
                if let Some(child) = app_child_block(trimmed) {
                    current_app_child = Some(child);
                    match child {
                        "uses" => app.has_uses = true,
                        "targets" => app.has_targets = true,
                        "environments" => app.has_environments = true,
                        "architecture" => app.has_architecture = true,
                        "services" => app.has_services = true,
                        "communication" => app.has_communication = true,
                        "runtime" => app.has_runtime = true,
                        "deploy" => app.has_deploy = true,
                        _ => {}
                    }
                    validate_app_child_header(&mut diagnostics, line_index, line, trimmed);
                } else if is_app_scalar_child(trimmed) {
                    current_app_child = None;
                    validate_app_scalar_child(&mut diagnostics, line_index, line, trimmed);
                } else {
                    current_app_child = None;
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "app manifests own app/runtime contracts: use `uses`, `packs`, `bindings`, `targets`, `environments`, `urls`, `env`, `integrations`, `capabilities`, `architecture`, `services`, `communication`, `runtime`, or `deploy` blocks.",
                    ));
                }
            }
            4 => match current_app_child {
                Some("uses") => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with("feature ") {
                        let parts: Vec<_> = trimmed.split_whitespace().collect();
                        if parts.len() < 2 {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "app-operational-contract",
                                "`uses` feature entries use `feature <name> [at \"./path.lzi\"]` or a feature name.",
                            ));
                        }
                    }
                }
                Some("packs") => validate_app_pack_use_line(&mut diagnostics, line_index, line),
                Some("bindings") => validate_app_binding_line(&mut diagnostics, line_index, line),
                Some("targets") => validate_app_target_line(&mut diagnostics, line_index, line),
                Some("environments") => {
                    if !is_identifier(trimmed) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-operational-contract",
                            "environment names should be identifiers such as `local`, `staging`, or `production`.",
                        ));
                    }
                }
                Some("urls") => validate_app_url_line(&mut diagnostics, line_index, line),
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                }
                Some("capabilities") => {
                    validate_app_capability_line(&mut diagnostics, line_index, line)
                }
                Some("integrations") => {
                    validate_app_integration_header(&mut diagnostics, line_index, line, trimmed);
                    current_integration = Some(AppIntegrationFacts::new());
                    current_integration_child = None;
                }
                Some("architecture") => {
                    validate_app_architecture_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("services") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() != 2 || parts[0] != "service" || !is_identifier(parts[1]) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-service-contract",
                            "service boundaries use `service <name>` under `services`.",
                        ));
                        current_service = None;
                        current_service_child = None;
                    } else {
                        app.services
                            .push(AppServiceFacts::new(line_index, line, parts[1]));
                        current_service = app.services.len().checked_sub(1);
                        current_service_child = None;
                    }
                }
                Some("communication") => {
                    validate_app_communication_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("runtime") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() != 2 || parts[0] != "unit" || !is_identifier(parts[1]) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "app-runtime-contract",
                            "runtime units use `unit <name>` under `runtime`.",
                        ));
                        current_runtime_unit = None;
                    } else {
                        app.runtime_units
                            .push(AppRuntimeUnitFacts::new(line_index, line, parts[1]));
                        current_runtime_unit = app.runtime_units.len().checked_sub(1);
                    }
                }
                Some("deploy") => {
                    validate_app_deploy_line(&mut diagnostics, app, line_index, line, trimmed)
                }
                // Cut A.11 — `cors` children are handled by
                // `cors_contract_diagnostics`. Skip here to avoid the
                // "unknown app block" warning firing on
                // `allow_origins` / `allow_credentials` / `max_age`.
                Some("cors") => {}
                // Roadmap §1.10 — `headers` children are handled by
                // `headers_contract_diagnostics` (file-local shape) +
                // doctor's `headers-contract` (closed catalogs +
                // production-profile completeness). Skip here so the
                // "unknown app block" warning does not fire on
                // `csp` / `hsts` / `x_frame_options` etc.
                Some("headers") => {}
                // Roadmap §1.2 — `cookie` / `proxy` / `limits`
                // children are doctor-validated (cookie profile
                // children at indent 6; proxy/limits scalars at indent
                // 4). Skip the "unknown app block" warning here.
                Some("cookie") | Some("proxy") | Some("limits") => {}
                // Observability bucket cycle row 36 — `logging` /
                // `tracing` children are handled by
                // `app_logging_tracing_diagnostics` (doctor) and the
                // closed-catalog completion in
                // `observability_catalog_detail`. Skip the
                // "unknown app block" warning here.
                Some("logging") | Some("tracing") => {}
                // i18n bucket cycle — `locale` children
                // (`default`/`supported`/`fallback`) are validated by
                // `parse_app_manifest`; skip the "unknown app block"
                // warning here.
                Some("locale") => {}
                // ir-route-guards Cell PARSE-1 — app-level guard
                // defaults are validated by the parser/analyzer.
                Some("route_guard") => {}
                // Encryption bucket cycle — `encryption` children are
                // `key @key.<scope>` lines validated by doctor's
                // encryption_binding_diagnostics; skip the "unknown
                // app block" warning here.
                Some("encryption") => {}
                Some(_) | None => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "app-operational-contract",
                    "nested app manifest declarations must live under a known app block.",
                )),
            },
            6 => {
                if current_app_child == Some("env") {
                    if current_env_group.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-env-contract",
                            "six-space env declarations must follow `group <name>` inside `env`.",
                        ));
                    } else {
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                } else if current_app_child == Some("integrations") {
                    if current_integration.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-integration-contract",
                            "integration children must follow `<name>: <CapabilityType>` under `integrations`.",
                        ));
                        continue;
                    }

                    validate_app_integration_child(
                        &mut diagnostics,
                        &mut current_integration_child,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("runtime") {
                    let Some(unit_index) = current_runtime_unit else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-runtime-contract",
                            "runtime unit children must follow a `unit <name>` declaration.",
                        ));
                        continue;
                    };

                    validate_app_runtime_unit_child(
                        &mut diagnostics,
                        &mut app.runtime_units[unit_index],
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("services") {
                    let Some(service_index) = current_service else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-service-contract",
                            "service boundary children must follow a `service <name>` declaration.",
                        ));
                        continue;
                    };
                    validate_app_service_child(
                        &mut diagnostics,
                        &mut app.services[service_index],
                        &mut current_service_child,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("encryption") {
                    // Encryption bucket cycle — body of a
                    // `key @key.<scope>` block: `source <expr>`,
                    // `algorithm <name>`, `rotation <cadence>`.
                    // Doctor's `encryption_binding_diagnostics` owns
                    // the closed-catalog validation; the LSP just
                    // needs to NOT fire the generic six-space
                    // warning here.
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "six-space app manifest declarations are only valid inside `env group`, `integrations`, `runtime unit`, `services service`, or `encryption key` blocks.",
                    ));
                }
            }
            8 => {
                if current_app_child == Some("integrations")
                    && current_integration_child == Some("credentials")
                {
                    validate_app_integration_credential_line(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("services")
                    && current_service_child == Some("exposes")
                {
                    validate_app_service_exposure_line(&mut diagnostics, line_index, line, trimmed);
                } else if current_app_child == Some("runtime")
                    && (trimmed.starts_with("source ")
                        || trimmed.starts_with("strategy ")
                        || trimmed.starts_with("fallback "))
                {
                    // i18n bucket cycle — `locale_negotiate` body lines
                    // sit at indent 8 under `runtime unit api`. The body
                    // grammar is `source <axis>` / `strategy <name>` /
                    // `fallback <tag>`; doctor validates the catalog.
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "eight-space app manifest declarations are only valid inside `integrations credentials`, `services service exposes`, or `runtime unit locale_negotiate` blocks.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-operational-contract",
                "app manifest declarations use two, four, six, or eight spaces of indentation.",
            )),
        }
    }

    if let Some(app) = current_app.take() {
        diagnostics.extend(app_operational_block_diagnostics(app));
    }

    diagnostics
}

pub(crate) fn app_child_block(trimmed: &str) -> Option<&'static str> {
    let first = trimmed.split_whitespace().next()?;
    match first {
        "uses" => Some("uses"),
        "packs" => Some("packs"),
        "bindings" => Some("bindings"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "cors" => Some("cors"),
        // Roadmap §1.10 — `headers` block. Body validated by
        // `headers_contract_diagnostics`; the LSP only needs to
        // recognize the header so warnings don't fire on the
        // children.
        "headers" => Some("headers"),
        // Roadmap §1.2 — HTTP hygiene blocks. Bodies are validated
        // by doctor's app_(cookie|proxy|limits)_contract_diagnostics
        // (closed catalog, parseable size/duration). LSP only needs
        // to recognize the header so warnings don't fire on the
        // children.
        "cookie" => Some("cookie"),
        "proxy" => Some("proxy"),
        "limits" => Some("limits"),
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
        "route_guard" => Some("route_guard"),
        // Observability bucket cycle row 36 — `app.logging` /
        // `app.tracing` are first-class app blocks. Child slots
        // (`level`/`format`/`redact`/`sample_rate` for logging;
        // `propagate`/`sample_rate`/`exporter` for tracing) are
        // closed-catalog-checked by doctor.
        "logging" => Some("logging"),
        "tracing" => Some("tracing"),
        // i18n bucket cycle — `app.locale` block (default / supported /
        // fallback). Supersedes bare `default_locale` scalar.
        "locale" => Some("locale"),
        // Encryption bucket cycle — `app.encryption` block carries one
        // `key @key.<scope>` per scope referenced by
        // `@cap.Encrypted` / `@cap.E2ee` field sites. Body grammar
        // (`source` / `algorithm` / `rotation`) is doctor-validated;
        // the LSP only needs to recognize the header so warnings
        // don't fire on the children.
        "encryption" => Some("encryption"),
        _ => None,
    }
}

pub(crate) fn registry_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_registry = false;
    let mut current_child: Option<&'static str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration = false;
    let mut current_integration_child: Option<&'static str> = None;
    let mut current_pack = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_registry = trimmed == "registry";
            current_child = None;
            current_env_group = None;
            current_integration = false;
            current_integration_child = None;
            current_pack = false;
            continue;
        }

        if !in_registry {
            continue;
        }

        match leading {
            2 => {
                current_env_group = None;
                current_integration = false;
                current_integration_child = None;
                current_pack = false;
                current_child = match trimmed.split_whitespace().next().unwrap_or_default() {
                    "env" => Some("env"),
                    "capabilities" => Some("capabilities"),
                    "integrations" => Some("integrations"),
                    // B1 (W3-blockers) — `bindings` is registry-level
                    // sugar over `integrations`. Same indent-4
                    // integration header (`<name>: <CapabilityType>`)
                    // and the same canonical indent-6 children, plus
                    // the simplified `endpoint env.X` /
                    // `auth keys env.A env.B` surface. We reuse the
                    // `integrations` sentinel so the existing
                    // header + child validators apply unchanged.
                    "bindings" => Some("integrations"),
                    "packs" => Some("packs"),
                    "tools" => Some("tools"),
                    "webhook_event" => Some("webhook_event"),
                    // Webhooks expanded cycle — registry-side catalog
                    // of expected inbound envelope shapes. Indent-4
                    // entries open new envelopes; indent-6 children
                    // declare typed fields. Validation lives in the
                    // doctor path (`WEBHOOK-PAYLOAD-001` etc.); the
                    // LSP contract diagnostic only suppresses the
                    // unknown-block warning.
                    "webhook_events" => Some("webhook_events"),
                    // Roadmap §1.10 — `secret_rotation <name>` is
                    // a NAMED block at indent-2. Body shape
                    // validated by
                    // `secret_rotation_contract_diagnostics`.
                    "secret_rotation" => Some("secret_rotation"),
                    _ => {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "registry blocks use `env`, `capabilities`, `integrations`, `bindings`, `packs`, `tools`, `webhook_event <name>`, `webhook_events`, or `secret_rotation`.",
                        ));
                        None
                    }
                };
            }
            4 => match current_child {
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                }
                Some("capabilities") => {
                    validate_app_capability_line(&mut diagnostics, line_index, line)
                }
                Some("integrations") => {
                    validate_app_integration_header(&mut diagnostics, line_index, line, trimmed);
                    current_integration = true;
                    current_integration_child = None;
                }
                Some("packs") => {
                    validate_registry_pack_header(&mut diagnostics, line_index, line, trimmed);
                    current_pack = true;
                }
                Some("webhook_event") => {
                    if !(trimmed == "payload"
                        || trimmed.starts_with("version ")
                        || trimmed.starts_with("previous_version ")
                        || trimmed.starts_with("deprecated "))
                    {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "`webhook_event` children use `payload`, `version <n>`, `previous_version <n>`, or `deprecated <bool>`.",
                        ));
                    }
                }
                _ => {}
            },
            6 => {
                if current_child == Some("env") {
                    if current_env_group.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "six-space registry env declarations must follow `group <name>` inside `env`.",
                        ));
                    } else {
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                } else if current_child == Some("integrations") {
                    if !current_integration {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "integration children must follow `<name>: <CapabilityType>` under `integrations`.",
                        ));
                    } else {
                        validate_app_integration_child(
                            &mut diagnostics,
                            &mut current_integration_child,
                            line_index,
                            line,
                            trimmed,
                        );
                    }
                } else if current_child == Some("packs") {
                    if !current_pack {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-pack-contract",
                            "pack children must follow `<name> from <package-or-path>` under `packs`.",
                        ));
                    } else {
                        validate_registry_pack_child(&mut diagnostics, line_index, line, trimmed);
                    }
                } else if current_child == Some("webhook_event") {
                    if !trimmed.contains(':') {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "`webhook_event payload` fields use `<name>: <Type>`.",
                        ));
                    }
                }
            }
            8 => {
                if current_child == Some("integrations")
                    && current_integration_child == Some("credentials")
                {
                    validate_app_integration_credential_line(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "registry-contract",
                        "eight-space registry declarations are only valid inside `integrations credentials`.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "registry-contract",
                "registry declarations use two, four, six, or eight spaces of indentation.",
            )),
        }
    }

    diagnostics
}

pub(crate) fn profile_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_profile = false;
    let mut current_child: Option<&str> = None;
    let mut saw_child = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 0 {
            if in_profile && !saw_child {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index.saturating_sub(1),
                    "profile",
                    DiagnosticSeverity::WARNING,
                    "profile-contract",
                    "profiles should declare at least one `urls`, `bindings`, `integrations`, or `deploy` override.",
                ));
            }

            in_profile = trimmed.starts_with("profile ");
            current_child = None;
            saw_child = false;

            if in_profile {
                match trimmed.split_whitespace().collect::<Vec<_>>().as_slice() {
                    ["profile", name] if is_identifier(name) => {}
                    _ => diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "profile-contract",
                        "profile headers use `profile <environment_name>`.",
                    )),
                }
            }
            continue;
        }

        if !in_profile {
            continue;
        }

        match leading {
            2 => {
                current_child = profile_child_kind(trimmed);
                if current_child.is_some() {
                    saw_child = true;
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "profile-contract",
                        "profile blocks support `urls`, `bindings`, `integrations`, and `deploy` children.",
                    ));
                }
            }
            4 => match current_child {
                Some("urls") => validate_profile_url_line(&mut diagnostics, line_index, line),
                Some("bindings") => {
                    if !is_profile_binding_line(trimmed) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "profile-binding-contract",
                            "profile bindings use `<feature>.<slot> = integrations.<name>` or `registry.integrations.<name>`.",
                        ));
                    }
                }
                Some("integrations") => {
                    validate_profile_integration_line(&mut diagnostics, line_index, line)
                }
                Some("deploy") => validate_profile_deploy_line(&mut diagnostics, line_index, line),
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "profile-contract",
                    "four-space profile declarations must belong to `urls`, `bindings`, `integrations`, or `deploy`.",
                )),
            },
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "profile-contract",
                "profile declarations use two-space sections and four-space override lines.",
            )),
        }
    }

    if in_profile && !saw_child {
        diagnostics.push(simple_canonical_diagnostic(
            source.lines().count().saturating_sub(1),
            "profile",
            DiagnosticSeverity::WARNING,
            "profile-contract",
            "profiles should declare at least one `urls`, `bindings`, `integrations`, or `deploy` override.",
        ));
    }

    diagnostics
}

pub(crate) fn profile_child_kind(trimmed: &str) -> Option<&'static str> {
    match trimmed {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

pub(crate) fn validate_profile_url_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), [target, url] if is_identifier(target) && is_quoted_lzx_literal(url))
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-url-contract",
        "profile URL overrides use `<target> \"https://...\"`, e.g. `web \"https://app.example\"`.",
    ));
}

pub(crate) fn is_profile_binding_line(trimmed: &str) -> bool {
    let Some((target, source)) = trimmed.split_once('=') else {
        return false;
    };
    let Some((feature, slot)) = target.trim().split_once('.') else {
        return false;
    };
    is_identifier(feature)
        && is_identifier(slot)
        && (source
            .trim()
            .strip_prefix("integrations.")
            .is_some_and(is_identifier)
            || source
                .trim()
                .strip_prefix("registry.integrations.")
                .is_some_and(is_identifier))
}

pub(crate) fn validate_profile_integration_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), [name, "environment", environment] if is_identifier(name) && is_identifier(environment))
        || matches!(parts.as_slice(), [name, "adapter", adapter] if is_identifier(name) && adapter_source_provenance(adapter).is_some())
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-integration-contract",
        "profile integration overrides use `<integration> environment sandbox|production` or `<integration> adapter <source>`, where adapter sources are `@runtime/...`, `@lazuli/plugin-publisher/name`, `@adapter.local`, or a local path.",
    ));
}

pub(crate) fn validate_profile_deploy_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["topology", value]
        | ["migrations", value]
        | ["migration_lock", value]
        | ["destructive_migrations", value]
        | ["rollback", value]
            if is_identifier(value) =>
        {
            return;
        }
        _ => {}
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-deploy-contract",
        "profile deploy overrides use `topology`, `migrations`, `migration_lock`, `destructive_migrations`, or `rollback` with an identifier value.",
    ));
}

pub(crate) fn feature_requirements_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature = false;
    let mut in_requires_block = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_feature = trimmed.starts_with("feature ");
            in_requires_block = false;
            continue;
        }

        if !in_feature {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                validate_feature_requirement_line(&mut diagnostics, line_index, line, requirement);
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block && leading == 4 {
            validate_feature_requirement_line(&mut diagnostics, line_index, line, trimmed);
        } else if in_requires_block && leading > 4 {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "feature-requirement-contract",
                "feature requirements use four-space children such as `integration gateway: PaymentGateway`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn validate_feature_requirement_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if parse_feature_integration_requirement(trimmed).is_some() {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "feature-requirement-contract",
        "feature requirements currently use `integration <name>: <CapabilityType>`; bind concrete providers from app/registry.",
    ));
}

pub(crate) fn parse_feature_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.trim().strip_prefix("integration ")?;
    let (name, contract) = rest.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();

    if is_identifier(name) && is_type_name(contract) {
        Some((name, contract))
    } else {
        None
    }
}

pub(crate) fn external_call_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature = false;
    let mut requirement_slots = HashSet::new();
    let mut current_block: Option<ExternalCallBlockFacts> = None;
    let mut current_call_child = false;
    let mut in_requires_block = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 0 {
            if let Some(block) = current_block.take() {
                diagnostics.extend(external_call_block_diagnostics(block));
            }
            in_feature = trimmed.starts_with("feature ");
            requirement_slots.clear();
            current_call_child = false;
            in_requires_block = false;
            continue;
        }

        if !in_feature {
            continue;
        }

        if leading == 2 {
            if let Some(block) = current_block.take() {
                diagnostics.extend(external_call_block_diagnostics(block));
            }

            in_requires_block = trimmed == "requires";
            current_call_child = false;

            if let Some(requirement) = trimmed.strip_prefix("requires ")
                && let Some((slot, _)) = parse_feature_integration_requirement(requirement)
            {
                requirement_slots.insert(slot.to_owned());
            }

            if let Some(name) = command_name_if(trimmed) {
                current_block = Some(ExternalCallBlockFacts::new("command", name, line_index));
            } else if let Some(name) = named_block_name(trimmed, "job") {
                current_block = Some(ExternalCallBlockFacts::new(
                    "job",
                    name.to_owned(),
                    line_index,
                ));
            }
            continue;
        }

        if in_requires_block && leading == 4 {
            if let Some((slot, _)) = parse_feature_integration_requirement(trimmed) {
                requirement_slots.insert(slot.to_owned());
            }
            continue;
        } else if leading <= 2 {
            in_requires_block = false;
        }

        let Some(block) = current_block.as_mut() else {
            continue;
        };

        if leading == 4 {
            current_call_child = false;
            if trimmed.starts_with("timeout ") {
                block.has_timeout = true;
            } else if trimmed.starts_with("retry ") {
                block.has_retry = true;
            } else if trimmed.starts_with("idempotency by ") {
                block.has_idempotency = true;
            } else if let Some((slot, _operation)) = parse_external_call_header(trimmed) {
                block.calls.push(ExternalCallLine {
                    line_index,
                    line: line.to_owned(),
                });
                if !requirement_slots.contains(slot) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "external-call-requirement",
                        "`calls <slot>.<operation>` should use a slot declared by `requires integration <slot>: <Contract>`.",
                    ));
                }
                current_call_child = true;
            } else if trimmed.starts_with("calls ") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "external-call-shape",
                    "external calls use `calls <integration_slot>.<operation>`.",
                ));
                current_call_child = true;
            }
        } else if leading == 6 && current_call_child && !trimmed.contains('=') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "external-call-arg",
                "external call children use `name = expression` argument bindings.",
            ));
        }
    }

    if let Some(block) = current_block {
        diagnostics.extend(external_call_block_diagnostics(block));
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct ExternalCallBlockFacts {
    kind: &'static str,
    name: String,
    line_index: usize,
    calls: Vec<ExternalCallLine>,
    has_timeout: bool,
    has_retry: bool,
    has_idempotency: bool,
}

impl ExternalCallBlockFacts {
    fn new(kind: &'static str, name: String, line_index: usize) -> Self {
        Self {
            kind,
            name,
            line_index,
            calls: Vec::new(),
            has_timeout: false,
            has_retry: false,
            has_idempotency: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ExternalCallLine {
    line_index: usize,
    line: String,
}

pub(crate) fn external_call_block_diagnostics(block: ExternalCallBlockFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if block.calls.is_empty() {
        return diagnostics;
    }

    if !block.has_timeout {
        diagnostics.push(simple_canonical_diagnostic(
            block.line_index,
            &format!("{} {}", block.kind, block.name),
            DiagnosticSeverity::WARNING,
            "external-call-timeout",
            "`calls <slot>.<operation>` should be paired with an explicit `timeout \"...\"` on the same command/job block.",
        ));
    }

    if !block.has_retry {
        for call in &block.calls {
            diagnostics.push(simple_canonical_diagnostic(
                call.line_index,
                &call.line,
                DiagnosticSeverity::WARNING,
                "external-call-retry",
                "`calls <slot>.<operation>` should have a visible `retry <count> backoff <strategy>` policy or a future explicit no-retry marker.",
            ));
        }
    }

    if block.kind == "job" && !block.has_idempotency {
        for call in &block.calls {
            diagnostics.push(simple_canonical_diagnostic(
                call.line_index,
                &call.line,
                DiagnosticSeverity::WARNING,
                "external-call-idempotency",
                "jobs with external calls should declare `idempotency by ...` so retries cannot duplicate side effects silently.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}

pub(crate) fn command_name_if(trimmed: &str) -> Option<String> {
    named_block_name(trimmed, "command").map(str::to_owned)
}

pub(crate) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

pub(crate) fn is_app_scalar_child(trimmed: &str) -> bool {
    matches!(
        trimmed.split_whitespace().next(),
        Some(
            "title"
                | "version"
                // ABI pin enforced by doctor LAZULI-VERSION-001;
                // accept here so the LSP doesn't redundantly warn
                // that `lazuli_version "0.15"` isn't a recognized
                // app block.
                | "lazuli_version"
                | "default_locale"
                | "default_timezone"
                | "auth_failed_redirect"
                | "actor_query"
                | "not_found"
        )
    )
}

pub(crate) fn validate_app_child_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    if matches!(
        first,
        "targets"
            | "bindings"
            | "packs"
            | "environments"
            | "urls"
            | "env"
            | "integrations"
            | "capabilities"
            | "architecture"
            | "services"
            | "communication"
            | "runtime"
            | "route_guard"
            | "deploy"
    ) && trimmed != first
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "multi-line app manifest blocks use a bare block header, with entries nested below it.",
        ));
    }
}

pub(crate) fn validate_app_scalar_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app scalar declarations need a value.",
        ));
    }
}

pub(crate) fn validate_app_architecture_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["mode", value]
            if matches!(*value, "monolith" | "modular_monolith" | "microservices") => {}
        ["service_ready", value] | ["enforce_service_boundaries", value]
            if matches!(*value, "true" | "false") => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-architecture-contract",
            "architecture lines use `mode monolith|modular_monolith|microservices`, `service_ready true|false`, or `enforce_service_boundaries true|false`.",
        )),
    }
}

pub(crate) fn validate_app_communication_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["internal", "sync", value] if matches!(*value, "rpc" | "http" | "in_process") => {}
        ["external", value] if matches!(*value, "http") => {}
        ["async", value] if matches!(*value, "event_bus" | "in_process") => {}
        ["propagate", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" ")).iter().all(|item| {
                    matches!(
                        item.as_str(),
                        "actor" | "tenant" | "trace_id" | "request_id" | "locale"
                    )
                }) => {}
        ["timeout", "default", value] if is_quoted_lzx_literal(value) => {}
        ["retry", "default", count, "backoff", strategy]
            if count.parse::<u32>().is_ok() && matches!(*strategy, "fixed" | "exponential") => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-communication-contract",
            "communication lines use `internal sync rpc|http|in_process`, `external http`, `async event_bus|in_process`, `propagate ...`, `timeout default \"...\"`, or `retry default <n> backoff fixed|exponential`.",
        )),
    }
}

pub(crate) fn validate_app_service_child(
    diagnostics: &mut Vec<Diagnostic>,
    service: &mut AppServiceFacts,
    current_service_child: &mut Option<&'static str>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if let Some(rest) = trimmed.strip_prefix("owns ") {
        service.has_owns = true;
        *current_service_child = None;
        if split_items(rest).is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                "service ownership uses `owns feature_a, feature_b`.",
            ));
        }
        return;
    }

    if trimmed == "exposes" {
        *current_service_child = Some("exposes");
        return;
    }

    if let Some(rest) = trimmed
        .strip_prefix("publishes ")
        .or_else(|| trimmed.strip_prefix("consumes "))
    {
        *current_service_child = None;
        if split_items(rest).is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                "service event edges use `publishes event.*` or `consumes feature.event_name`.",
            ));
        }
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "app-service-contract",
        "service children use `owns ...`, `exposes`, `publishes ...`, or `consumes ...`.",
    ));
}

pub(crate) fn validate_app_service_exposure_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2
        || !matches!(
            parts[0],
            "query" | "command" | "api" | "workflow" | "report"
        )
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-service-contract",
            "service exposures use `query|command|api|workflow|report <feature>.<kind>.<name>`.",
        ));
    }
}

pub(crate) fn validate_app_target_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2 || !matches!(parts[0], "backend" | "web" | "mobile") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "app-target-contract",
            "app targets use `backend <runtime>`, `web <runtime>`, or `mobile <runtime>`.",
        ));
    }
}

pub(crate) fn validate_app_url_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 3 || !matches!(parts[0], "web" | "api" | "mobile") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "app URLs use `<web|api|mobile> <environment> \"https://...\"`.",
        ));
        return;
    }

    let url = unquote_lzx_literal(parts[2]);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "app URLs should be absolute HTTP(S) URLs so generated clients, CORS, emails, and callbacks agree.",
        ));
    }

    if parts[1] != "local" && url.starts_with("http://") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "non-local app URLs should use HTTPS.",
        ));
    }
}

pub(crate) fn validate_app_binding_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    if parse_app_binding_line(trimmed).is_none() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-binding-contract",
            "app bindings use `<feature>.<slot> = integrations.<name>` or `<feature>.<slot> = registry.integrations.<name>`.",
        ));
    }
}

pub(crate) fn validate_app_pack_use_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let Some((name, source)) = trimmed.split_once(" from ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-pack-contract",
            "app pack entries use `<alias> from registry.packs.<name>` or `<alias> from packs.<name>`.",
        ));
        return;
    };

    let source_name = source
        .trim()
        .strip_prefix("packs.")
        .or_else(|| source.trim().strip_prefix("registry.packs."));
    if !is_identifier(name.trim()) || !source_name.is_some_and(is_identifier) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-pack-contract",
            "app pack entries use identifier aliases and `packs.<name>` or `registry.packs.<name>` sources.",
        ));
    }
}

pub(crate) fn validate_registry_pack_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let Some((name, source)) = trimmed.split_once(" from ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "registry-pack-contract",
            "registry packs use `<name> from @scope/package` or a local path.",
        ));
        return;
    };

    let source = source.trim();
    let valid_source = source.starts_with('@')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || is_quoted_lzx_literal(source);

    if !is_identifier(name.trim()) || !valid_source {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "registry-pack-contract",
            "registry pack entries use identifier names and package/path sources such as `payments from @runtime/payments`.",
        ));
    }
}

pub(crate) fn validate_registry_pack_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if let Some(version) = trimmed.strip_prefix("version ") {
        if is_quoted_lzx_literal(version.trim()) {
            return;
        }
    }

    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), ["provides", kind, name] if is_identifier(kind) && is_identifier(name))
    {
        return;
    }

    if let Some(requirement) = trimmed.strip_prefix("requires ")
        && parse_feature_integration_requirement(requirement).is_some()
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "registry-pack-contract",
        "pack children use `version \"...\"`, `provides feature <name>`, or `requires integration <slot>: <CapabilityType>`.",
    ));
}

pub(crate) fn parse_app_binding_line(trimmed: &str) -> Option<(&str, &str, &str)> {
    let (target, source) = trimmed.split_once('=')?;
    let target = target.trim();
    let source = source.trim();
    let (feature, slot) = target.split_once('.')?;
    let source_name = source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))?;

    if is_identifier(feature) && is_identifier(slot) && is_identifier(source_name) {
        Some((feature, slot, source_name))
    } else {
        None
    }
}

pub(crate) fn validate_app_env_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !valid_env_declaration_parts(&parts) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "app-env-contract",
            "app env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`.",
        ));
        return;
    }

    let name = parts[1].trim_end_matches(':');
    if parts[0] == "client" && !has_public_token(name) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "env-client-exposure",
            "client env names should contain a `PUBLIC` token (e.g. `PUBLIC_MERCADOPAGO_KEY` or vendor-style `MERCADOPAGO_PUBLIC_KEY`) so secret/server-only values are not accidentally bundled.",
        ));
    }

    if parts[0] == "mobile" && !name.starts_with("EXPO_PUBLIC_") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "env-mobile-exposure",
            "mobile env names should use an `EXPO_PUBLIC_` prefix so Expo-visible values are explicit.",
        ));
    }
}

/// Closes WAR-DOCTOR-ENV-01 false-positive. `PUBLIC` may appear as
/// the leading token (`PUBLIC_API_KEY`) OR as a mid-name token
/// (`MERCADOPAGO_PUBLIC_KEY`, `STRIPE_PUBLIC_KEY`). Vendor SDKs
/// frequently impose the latter shape because their key names are
/// fixed by the upstream service. As long as `PUBLIC` shows up as a
/// `_`-delimited token, the author has signalled intent to expose.
pub(crate) fn has_public_token(name: &str) -> bool {
    name.split('_').any(|token| token == "PUBLIC")
}

pub(crate) fn valid_env_declaration_parts(parts: &[&str]) -> bool {
    let has_environment_scope = parts.len() >= 6
        && parts[4] == "in"
        && split_items(&parts[5..].join(" "))
            .iter()
            .all(|environment| is_identifier(environment));

    (parts.len() == 4 || has_environment_scope)
        && matches!(parts[0], "server" | "client" | "mobile")
        && parts[1].ends_with(':')
        && matches!(parts[2], "Secret" | "Text" | "Url" | "Boolean" | "Integer")
        && matches!(parts[3], "required" | "optional")
}

pub(crate) fn parse_env_group_name(trimmed: &str) -> Option<&str> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "group" && is_identifier(parts[1]) {
        Some(parts[1])
    } else {
        None
    }
}

pub(crate) fn validate_app_integration_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if parse_app_integration_header(trimmed).is_none() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integrations use `<name>: <CapabilityType>` such as `crm: CRMProvider`; provider details stay in adapters.",
        ));
    }
}

pub(crate) fn parse_app_integration_header(trimmed: &str) -> Option<(&str, &str)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name, kind))
    } else {
        None
    }
}

pub(crate) fn validate_app_integration_child(
    diagnostics: &mut Vec<Diagnostic>,
    current_integration_child: &mut Option<&'static str>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["adapter", adapter] if adapter_source_provenance(adapter).is_some() => {
            *current_integration_child = None;
        }
        ["environments", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" "))
                    .iter()
                    .all(|environment| is_identifier(environment)) =>
        {
            *current_integration_child = None;
        }
        ["credentials", scope] if matches!(*scope, "platform" | "tenant" | "actor") => {
            *current_integration_child = Some("credentials");
        }
        ["data_classification", classification] if classification.starts_with("@pii.") => {
            *current_integration_child = None;
        }
        // B1 (W3-blockers) — `bindings` registry sugar accepted at the
        // same indent-6 site as the canonical integration children.
        // `endpoint <source>` lowers to a single credential binding;
        // `auth keys <id-source> <secret-source>` lowers to the two
        // positional S3-style credential bindings. Both lines reuse
        // the existing `parse_credential_binding`-shaped source grammar
        // (env.X / secrets.X / literal).
        ["endpoint", source] if !source.is_empty() => {
            *current_integration_child = None;
        }
        ["auth", "keys", id_source, secret_source]
            if !id_source.is_empty() && !secret_source.is_empty() =>
        {
            *current_integration_child = None;
        }
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration children use `adapter @runtime/...`, `adapter @lazuli/plugin-publisher/name`, `adapter @adapter.<local>`, local adapter paths, `environments ...`, `credentials platform|tenant|actor`, `endpoint env.<NAME>`, `auth keys env.<ID> env.<SECRET>`, or `data_classification @pii.<class>`.",
        )),
    }
}

pub(crate) fn adapter_source_provenance(source: &str) -> Option<&'static str> {
    if source
        .strip_prefix("@runtime/")
        .is_some_and(valid_pathish_tail)
    {
        Some("runtime")
    } else if source
        .strip_prefix("@lazuli/plugin-")
        .is_some_and(valid_plugin_tail)
    {
        Some("plugin")
    } else if source.strip_prefix("@adapter.").is_some_and(is_identifier)
        || source.starts_with("./")
        || source.starts_with("../")
        || is_quoted_lzx_literal(source)
    {
        Some("local")
    } else {
        None
    }
}

pub(crate) fn valid_plugin_tail(value: &str) -> bool {
    // Mirror `app_manifest::valid_plugin_tail` — accept single-segment
    // (`@lazuli/plugin-<name>`) as well as multi-segment (`@lazuli/plugin-<publisher>/<name>`).
    // All currently-shipped Lazuli plugins use the single-segment convention.
    let segments: Vec<&str> = value.split('/').filter(|p| !p.is_empty()).collect();
    !segments.is_empty() && segments.iter().all(|s| valid_path_segment(s))
}

pub(crate) fn valid_pathish_tail(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_path_segment)
}

pub(crate) fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

pub(crate) fn validate_app_integration_credential_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let mut parts = trimmed.split_whitespace();
    let Some(name) = parts.next() else {
        return;
    };
    let source = parts.collect::<Vec<_>>().join(" ");
    if !is_identifier(name) || source.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration credentials use `<credential_name> <source>`, for example `access_token env.MERCADOPAGO_ACCESS_TOKEN`.",
        ));
    }
}

pub(crate) fn validate_app_capability_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2
        || !matches!(
            parts[0],
            "database"
                | "queue"
                | "object_storage"
                | "mailer"
                | "event_bus"
                | "tracing"
                | "search"
                | "cache"
                | "storage"
                | "secret_provider"
                | "integration"
                | "secret_provider"
                | "payment_gateway"
                | "credit_bureau"
        )
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-capability-contract",
            "app capabilities declare intent such as `database postgres`, `queue background_jobs`, `object_storage files`, or `integration crm`; providers stay in adapters under `@runtime/<name>`.",
        ));
    }
}

pub(crate) fn validate_app_deploy_line(
    diagnostics: &mut Vec<Diagnostic>,
    app: &mut AppOperationalFacts,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["migrations", value] if matches!(*value, "before_deploy" | "manual" | "disabled") => {
            app.deploy_has_migrations = true;
        }
        ["migration_lock", value] if matches!(*value, "required" | "optional") => {}
        ["destructive_migrations", value]
            if matches!(*value, "require_approval" | "forbidden" | "manual") => {}
        ["rollback", value]
            if matches!(*value, "on_failed_healthcheck" | "manual" | "disabled") =>
        {
            app.deploy_has_rollback = true;
        }
        // Migrations bucket cycle Route C — five new deploy children.
        // `strategy` catalog enforced downstream by `DEPLOY-STRATEGY-001`.
        ["strategy", value]
            if matches!(*value, "rolling" | "blue_green" | "canary") => {}
        ["lock_timeout", _value] => {}
        ["pre_migration_hook", _value] => {}
        ["post_migration_hook", _value] => {}
        ["checkpoint", _name, _path] => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-deploy-contract",
            "deploy contracts use `migrations before_deploy|manual|disabled`, `migration_lock required|optional`, `destructive_migrations require_approval|forbidden`, `rollback on_failed_healthcheck|manual|disabled`, `strategy rolling|blue_green|canary`, `lock_timeout \"<duration>\"`, `pre_migration_hook \"<path>\"`, `post_migration_hook \"<path>\"`, and `checkpoint <name> \"<path>\"`.",
        )),
    }
}

pub(crate) fn validate_app_runtime_unit_child(
    diagnostics: &mut Vec<Diagnostic>,
    unit: &mut AppRuntimeUnitFacts,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if trimmed.starts_with("serves ") || trimmed.starts_with("runs ") {
        unit.has_serves_or_runs = true;
        return;
    }

    if let Some(path) = trimmed
        .strip_prefix("healthcheck ")
        .or_else(|| trimmed.strip_prefix("readiness "))
    {
        unit.has_healthcheck_or_readiness = true;
        let path = unquote_lzx_literal(path.trim());
        if !path.starts_with('/') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "runtime healthcheck/readiness paths should be absolute paths such as `\"/healthz\"`.",
            ));
        }
        return;
    }

    // i18n bucket cycle — `locale_negotiate` opens a child block whose
    // entries land at indent 8. The LSP file-local rule accepts the
    // header; doctor validates the body via the IR
    // (`locale_negotiate_source_invalid`, `_strategy_invalid`,
    // `app_locale_fallback_unknown_dest`).
    if trimmed == "locale_negotiate" {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "app-runtime-contract",
        "runtime unit children use `serves ...`, `runs ...`, `healthcheck \"...\"`, `readiness \"...\"`, or `locale_negotiate`.",
    ));
}

pub(crate) fn app_operational_block_diagnostics(app: AppOperationalFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !app.has_uses {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app manifests should declare `uses` so the entrypoint owns feature registration explicitly.",
        ));
    }

    if !app.has_targets {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::ERROR,
            "app-target-contract",
            "app manifests must declare `targets` so the runtime can materialize backend, web, and mobile outputs deterministically.",
        ));
    }

    if !app.has_environments {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app manifests should declare `environments` so env, URLs, deploy gates, and runtime safety can be checked per environment.",
        ));
    }

    if app.has_services && !app.has_architecture {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-architecture-contract",
            "apps with `services` should declare `architecture` so boundaries are separated from deploy topology.",
        ));
    }

    if app.has_services && !app.has_communication {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-communication-contract",
            "apps with `services` should declare `communication` context propagation and sync/async intent.",
        ));
    }

    if !app.has_runtime {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-runtime-contract",
            "app manifests should declare `runtime` units such as `api`, `web`, `worker`, and `scheduler`.",
        ));
    } else if app.runtime_units.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-runtime-contract",
            "`runtime` should declare at least one `unit <name>`.",
        ));
    }

    if !app.has_deploy {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-deploy-contract",
            "app manifests should declare `deploy` gates for migrations, rollback, and destructive changes without becoming provider-specific infra.",
        ));
    } else {
        if !app.deploy_has_migrations {
            diagnostics.push(simple_canonical_diagnostic(
                app.line_index,
                &app.line,
                DiagnosticSeverity::WARNING,
                "app-deploy-contract",
                "deploy contracts should declare a migrations policy.",
            ));
        }
        if !app.deploy_has_rollback {
            diagnostics.push(simple_canonical_diagnostic(
                app.line_index,
                &app.line,
                DiagnosticSeverity::WARNING,
                "app-deploy-contract",
                "deploy contracts should declare rollback behavior.",
            ));
        }
    }

    for unit in app.runtime_units {
        if !unit.has_serves_or_runs {
            diagnostics.push(simple_canonical_diagnostic(
                unit.line_index,
                &unit.line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "runtime units should declare what they `serves` or `runs`.",
            ));
        }
        if unit.name == "api" && !unit.has_healthcheck_or_readiness {
            diagnostics.push(simple_canonical_diagnostic(
                unit.line_index,
                &unit.line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "the `api` runtime unit should declare `healthcheck` and/or `readiness` paths for deploy safety.",
            ));
        }
    }

    for service in app.services {
        if !service.has_owns {
            diagnostics.push(simple_canonical_diagnostic(
                service.line_index,
                &service.line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                &format!(
                    "service `{}` should declare `owns ...` so feature ownership is explicit.",
                    service.name
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn env_top_level_legacy_diagnostics(source: &str) -> Vec<Diagnostic> {
    // Warn when an `env` block lives at indent 0 in a `.lzi` source that
    // also declares `feature` or `app`. The canonical home for env schema
    // is `registry.lzi`; top-level `env` here is a legacy duplicate.
    let mut diagnostics = Vec::new();
    let mut env_at_top: Option<(usize, String)> = None;
    let mut has_feature_or_app = false;
    let mut has_registry = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) != 0 {
            continue;
        }

        if trimmed == "env" {
            env_at_top.get_or_insert((line_index, line.to_owned()));
        } else if trimmed.starts_with("feature ") || trimmed.starts_with("app ") {
            has_feature_or_app = true;
        } else if trimmed == "registry" || trimmed.starts_with("registry ") {
            has_registry = true;
        }
    }

    if let Some((line_index, line)) = env_at_top {
        if has_feature_or_app && !has_registry {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "env-top-level-legacy",
                "top-level `env` blocks in `.lzi` feature/app sources are legacy. Move env schema to `registry.lzi` (or `registry.env` inside the same package) so the declaration has a single source of truth.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn env_schema_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut declared = HashSet::new();
    let mut env_indent: Option<usize> = None;
    let mut current_env_group: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            env_indent = if trimmed == "env" { Some(0) } else { None };
            current_env_group = None;
            continue;
        }

        if leading == 2 && trimmed == "env" {
            env_indent = Some(2);
            current_env_group = None;
            continue;
        }

        let Some(base_indent) = env_indent else {
            continue;
        };

        if leading <= base_indent {
            env_indent = None;
            current_env_group = None;
            continue;
        }

        if leading == base_indent + 2 {
            if let Some(group) = parse_env_group_name(trimmed) {
                current_env_group = Some(group.to_owned());
                continue;
            }
            current_env_group = None;
        } else if leading == base_indent + 4 && current_env_group.is_some() {
        } else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "env-schema-contract",
                "env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`, optionally nested under `group <name>`.",
            ));
            continue;
        }

        let parts: Vec<_> = trimmed.split_whitespace().collect();
        if !valid_env_declaration_parts(&parts) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "env-schema-contract",
                "env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`, optionally nested under `group <name>`.",
            ));
            continue;
        }

        let name = parts[1].trim_end_matches(':');
        declared.insert(name.to_owned());

        if parts[0] == "client" && !has_public_token(name) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "env-client-exposure",
                "client env names should contain a `PUBLIC` token (e.g. `PUBLIC_MERCADOPAGO_KEY` or vendor-style `MERCADOPAGO_PUBLIC_KEY`) so secret/server-only values are not accidentally bundled.",
            ));
        }

        if parts[0] == "mobile" && !name.starts_with("EXPO_PUBLIC_") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "env-mobile-exposure",
                "mobile env names should use an `EXPO_PUBLIC_` prefix so Expo-visible values are explicit.",
            ));
        }
    }

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        for reference in path_references(trimmed, "env.") {
            if !declared.contains(reference) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "env-schema-reference",
                    &format!(
                        "environment reference `env.{reference}` should be declared in `registry.env` with scope, type, and requiredness. Doctor cross-checks against the package registry; this LSP rule only sees the current file.",
                    ),
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn collect_field_policy_facts(source: &str) -> HashMap<(String, String, String), FieldPolicyFacts> {
    let mut policies = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_policy_resource: Option<String> = None;
    let mut current_policy_field: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_policy_resource = None;
                current_policy_field = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_policy_resource = None;
                current_policy_field = None;
            }
            4 if current_top == Some("policies") => {
                current_policy_resource = trimmed
                    .strip_prefix("fields ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                current_policy_field = None;
            }
            6 if current_top == Some("policies") && current_policy_resource.is_some() => {
                current_policy_field = Some(trimmed.to_owned());
            }
            8 if current_top == Some("policies") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource) = current_policy_resource.as_deref() else {
                    continue;
                };
                let Some(field) = current_policy_field.as_deref() else {
                    continue;
                };
                let entry = policies
                    .entry((feature.to_owned(), resource.to_owned(), field.to_owned()))
                    .or_insert_with(FieldPolicyFacts::default);
                if trimmed.starts_with("read:") {
                    entry.read = true;
                } else if trimmed.starts_with("write:") {
                    entry.write = true;
                }
            }
            _ => {}
        }
    }

    policies
}



pub(crate) fn apply_security_profile(
    mut diagnostics: Vec<Diagnostic>,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    for diagnostic in &mut diagnostics {
        let Some(code) = diagnostic_code(diagnostic) else {
            continue;
        };

        if is_security_enforcement_code(code) {
            diagnostic.severity = Some(match security_profile {
                SecurityProfile::Prototype => DiagnosticSeverity::WARNING,
                SecurityProfile::Strict | SecurityProfile::Production => DiagnosticSeverity::ERROR,
            });
        } else if security_profile == SecurityProfile::Production && is_security_opt_out_code(code)
        {
            diagnostic.severity = Some(DiagnosticSeverity::ERROR);
        }
    }

    diagnostics
}

pub(crate) fn diagnostic_code(diagnostic: &Diagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        tower_lsp::lsp_types::NumberOrString::String(code) => Some(code.as_str()),
        tower_lsp::lsp_types::NumberOrString::Number(_) => None,
    }
}

pub(crate) fn is_security_enforcement_code(code: &str) -> bool {
    matches!(
        code,
        "command-policy"
            | "command-rate-limit"
            | "scope-override-policy"
            | "scope-override-reason"
            | "field-security-policy"
            | "webhook-verify"
            | "webhook-idempotency"
            | "event-job-tenant-from"
            | "event-consumer-payload"
            | "crypto-tier"
            | "crypto-hash-algorithm"
            | "crypto-key-scope"
            | "crypto-token-contract"
            | "crypto-capability-arguments"
            | "escape-route-security"
            | "auth-password-algorithm"
            | "auth-password-rate-limit"
            | "auth-session-ttl"
            | "auth_password_algorithm_hash_mismatch"
            | "auth_sessions_resource_unknown"
            | "auth_identity_field_unknown"
            | "auth_oauth_adapter_unbound"
            | "security-opt-out-reason"
    )
}

pub(crate) fn is_security_opt_out_code(code: &str) -> bool {
    matches!(code, "security-opt-out")
}

pub(crate) fn simple_canonical_diagnostic(
    line_index: usize,
    line: &str,
    severity: DiagnosticSeverity,
    code: &str,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: line_index as u32,
                character: leading_spaces(line) as u32,
            },
            end: Position {
                line: line_index as u32,
                character: line.len().max(leading_spaces(line) + 1) as u32,
            },
        },
        severity: Some(severity),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            code.to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: message.to_owned(),
        related_information: None,
        tags: None,
        data: None,
    }
}

#[derive(Debug, Default)]
pub(crate) struct CanonicalFeatureFacts {
    default_tenancy: Option<String>,
    default_timestamps: bool,
    resources: HashMap<String, CanonicalResourceFacts>,
}

#[derive(Debug)]
pub(crate) struct CanonicalResourceFacts {
    fields: HashSet<String>,
    tenancy_field: Option<String>,
}

impl CanonicalResourceFacts {
    fn new(default_tenancy: Option<&str>, default_timestamps: bool) -> Self {
        let mut facts = Self {
            fields: HashSet::from(["id".to_owned()]),
            tenancy_field: None,
        };

        if let Some(tenancy) = default_tenancy {
            facts.set_tenancy(tenancy);
        }

        if default_timestamps {
            facts.set_timestamps(true);
        }

        facts
    }

    fn set_tenancy(&mut self, tenancy: &str) {
        if let Some(previous) = self.tenancy_field.take() {
            self.fields.remove(&previous);
        }

        if tenancy != "none" {
            self.fields.insert(tenancy.to_owned());
            self.tenancy_field = Some(tenancy.to_owned());
        }
    }

    fn set_timestamps(&mut self, enabled: bool) {
        if enabled {
            self.fields.insert("created_at".to_owned());
            self.fields.insert("updated_at".to_owned());
        } else {
            self.fields.remove("created_at");
            self.fields.remove("updated_at");
        }
    }
}

pub(crate) fn event_payload_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
    let features = collect_canonical_feature_facts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_events_resource: Option<String> = None;
    let mut in_payload = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_events_resource = None;
                in_payload = false;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_events_resource = None;
                in_payload = false;
            }
            4 if current_top == Some("domain") => {
                current_events_resource = events_resource_name(trimmed).map(str::to_owned);
                in_payload = false;
            }
            6 if current_top == Some("domain") && current_events_resource.is_some() => {
                in_payload = trimmed == "payload";
            }
            8 if current_top == Some("domain") && in_payload => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource_name) = current_events_resource.as_deref() else {
                    continue;
                };
                let Some(rhs) = payload_assignment_rhs(trimmed) else {
                    continue;
                };
                let Some(field_name) = resource_field_reference(rhs) else {
                    continue;
                };
                let Some(resource) = features
                    .get(feature_name)
                    .and_then(|feature| feature.resources.get(resource_name))
                else {
                    continue;
                };

                if !resource.fields.contains(field_name) {
                    diagnostics.push(event_payload_reference_diagnostic(
                        line_index,
                        line,
                        resource_name,
                        field_name,
                    ));
                }
            }
            _ => {}
        }
    }

    diagnostics
}

#[derive(Debug, Default)]
pub(crate) struct EventPayloadGroup {
    prefix: String,
    fields: HashSet<String>,
}

#[derive(Debug)]
pub(crate) struct JobPayloadReference {
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug)]
pub(crate) struct EventTriggeredJobFacts {
    feature: String,
    trigger: Option<String>,
    payload_references: Vec<JobPayloadReference>,
}

pub(crate) fn event_consumer_payload_diagnostics(source: &str) -> Vec<Diagnostic> {
    let contracts = collect_event_contracts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_job: Option<EventTriggeredJobFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            current_job = current_feature
                .as_ref()
                .map(|feature| EventTriggeredJobFacts {
                    feature: feature.clone(),
                    trigger: None,
                    payload_references: Vec::new(),
                });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            continue;
        }

        let Some(job) = current_job.as_mut() else {
            continue;
        };

        if let Some(event_ref) = trimmed.strip_prefix("trigger event ") {
            let event_ref = event_ref.split_whitespace().next().unwrap_or(event_ref);
            job.trigger = Some(event_ref.to_owned());
        }

        for field in payload_field_references(line) {
            job.payload_references.push(JobPayloadReference {
                field,
                line_index,
                line: line.to_owned(),
            });
        }
    }

    if let Some(job) = current_job {
        diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
    }

    diagnostics
}

pub(crate) fn event_triggered_job_payload_diagnostics(
    job: EventTriggeredJobFacts,
    contracts: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    let Some(trigger) = job.trigger else {
        return Vec::new();
    };
    let event_ref = if trigger.contains('.') {
        trigger
    } else {
        format!("{}.{}", job.feature, trigger)
    };
    let Some(contract) = contracts.get(&event_ref) else {
        return Vec::new();
    };

    job.payload_references
        .into_iter()
        .filter(|reference| !contract.contains(&reference.field))
        .map(|reference| {
            event_consumer_payload_diagnostic(
                reference.line_index,
                &reference.line,
                &event_ref,
                &reference.field,
            )
        })
        .collect()
}

#[derive(Debug)]
pub(crate) struct EventJobTenantFacts {
    feature: String,
    line_index: usize,
    line: String,
    trigger: Option<String>,
    tenant_from: Option<String>,
}

pub(crate) fn event_job_tenant_from_diagnostics(source: &str) -> Vec<Diagnostic> {
    let contracts = collect_event_contracts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_job: Option<EventJobTenantFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_job_tenant_from_diagnostic(job, &contracts));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_job_tenant_from_diagnostic(job, &contracts));
            }
            current_job = current_feature.as_ref().map(|feature| EventJobTenantFacts {
                feature: feature.clone(),
                line_index,
                line: line.to_owned(),
                trigger: None,
                tenant_from: None,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_job_tenant_from_diagnostic(job, &contracts));
            }
            continue;
        }

        let Some(job) = current_job.as_mut() else {
            continue;
        };

        if let Some(event_ref) = trimmed.strip_prefix("trigger event ") {
            let event_ref = event_ref.split_whitespace().next().unwrap_or(event_ref);
            job.trigger = Some(event_ref.to_owned());
        } else if let Some(tenant_from) = trimmed.strip_prefix("tenant_from ") {
            job.tenant_from = Some(tenant_from.trim().to_owned());
        }
    }

    if let Some(job) = current_job {
        diagnostics.extend(event_job_tenant_from_diagnostic(job, &contracts));
    }

    diagnostics
}

pub(crate) fn event_job_tenant_from_diagnostic(
    job: EventJobTenantFacts,
    contracts: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    let Some(trigger) = job.trigger else {
        return Vec::new();
    };
    let event_ref = if trigger.contains('.') {
        trigger
    } else {
        format!("{}.{}", job.feature, trigger)
    };
    let Some(contract) = contracts.get(&event_ref) else {
        return Vec::new();
    };

    if contract.contains("org_id") && job.tenant_from.is_none() {
        vec![simple_canonical_diagnostic(
            job.line_index,
            &job.line,
            DiagnosticSeverity::WARNING,
            "event-job-tenant-from",
            &format!(
                "event `{event_ref}` declares `org_id`; event-triggered jobs should declare `tenant_from payload.org_id` so generated handlers run with a fixed tenant context."
            ),
        )]
    } else {
        Vec::new()
    }
}

#[derive(Debug)]
pub(crate) struct ScheduledJobFacts {
    feature: String,
    line_index: usize,
    line: String,
    is_scheduled: bool,
    has_tenant_fanout: bool,
    has_global_scope: bool,
}

pub(crate) fn scheduled_job_tenancy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let tenant_axes = collect_feature_tenant_axes(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_job: Option<ScheduledJobFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(scheduled_job_tenancy_facts_diagnostics(job, &tenant_axes));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(scheduled_job_tenancy_facts_diagnostics(job, &tenant_axes));
            }
            current_job = current_feature.as_ref().map(|feature| ScheduledJobFacts {
                feature: feature.clone(),
                line_index,
                line: line.to_owned(),
                is_scheduled: false,
                has_tenant_fanout: false,
                has_global_scope: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(job) = current_job.take() {
                diagnostics.extend(scheduled_job_tenancy_facts_diagnostics(job, &tenant_axes));
            }
            continue;
        }

        let Some(job) = current_job.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("trigger schedule ") {
                job.is_scheduled = true;
            } else if trimmed.starts_with("fanout tenants ") {
                job.has_tenant_fanout = true;
            } else if trimmed.starts_with("scope global") {
                job.has_global_scope = true;
            }
        }
    }

    if let Some(job) = current_job {
        diagnostics.extend(scheduled_job_tenancy_facts_diagnostics(job, &tenant_axes));
    }

    diagnostics
}

pub(crate) fn scheduled_job_tenancy_facts_diagnostics(
    job: ScheduledJobFacts,
    tenant_axes: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    if !job.is_scheduled || job.has_tenant_fanout || job.has_global_scope {
        return Vec::new();
    }

    let Some(axes) = tenant_axes
        .get(&job.feature)
        .filter(|axes| !axes.is_empty())
    else {
        return Vec::new();
    };
    let mut axes: Vec<_> = axes.iter().cloned().collect();
    axes.sort();

    vec![simple_canonical_diagnostic(
        job.line_index,
        &job.line,
        DiagnosticSeverity::WARNING,
        "scheduled-job-tenancy",
        &format!(
            "scheduled job in tenant-scoped feature `{}` should declare `fanout tenants {}` or explicit `scope global` with a reason.",
            job.feature,
            axes.join(", ")
        ),
    )]
}

pub(crate) fn collect_feature_tenant_axes(source: &str) -> HashMap<String, HashSet<String>> {
    let mut axes: HashMap<String, HashSet<String>> = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut in_resource = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                let feature = feature_name(trimmed);
                axes.entry(feature.clone()).or_default();
                current_feature = Some(feature);
                current_top = None;
                in_resource = false;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                in_resource = false;
            }
            4 if current_top == Some("domain") => {
                in_resource = trimmed.starts_with("resource ");
            }
            4 if current_top == Some("defaults") => {
                if let Some(axis) = trimmed.strip_prefix("tenancy ") {
                    insert_tenant_axis(&mut axes, current_feature.as_deref(), axis);
                }
            }
            6 if current_top == Some("domain") && in_resource => {
                if let Some(axis) = trimmed.strip_prefix("tenancy ") {
                    insert_tenant_axis(&mut axes, current_feature.as_deref(), axis);
                }
            }
            _ => {}
        }
    }

    axes
}

pub(crate) fn insert_tenant_axis(
    axes: &mut HashMap<String, HashSet<String>>,
    feature: Option<&str>,
    axis: &str,
) {
    let axis = axis.split_whitespace().next().unwrap_or(axis);
    if axis != "none"
        && let Some(feature) = feature
    {
        axes.entry(feature.to_owned())
            .or_default()
            .insert(axis.to_owned());
    }
}

pub(crate) fn collect_event_contracts(source: &str) -> HashMap<String, HashSet<String>> {
    let mut event_fields: HashMap<String, HashSet<String>> = HashMap::new();
    let mut groups_by_feature: HashMap<String, Vec<EventPayloadGroup>> = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_group_index: Option<usize> = None;
    let mut current_event: Option<String> = None;
    let mut in_group_payload = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_group_index = None;
                current_event = None;
                in_group_payload = false;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_group_index = None;
                current_event = None;
                in_group_payload = false;
            }
            4 if current_top == Some("domain") => {
                current_group_index = None;
                current_event = None;
                in_group_payload = false;

                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };

                if trimmed.starts_with("event_group ") || trimmed.starts_with("events ") {
                    if let Some(pattern) = trimmed.split_whitespace().nth(1) {
                        let prefix = pattern.trim_end_matches('*').to_owned();
                        let groups = groups_by_feature.entry(feature.to_owned()).or_default();
                        groups.push(EventPayloadGroup {
                            prefix,
                            fields: HashSet::new(),
                        });
                        current_group_index = Some(groups.len() - 1);
                    }
                } else if let Some(event) = event_decl_name(trimmed) {
                    let key = format!("{feature}.{event}");
                    event_fields.entry(key.clone()).or_default();
                    current_event = Some(key);
                }
            }
            6 if current_top == Some("domain") => {
                if current_group_index.is_some() {
                    if trimmed == "payload" {
                        in_group_payload = true;
                        current_event = None;
                    } else if let Some(event) = event_decl_name(trimmed) {
                        let Some(feature) = current_feature.as_deref() else {
                            continue;
                        };
                        let Some(group_index) = current_group_index else {
                            continue;
                        };
                        let Some(prefix) = groups_by_feature
                            .get(feature)
                            .and_then(|groups| groups.get(group_index))
                            .map(|group| group.prefix.as_str())
                        else {
                            continue;
                        };
                        let key = format!("{feature}.{}", qualify_group_event_name(prefix, event));
                        event_fields.entry(key.clone()).or_default();
                        current_event = Some(key);
                        in_group_payload = false;
                    } else {
                        in_group_payload = false;
                    }
                } else if let Some(event_key) = current_event.as_deref() {
                    if let Some((field, _)) = typed_param(trimmed) {
                        event_fields
                            .entry(event_key.to_owned())
                            .or_default()
                            .insert(field.to_owned());
                    }
                }
            }
            8 if current_top == Some("domain") => {
                if in_group_payload {
                    let Some(feature) = current_feature.as_deref() else {
                        continue;
                    };
                    let Some(group_index) = current_group_index else {
                        continue;
                    };
                    let Some(field) = payload_assignment_field(trimmed) else {
                        continue;
                    };
                    if let Some(group) = groups_by_feature
                        .get_mut(feature)
                        .and_then(|groups| groups.get_mut(group_index))
                    {
                        group.fields.insert(field.to_owned());
                    }
                } else if let Some(event_key) = current_event.as_deref()
                    && let Some((field, _)) = typed_param(trimmed)
                {
                    event_fields
                        .entry(event_key.to_owned())
                        .or_default()
                        .insert(field.to_owned());
                }
            }
            _ => {}
        }
    }

    for (event_ref, fields) in &mut event_fields {
        let Some((feature, event)) = event_ref.split_once('.') else {
            continue;
        };
        if let Some(groups) = groups_by_feature.get(feature) {
            for group in groups {
                if event.starts_with(&group.prefix) {
                    fields.extend(group.fields.iter().cloned());
                }
            }
        }
    }

    event_fields
}

pub(crate) fn event_decl_name(trimmed_line: &str) -> Option<&str> {
    if trimmed_line.starts_with("event.trace ") || trimmed_line.starts_with("event ") {
        trimmed_line.split_whitespace().nth(1)
    } else {
        None
    }
}

pub(crate) fn event_group_prefix(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }
    parts.next()?.strip_suffix('*')
}

pub(crate) fn qualify_group_event_name(prefix: &str, raw_name: &str) -> String {
    if raw_name.starts_with(prefix) {
        raw_name.to_owned()
    } else {
        format!("{prefix}{raw_name}")
    }
}

pub(crate) fn payload_assignment_field(trimmed_line: &str) -> Option<&str> {
    let (field, _) = trimmed_line.split_once('=')?;
    let field = field.trim();
    (!field.is_empty()).then_some(field)
}

pub(crate) fn payload_field_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("payload.") {
        let after_prefix = &rest[start + "payload.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let field = &after_prefix[..end];

        if !field.is_empty() {
            references.push(field.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}

pub(crate) fn event_consumer_payload_diagnostic(
    line_index: usize,
    line: &str,
    event_ref: &str,
    field: &str,
) -> Diagnostic {
    simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "event-consumer-payload",
        &format!(
            "`payload.{field}` is not declared by event `{event_ref}`. Consumers may only read fields from the producer event contract, including inherited `event_group` payload fields."
        ),
    )
}

pub(crate) fn collect_canonical_feature_facts(source: &str) -> HashMap<String, CanonicalFeatureFacts> {
    let mut features = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                let name = feature_name(trimmed);
                features
                    .entry(name.clone())
                    .or_insert_with(CanonicalFeatureFacts::default);
                current_feature = Some(name);
                current_top = None;
                current_resource = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_resource = None;
            }
            4 if current_top == Some("defaults") => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let feature = features.entry(feature_name.to_owned()).or_default();

                if let Some(tenancy) = tenancy_axis(trimmed) {
                    feature.default_tenancy = Some(tenancy.to_owned());
                } else if trimmed == "timestamps" {
                    feature.default_timestamps = true;
                }
            }
            4 if current_top == Some("domain") => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };

                if let Some(resource_name) = resource_name(trimmed) {
                    let feature = features.entry(feature_name.to_owned()).or_default();
                    let resource = CanonicalResourceFacts::new(
                        feature.default_tenancy.as_deref(),
                        feature.default_timestamps,
                    );
                    feature
                        .resources
                        .entry(resource_name.to_owned())
                        .or_insert(resource);
                    current_resource = Some(resource_name.to_owned());
                } else {
                    current_resource = None;
                }
            }
            6 if current_top == Some("domain") => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource_name) = current_resource.as_deref() else {
                    continue;
                };
                let Some(resource) = features
                    .get_mut(feature_name)
                    .and_then(|feature| feature.resources.get_mut(resource_name))
                else {
                    continue;
                };

                if let Some(tenancy) = tenancy_axis(trimmed) {
                    resource.set_tenancy(tenancy);
                } else if trimmed == "timestamps" {
                    resource.set_timestamps(true);
                } else if trimmed == "no_timestamps" {
                    resource.set_timestamps(false);
                } else if trimmed == "soft_delete" {
                    resource.fields.insert("deleted_at".to_owned());
                } else if let Some(field) = field_name(trimmed) {
                    resource.fields.insert(field.to_owned());
                }
            }
            _ => {}
        }
    }

    features
}

pub(crate) fn tenancy_axis(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "tenancy" {
        parts.next()
    } else {
        None
    }
}

pub(crate) fn resource_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "resource" {
        parts.next()
    } else {
        None
    }
}

pub(crate) fn events_resource_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }

    while let Some(part) = parts.next() {
        if part == "on" {
            return parts.next();
        }
    }

    None
}

pub(crate) fn field_name(trimmed_line: &str) -> Option<&str> {
    let (head, _) = trimmed_line.split_once(':')?;
    let name = head.trim().split_whitespace().next()?;

    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(name)
    } else {
        None
    }
}

pub(crate) fn payload_assignment_rhs(trimmed_line: &str) -> Option<&str> {
    let (_, rhs) = trimmed_line.split_once('=')?;
    Some(rhs.trim())
}

pub(crate) fn resource_field_reference(expression: &str) -> Option<&str> {
    let first = expression.bytes().next()?;

    if first == b'"' || first.is_ascii_digit() || first.is_ascii_uppercase() {
        return None;
    }

    let end = expression
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
        .unwrap_or(expression.len());
    let segment = &expression[..end];

    if segment.is_empty()
        || matches!(
            segment,
            "ctx"
                | "event"
                | "ext"
                | "input"
                | "nil"
                | "null"
                | "params"
                | "payload"
                | "route"
                | "self"
                | "true"
                | "false"
        )
    {
        None
    } else {
        Some(segment)
    }
}

pub(crate) fn event_payload_reference_diagnostic(
    line_index: usize,
    line: &str,
    resource_name: &str,
    field_name: &str,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: line_index as u32,
                character: leading_spaces(line) as u32,
            },
            end: Position {
                line: line_index as u32,
                character: line.len().max(leading_spaces(line) + 1) as u32,
            },
        },
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            "event-payload-field".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "event payload references `{field_name}`, but resource `{resource_name}` has no field named `{field_name}`. Shared event payload expressions resolve against the `event_group ... on {resource_name}` resource."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}


pub(crate) fn format_canonical_source(source: &str) -> Option<String> {
    if !is_canonical_source(source) {
        return None;
    }

    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut formatted_lines = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        if leading_spaces(line) == 0 && line.trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let next = lines[index];
                if leading_spaces(next) == 0 && next.trim_start().starts_with("feature ") {
                    break;
                }
                index += 1;
            }

            formatted_lines.extend(format_feature_lines(&lines[start..index]));
        } else {
            formatted_lines.push(line.to_owned());
            index += 1;
        }
    }

    let mut formatted = formatted_lines.join(newline);
    if source.ends_with('\n') {
        formatted.push_str(newline);
    }

    Some(formatted)
}

#[derive(Debug)]
pub(crate) struct FeatureBlockSegment {
    kind: Option<CanonicalBlockKind>,
    ordinal: usize,
    lines: Vec<String>,
}

pub(crate) fn format_feature_lines(lines: &[&str]) -> Vec<String> {
    let Some((first, rest)) = lines.split_first() else {
        return Vec::new();
    };

    let mut formatted = vec![(*first).to_owned()];
    let mut segments = Vec::new();
    let mut index = 0;

    while index < rest.len() && is_trivia_line(rest[index]) {
        formatted.push(rest[index].to_owned());
        index += 1;
    }

    while index < rest.len() {
        let line = rest[index];
        let kind = if leading_spaces(line) == 2 {
            canonical_block_kind(line.trim_start())
        } else {
            None
        };

        let start = index;
        index += 1;

        if kind.is_some() {
            while index < rest.len() {
                let next = rest[index];
                if leading_spaces(next) == 2 && canonical_block_kind(next.trim_start()).is_some() {
                    break;
                }
                index += 1;
            }
        } else {
            while index < rest.len() {
                let next = rest[index];
                if leading_spaces(next) == 2 && canonical_block_kind(next.trim_start()).is_some() {
                    break;
                }
                index += 1;
            }
        }

        segments.push(FeatureBlockSegment {
            kind,
            ordinal: segments.len(),
            lines: rest[start..index]
                .iter()
                .map(|line| (*line).to_owned())
                .collect(),
        });
    }

    segments.sort_by_key(|segment| {
        (
            segment
                .kind
                .map(CanonicalBlockKind::rank)
                .unwrap_or(u8::MAX),
            segment.ordinal,
        )
    });

    for segment in segments {
        if segment.kind == Some(CanonicalBlockKind::Workflow) {
            formatted.extend(format_workflow_lines(segment.lines));
        } else {
            formatted.extend(segment.lines);
        }
    }

    formatted
}

pub(crate) fn format_workflow_lines(lines: Vec<String>) -> Vec<String> {
    let mut formatted = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        formatted.push(line.to_owned());

        if is_transition_line(line.trim_start()) {
            let transition_indent = leading_spaces(line);
            let mut next_non_blank = index + 1;

            while next_non_blank < lines.len() && lines[next_non_blank].trim().is_empty() {
                next_non_blank += 1;
            }

            if next_non_blank > index + 1
                && next_non_blank < lines.len()
                && leading_spaces(&lines[next_non_blank]) > transition_indent
            {
                index = next_non_blank;
                continue;
            }
        }

        index += 1;
    }

    formatted
}

pub(crate) fn is_trivia_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

pub(crate) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

pub(crate) fn feature_name(trimmed_line: &str) -> String {
    trimmed_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("<anonymous>")
        .to_owned()
}

pub(crate) fn range_from_span(source: &str, span: Span) -> Range {
    let len = source.len();
    let start = span.start.min(len);
    let end = span.end.max(span.start.saturating_add(1)).min(len);

    Range {
        start: position_for_offset(source, start),
        end: position_for_offset(source, end),
    }
}

pub(crate) fn first_line_range(source: &str) -> Range {
    let end = source.lines().next().map(str::len).unwrap_or(1).max(1);
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: end as u32,
        },
    }
}

pub(crate) fn full_document_range(source: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: position_for_offset(source, source.len()),
    }
}

pub(crate) fn position_for_offset(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position { line, character }
}

pub(crate) fn word_at_position(source: &str, position: Position) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let target = byte_index_for_utf16_position(line, position.character);
    let mut start = target.min(line.len());
    let mut end = target.min(line.len());
    let bytes = line.as_bytes();

    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }

    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }

    if start == end {
        None
    } else {
        Some(line[start..end].to_owned())
    }
}

pub(crate) fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

pub(crate) fn line_prefix_at_position(line: &str, character: u32) -> &str {
    let byte_index = byte_index_for_utf16_position(line, character);
    &line[..byte_index]
}

pub(crate) fn byte_index_for_utf16_position(line: &str, character: u32) -> usize {
    let mut utf16 = 0u32;
    for (byte_index, ch) in line.char_indices() {
        if utf16 >= character {
            return byte_index;
        }
        let next = utf16 + ch.len_utf16() as u32;
        if next > character {
            return byte_index;
        }
        utf16 = next;
    }
    line.len()
}

pub(crate) fn is_design_lzi_uri(uri: &Url) -> bool {
    uri.path().ends_with("design.lzi")
}

pub(crate) fn is_lzx_uri(uri: &Url) -> bool {
    uri.path().ends_with(".lzx")
}


/// Row 30 — context-aware closed-catalog completions for the four
/// `@cap.File(...)` argument values. Returns `None` outside of
/// `@cap.File(...)`; when inside, looks at the most recent keyword
/// before the cursor to pick the right catalog:
///
/// - `visibility:` → `public`, `private`, `signed`
/// - `max_size:<int>` → `kb`, `mb`, `gb`
/// - `signed_ttl:<int>` → `s`, `m`, `h`, `d`
/// - `accept:` → the seven IANA-top families (`text`, `image`, …, `*`)
pub(crate) fn cap_file_value_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    // Cheap context check — only fire when we are inside an open
    // `@cap.File(` on the same line. Multi-line capabilities are not
    // canonical; the LSP only sees the current line for this hint.
    let open = before.rfind("@cap.File(")?;
    let after_open = &before[open + "@cap.File(".len()..];

    // Find the most recent argument keyword. We accept either
    // `<key>:` (cursor right after the colon) or `<key>:<value>`
    // (cursor mid-value).
    let trimmed = after_open.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    let last_colon = trimmed.rfind(':')?;
    // The argument key is the word ending at last_colon.
    let prefix_to_colon = &trimmed[..last_colon];
    let key_start = prefix_to_colon
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let key = &prefix_to_colon[key_start..];

    let labels: &[(&str, &str)] = match key {
        "visibility" => &[
            ("public", "Unguessable URL; un-gated fetch (CDN-style)."),
            (
                "private",
                "Policy-gated download handler enforced by the runtime.",
            ),
            ("signed", "Time-limited signed URL; requires `signed_ttl`."),
        ],
        "max_size" => &[
            (
                "kb",
                "Kilobyte size unit (binary prefix; `n * 1024` bytes).",
            ),
            (
                "mb",
                "Megabyte size unit (binary prefix; `n * 1024^2` bytes).",
            ),
            (
                "gb",
                "Gigabyte size unit (binary prefix; `n * 1024^3` bytes).",
            ),
        ],
        "signed_ttl" => &[
            ("s", "Seconds."),
            ("m", "Minutes."),
            ("h", "Hours."),
            ("d", "Days."),
        ],
        "accept" => &[
            (
                "text",
                "IANA family `text` (e.g. `text/csv`, `text/plain`).",
            ),
            ("image", "IANA family `image` (e.g. `image/png`)."),
            (
                "application",
                "IANA family `application` (e.g. `application/json`).",
            ),
            ("audio", "IANA family `audio`."),
            ("video", "IANA family `video`."),
            ("font", "IANA family `font`."),
            ("*", "Wildcard family."),
        ],
        _ => return None,
    };

    Some(
        labels
            .iter()
            .map(|(label, detail)| CompletionItem {
                label: (*label).to_owned(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some((*detail).to_owned()),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5 — when
/// the cursor sits inside `@owner_axis(through: <|>)`, offer the FK
/// fields of the current resource as completion candidates. "FK field"
/// here means a field on the surrounding `resource <Name>` block whose
/// `type_text` is a bare PascalCase identifier (the analyzer resolves
/// these to `TypeRef::UserDefined(QualifiedName)` — surface-level
/// references to other resources).
///
/// Returns `None` outside `@owner_axis(through: ...)`; when inside but
/// no FK fields are visible on the surrounding resource, returns
/// `Some(vec![])` (the LSP suppresses the global keyword list in
/// favour of the empty context-specific list rather than offering noise).
pub(crate) fn owner_axis_through_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    // Cheap context check — only fire when we are inside an open
    // `@owner_axis(` on the same line and the cursor is positioned
    // after `through:` (the only keyword argument in this proposal —
    // see §7.1 grammar).
    let open = before.rfind("@owner_axis(")?;
    let after_open = &before[open + "@owner_axis(".len()..];
    let through_idx = after_open.rfind("through:")?;
    let after_through = &after_open[through_idx + "through:".len()..];
    // Accept either cursor right after `through:` (possibly with
    // whitespace) or mid-value with a partial identifier. Reject when
    // a comma intervenes (would mean we've moved to a different — and
    // currently non-existent — argument).
    if after_through.contains(',') {
        return None;
    }

    // Walk source lines backward from the cursor to find the
    // surrounding `resource <Name>` header. Indent-aware: the resource
    // header is at the feature's `resource` indent (2 spaces in
    // canonical authoring), and field lines sit one level deeper.
    let cursor_line = position.line as usize;
    let lines: Vec<&str> = source.lines().collect();
    let mut resource_start: Option<usize> = None;
    let mut resource_indent: Option<usize> = None;
    for idx in (0..=cursor_line.min(lines.len().saturating_sub(1))).rev() {
        let l = lines[idx];
        let trimmed = l.trim_start();
        if let Some(name) = trimmed.strip_prefix("resource ") {
            // `resource <Name>` header — anchors our field scan.
            // Ignore the trailing modifiers (none authored today).
            let _ = name;
            resource_start = Some(idx);
            resource_indent = Some(l.len() - trimmed.len());
            break;
        }
    }
    let start = resource_start?;
    let res_indent = resource_indent?;

    // Scan forward from the resource header collecting field names
    // whose `type_text` looks like a bare resource reference (PascalCase
    // identifier with no decorator chain and no builtin keyword).
    let mut fk_fields: Vec<String> = Vec::new();
    for l in lines.iter().skip(start + 1) {
        let trimmed = l.trim_start();
        let indent = l.len() - trimmed.len();
        // Stop at the next sibling/parent block (same or shallower indent
        // than the resource header), skipping blank lines.
        if !trimmed.is_empty() && indent <= res_indent {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Only consider direct-child lines of the resource (depth
        // exactly `res_indent + 2`). Deeper indents are sub-clauses
        // (e.g. `conventions [..]` children, lifecycle bodies).
        if indent != res_indent + 2 {
            continue;
        }
        // Field declarations have shape `<name>: <Type> ...`. Split
        // off the type half and discard everything past the first
        // whitespace / modifier / decorator.
        let Some((name, after_colon)) = trimmed.split_once(':') else {
            continue;
        };
        let field_name = name.trim();
        // Field names are snake_case identifiers; reject other lines
        // (e.g. `conventions [..]`).
        if !field_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || field_name.is_empty()
        {
            continue;
        }
        let type_text = after_colon.trim_start();
        // Take the first whitespace-delimited token as the type. FK
        // type refs are bare PascalCase identifiers with no `@` prefix
        // and no `.` (builtins like `Text`/`Integer` are filtered by
        // the closed-catalog skip-list below).
        let head = type_text
            .split(|c: char| c.is_ascii_whitespace())
            .next()
            .unwrap_or("");
        if head.is_empty() || head.starts_with('@') || head.contains('.') {
            continue;
        }
        let first_char = head.chars().next().unwrap_or('a');
        if !first_char.is_ascii_uppercase() {
            continue;
        }
        // Closed-catalog skip list — builtin PascalCase types that are
        // not FK references. `User`/`Org` are excluded so the synth's
        // tenant-keyed default is the canonical surface; authors who
        // want owner-scope on a tenant column would do it via the
        // `user: User required unique` field semantics, not @owner_axis.
        if matches!(
            head,
            "Text" | "Integer" | "Boolean" | "Date" | "DateTime" | "Decimal" | "Json" | "ID" | "Id"
        ) {
            continue;
        }
        fk_fields.push(field_name.to_owned());
    }

    Some(
        fk_fields
            .into_iter()
            .map(|name| CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("FK column on the current resource".to_owned()),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Cell A4 — `input.<field>` completion inside a `command` block.
///
/// Surfaces both `command.route` slots (typed `route <name>: <Type>` lines)
/// and `command.input` slots (short `input a, b, c` form and the typed
/// `input` block) so authors don't hit "field not found" at codegen time
/// when they reach for a route parameter inside `effect.bindings` /
/// `effect.where`.
///
/// **Ordering**: route params are surfaced *first* (sort_text `0_*`) so
/// the IDE puts them at the top of the completion list — that matches
/// the struct-field order Cell A1/A2 used at the IR layer and gives
/// authors an at-a-glance hint that the `id` they want lives on `route`,
/// not `input`. Each item carries a detail label that distinguishes
/// route-param vs input-field at-a-glance.
///
/// Trigger: the line prefix ends with `input.<optional-partial>` and the
/// cursor sits inside a `command` block (any indent depth — covers both
/// the `name = input.id` binding shape at indent 6 and any future
/// `where (id = input.id)` expression on the command body at indent 4).
pub fn input_field_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trigger = input_dot_trigger(before)?;
    let _ = trigger; // accepted; partial identifier text not needed for filtering — client filters.

    let (route_params, input_fields) = collect_command_input_and_route_params(source, position)?;

    if route_params.is_empty() && input_fields.is_empty() {
        return None;
    }

    let mut items: Vec<CompletionItem> = Vec::new();

    for (idx, name) in route_params.iter().enumerate() {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some("route param".to_owned()),
            sort_text: Some(format!("0_{:03}_{name}", idx)),
            ..CompletionItem::default()
        });
    }

    for (idx, name) in input_fields.iter().enumerate() {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some("input field".to_owned()),
            sort_text: Some(format!("1_{:03}_{name}", idx)),
            ..CompletionItem::default()
        });
    }

    Some(items)
}

/// Returns `Some(partial_identifier_text)` when the line prefix ends with
/// `input.<partial>` (partial may be empty). Returns `None` otherwise.
pub(crate) fn input_dot_trigger(before: &str) -> Option<&str> {
    let dot = before.rfind("input.")?;
    let after_dot = &before[dot + "input.".len()..];
    if !after_dot
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    // The `input.` token must be at a word boundary — guard against a
    // suffix match inside a longer identifier like `payload_input.x`.
    let prefix = &before[..dot];
    if let Some(last) = prefix.chars().last() {
        if last.is_ascii_alphanumeric() || last == '_' {
            return None;
        }
    }
    Some(after_dot)
}

/// Walk source backward from `position` to locate the enclosing
/// `command <name>` header, then walk forward gathering its `route` slot
/// names (in source order) and `input` field names (short + typed). Stops
/// at the next sibling/parent block.
pub(crate) fn collect_command_input_and_route_params(
    source: &str,
    position: Position,
) -> Option<(Vec<String>, Vec<String>)> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line = (position.line as usize).min(lines.len().saturating_sub(1));

    let mut command_start: Option<usize> = None;
    let mut command_indent: usize = 0;
    for idx in (0..=cursor_line).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("command ")
            || trimmed == "command"
            || trimmed.starts_with("command\t")
        {
            command_start = Some(idx);
            command_indent = leading_spaces(line);
            break;
        }
    }
    let start = command_start?;
    let child_indent = command_indent + 2;
    let body_indent = command_indent + 4;

    let mut route_params: Vec<String> = Vec::new();
    let mut input_fields: Vec<String> = Vec::new();
    let mut in_typed_input_block = false;

    for line in lines.iter().skip(start + 1) {
        let trimmed = line.trim_start();
        let indent = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Reached the next sibling/parent block — stop scanning.
        if indent <= command_indent {
            break;
        }

        if indent == child_indent {
            in_typed_input_block = false;

            if let Some(rest) = trimmed.strip_prefix("route ") {
                // Accept both `route id: ID` (named slot) and a bare
                // `route` (unlikely but safe to skip).
                let name = rest
                    .split(|c: char| c == ':' || c.is_ascii_whitespace())
                    .find(|t| !t.is_empty())
                    .unwrap_or("");
                if !name.is_empty() && route_params.iter().all(|n| n != name) {
                    route_params.push(name.to_owned());
                }
                continue;
            }

            if trimmed == "input" {
                in_typed_input_block = true;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("input ") {
                for field in rest.split(',').map(str::trim) {
                    if field.is_empty() {
                        continue;
                    }
                    // Reject typed-pair shape `name: Type` at this slot —
                    // doctor already warns; we just skip.
                    if field.contains(':') {
                        continue;
                    }
                    if field
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        && input_fields.iter().all(|n| n != field)
                    {
                        input_fields.push(field.to_owned());
                    }
                }
                continue;
            }

            continue;
        }

        if in_typed_input_block && indent == body_indent {
            // `<name>: <Type>` declarations.
            if let Some((name, _)) = trimmed.split_once(':') {
                let name = name.trim();
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && input_fields.iter().all(|n| n != name)
                {
                    input_fields.push(name.to_owned());
                }
            }
        }
    }

    Some((route_params, input_fields))
}

pub(crate) fn design_keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "design" => Some(
            "Declares the project-root design token catalog. See `docs/proposals/design-tokens.md`.",
        ),
        "extends" => Some(
            "Declares a base design catalog that this design overrides. See `docs/proposals/design-tokens.md`.",
        ),
        "color" => Some(
            "Closed design token group for brand, semantic, and surface colors. See `docs/proposals/design-tokens.md`.",
        ),
        "typography" => Some(
            "Closed design token group for type families, scales, weights, and tracking. See `docs/proposals/design-tokens.md`.",
        ),
        "space" => Some(
            "Closed design token group for the spacing scale. See `docs/proposals/design-tokens.md`.",
        ),
        "radius" => Some(
            "Closed design token group for border radius values. See `docs/proposals/design-tokens.md`.",
        ),
        "shadow" => Some(
            "Closed design token group for CSS box-shadow elevation values. See `docs/proposals/design-tokens.md`.",
        ),
        "motion" => Some(
            "Closed design token group for transition and animation primitives. See `docs/proposals/design-tokens.md`.",
        ),
        "breakpoint" => Some(
            "Closed design token group for responsive viewport cutoffs. See `docs/proposals/design-tokens.md`.",
        ),
        "z" => Some(
            "Closed design token group for stacking order values. See `docs/proposals/design-tokens.md`.",
        ),
        "family" => Some(
            "Typography sub-group for named font stacks. See `docs/proposals/design-tokens.md`.",
        ),
        "scale" => Some(
            "Typography sub-group for named text sizes and line heights. See `docs/proposals/design-tokens.md`.",
        ),
        "weight" => Some(
            "Typography sub-group for named font weights. See `docs/proposals/design-tokens.md`.",
        ),
        "tracking" => Some(
            "Typography sub-group for named letter-spacing values. See `docs/proposals/design-tokens.md`.",
        ),
        "duration" => Some(
            "Motion sub-group for named transition durations. See `docs/proposals/design-tokens.md`.",
        ),
        "easing" => {
            Some("Motion sub-group for named easing curves. See `docs/proposals/design-tokens.md`.")
        }
        "size" => Some(
            "Typography scale field for a text token's font size. See `docs/proposals/design-tokens.md`.",
        ),
        "line_height" => Some(
            "Typography scale field for a text token's line height. See `docs/proposals/design-tokens.md`.",
        ),
        "base" => Some(
            "Required default color state; also commonly used as a token name. See `docs/proposals/design-tokens.md`.",
        ),
        "hover" => Some(
            "Optional color state for mouse hover or touch press start. See `docs/proposals/design-tokens.md`.",
        ),
        "active" => Some(
            "Optional color state for mouse down or touch press end. See `docs/proposals/design-tokens.md`.",
        ),
        "foreground" => Some(
            "Optional text/icon color when the token is used as a background. See `docs/proposals/design-tokens.md`.",
        ),
        "dark" => Some(
            "Optional dark-theme suffix for a color value. See `docs/proposals/design-tokens.md`.",
        ),
        _ => None,
    }
}


/// Children offered as completion when the cursor sits on an indented
/// blank line inside a known kind block. The first slice element is
/// the kind keyword (matched by `block_kind_at`), the second is the
/// closed-catalog children offered as `CompletionItem`s. Required and
/// optional children are merged so authors / LLMs see the full
/// vocabulary; the per-token hover (via `keyword_description`) tells
/// them which are required.
///
/// The kind matcher is forgiving: any line whose first non-space token
/// starts with one of these prefixes (e.g. `command capture_lead`)
/// counts as the matching block.
pub(crate) const KIND_CHILD_COMPLETIONS: &[(&str, &[&str])] = &[
    (
        "command",
        &[
            "input",
            "route",
            "policy",
            "rate_limit",
            "audit",
            "validate",
            "let",
            "creates",
            "updates",
            "deletes",
            "returns",
            "handler",
            "emits",
            "invalidates",
            "approval",
            "tests",
            "previously",
            "deprecated",
            "gate",
            "idempotency",
            "write_window",
        ],
    ),
    (
        "query.list",
        &[
            "params",
            "filters",
            "search",
            "order",
            "paginate",
            "cache",
            "policy",
            "modifier",
            "scope",
            "rate_limit",
            "audit",
        ],
    ),
    (
        "query.lookup",
        &["params", "key", "policy", "cache", "scope", "audit"],
    ),
    (
        "query.sql",
        &[
            "returns", "sql", "params", "scope", "policy", "cache", "audit",
        ],
    ),
    (
        "query.view",
        &["policy", "returns", "source", "params", "scope"],
    ),
    // CL.C.3 — feature-level `cache <name>` profile children. The
    // inline (per-query) cache shape reuses these same keywords; the
    // context-aware completion uses the block kind, not the shape.
    (
        "cache",
        &[
            "key",
            "ttl",
            "namespace",
            "tags",
            "stale_while_revalidate",
            "coalesce",
            "sliding",
        ],
    ),
    (
        "api",
        &[
            "method",
            "path",
            "input",
            "output",
            "policy",
            "handler",
            "rate_limit",
            "audit",
            "route",
            "deprecated",
        ],
    ),
    ("deprecated", &["since", "replacement", "sunset"]),
    (
        "agent",
        &[
            "input",
            "context",
            "policy",
            "rate_limit",
            "output",
            "model",
            "prompt",
            "tools",
            "safety",
            "evals",
            "temperature",
            "top_p",
            "max_tokens",
            "seed",
            "expose",
            "audit",
        ],
    ),
    ("error_page", &["template", "audience"]),
    (
        "tenant_migration",
        &[
            "target",
            "axis",
            "idempotency",
            "timeout",
            "retry",
            "handler",
        ],
    ),
];

/// Closed catalog of effect verbs offered as completion when the
/// cursor is positioned where an effect would go inside a `command`.
/// `returns` is the non-mutating sibling shipped on commands like the
/// `smoke-hello` fixture's `greet` command.
pub(crate) const EFFECT_VERBS: &[&str] = &["creates", "updates", "deletes", "returns"];

/// Closed catalog of `rate_limit` axis tokens for the
/// `"<N> per <window> per <axis>"` grammar. Surfaced inside double
/// quotes after `per` so authors / LLMs see the closed set instead of
/// guessing tenant-style words.
pub(crate) const RATE_LIMIT_AXES: &[&str] = &["ip", "user", "org", "tenant"];

/// Detect which kind block the cursor is in by walking the source
/// backwards from `position.line` looking for the closest header line
/// at the parent indent. Returns the kind keyword (e.g. `command`,
/// `query.list`, `api`) or `None` when the cursor is at the top
/// level / outside a recognised block.
///
/// We treat header-line indent as "parent" if it's strictly less than
/// the cursor line's indent. The first kind keyword we find at that
/// shallower indent wins. This handles the canonical Lazurite layout
/// where `command capture_lead` sits at indent 2 inside a feature and
/// its children at indent 4.
pub(crate) fn block_kind_at(source: &str, position: Position) -> Option<&'static str> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = position.line as usize;
    if cursor_line_idx >= lines.len() {
        // The completion handler can be called past EOL; treat as the
        // last real line.
    }
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);

    // Walk backwards looking for a header line at indent < cursor_indent.
    for idx in (0..cursor_line_idx).rev() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < cursor_indent {
            // Try each known kind prefix. Match the longest first to
            // distinguish `query.list` from a hypothetical `query`.
            let mut kinds: Vec<&str> = KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
            kinds.sort_by_key(|k| std::cmp::Reverse(k.len()));
            for kind in kinds {
                if trimmed == kind
                    || trimmed.starts_with(&format!("{kind} "))
                    || trimmed.starts_with(&format!("{kind}\t"))
                {
                    return Some(kind);
                }
            }
            // Found a shallower line but it isn't one of our kinds —
            // stop walking; we're inside a sibling/parent block we
            // don't have a rich completion for yet.
            return None;
        }
    }
    None
}

/// Context-aware completions covering the seven LSP-extended kinds.
/// Returns `None` to fall back to the flat keyword list; returns
/// `Some(items)` when a context is recognised. Three contexts handled:
///
/// 1. `@policy.` / `@validator.` / `@fn.` / `@hook.` / `@role.` /
///    `@scope.` / `@actor.` immediately before the cursor — offers
///    every declared name from the same source.
/// 2. Indented blank line inside a known kind block — offers the
///    closed-catalog children (`KIND_CHILD_COMPLETIONS`). When the
///    block is a `command` the effect verbs are tagged as
///    `ENUM_MEMBER` so editors render them distinctly.
/// 3. Inside a `rate_limit "..."` value after the second `per ` —
///    offers the closed axis catalog (`ip`/`user`/`org`/`tenant`).
pub(crate) fn context_aware_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);

    // `docs/proposals/ir-resource-conventions-crud.md` §4.4 — closed-
    // catalog completion inside `conventions [..]`. Runs first because
    // the trigger is narrow (cursor inside a single-line bracket list
    // after the `conventions` keyword) and the surface is single-
    // member today; ranking before other context checks keeps the
    // typo'd intermediate `co` keystroke from offering noisy matches.
    if let Some(items) = conventions_list_completions(before) {
        return Some(items);
    }

    // IR Route-Guards — narrow context completions for view policies,
    // redirect paths, `actor_query`, and `app.route_guard` defaults.
    // Runs before generic namespace completion so `policy @policy.` inside
    // a view can offer full `@policy.<name>` refs instead of bare names.
    let lifecycle_items = lifecycle_gate_completions(source, position);
    let route_guard_items = route_guard_completions(source, position);
    if lifecycle_items.is_some() || route_guard_items.is_some() {
        return Some(merge_completion_items(lifecycle_items, route_guard_items));
    }

    // 1. `@<ns>.` prefix completion.
    if let Some(items) = namespace_prefix_completions(source, before) {
        return Some(items);
    }

    // 3. `rate_limit "...per <axis>"` — checked before the indent
    // path so we don't drop into kind-child completion while the
    // cursor is inside a string literal on a `rate_limit` line.
    if let Some(items) = rate_limit_axis_completions(before) {
        return Some(items);
    }

    // IR Rate-Limit env-aware (`docs/proposals/ir-rate-limit-env-aware.md`
    // §11.3) — when the cursor sits inside the `in <env>` tail of a
    // `rate_limit "..." in <|>` line, surface the 5-entry closed env
    // catalog. Runs after `rate_limit_axis_completions` so the axis
    // inside-string slot still wins when the cursor is in the spec.
    if let Some(items) = rate_limit_env_completions(before) {
        return Some(items);
    }

    if let Some(items) = error_page_value_completions(source, position, before) {
        return Some(items);
    }

    // IR Auth Refresh Rotation — narrow completions for
    // `auth.sessions.access_ttl` and nested `rotation` children. This runs
    // before error-vocab and kind-child fallback because the triggers are
    // specific to the auth.sessions indent shape.
    if let Some(items) = auth_refresh_completions(source, position) {
        return Some(items);
    }

    // IR Error-Vocab — the 6 trigger positions from proposal §7.1. Runs
    // before the indent-aware kind-child fallback because the matches are
    // strictly narrower (`when_denied `, `<code> message `,
    // `expose client 4xx ...`, `default ` inside `errors`, blank line inside
    // `errors`).
    if let Some(items) = error_vocab_completions(source, position) {
        return Some(items);
    }

    // 2. Indent-aware kind child.
    // Only fire when the prefix is whitespace (cursor on a blank
    // indented line) or a partial child keyword. We don't want to
    // shadow `@cap.File(...)` value completion (handled earlier in
    // the dispatch chain) or general keyword completion mid-token in
    // an unrelated context.
    let trimmed_before = before.trim_start();
    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word = !trimmed_before.is_empty()
        && trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !(is_blank_indented || is_partial_word) {
        return None;
    }
    let kind = block_kind_at(source, position)?;
    let (_, children) = KIND_CHILD_COMPLETIONS.iter().find(|(k, _)| *k == kind)?;
    let mut items: Vec<CompletionItem> = children
        .iter()
        .map(|child| CompletionItem {
            label: (*child).to_owned(),
            kind: Some(if EFFECT_VERBS.contains(child) {
                CompletionItemKind::ENUM_MEMBER
            } else {
                CompletionItemKind::KEYWORD
            }),
            detail: keyword_description(child).map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect();
    // Sort effects to the top of `command` completions so authors see
    // the canonical write verbs at-a-glance.
    if kind == "command" {
        items.sort_by_key(|item| {
            (
                !EFFECT_VERBS.contains(&item.label.as_str()),
                item.label.clone(),
            )
        });
    }
    Some(items)
}


pub(crate) fn convention_bundle_hover(source: &str, position: Position, word: &str) -> Option<String> {
    let bundle = match word {
        "crud" | "me" => word,
        _ => return None,
    };
    if !is_inside_conventions_list(source, position) {
        return None;
    }

    let lines: &[&str] = match bundle {
        "crud" => &[
            "`conventions [crud]` synthesizes:",
            "- `query.list list_<resource_snake>s` - paginated list of rows",
            "- `query.lookup lookup_<resource_snake>` - lookup by route id",
            "- `command create_<resource_snake>`",
            "- `command update_<resource_snake>`",
            "- `command delete_<resource_snake>`",
            "",
            "When the author declares a query/command with one of these names, the synth silently skips that entry (author wins).",
        ],
        "me" => &[
            "`conventions [me]` synthesizes:",
            "- `query.lookup lookup_my_<resource_snake>` - lookup for the active actor",
            "",
            "When the author declares a query with this name, the synth silently skips that entry (author wins).",
        ],
        _ => return None,
    };
    Some(lines.join("\n"))
}

pub(crate) fn is_inside_conventions_list(source: &str, position: Position) -> bool {
    let Some(line) = source.lines().nth(position.line as usize) else {
        return false;
    };
    let trimmed = line.trim_start();
    if !trimmed.starts_with("conventions ") {
        return false;
    }

    let cursor = byte_index_for_utf16_position(line, position.character);
    let before = &line[..cursor.min(line.len())];
    let Some(conv_idx) = before.rfind("conventions") else {
        return false;
    };
    let after_kw_before_cursor = &before[conv_idx + "conventions".len()..];
    after_kw_before_cursor.contains('[') && !after_kw_before_cursor.contains(']')
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthSessionsBlock {
    pub(crate) line_idx: usize,
    pub(crate) indent: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthRotationBlock {
    pub(crate) line_idx: usize,
    pub(crate) indent: usize,
}

/// Completion provider for Cell LSP-1 of auth refresh rotation. This stays
/// text/indent based to mirror the rest of this crate's lightweight LSP
/// helpers and avoid touching parser, IR, codegen, or runtime layers.
pub fn auth_refresh_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    let trimmed_before = before.trim_start();

    if enclosing_auth_sessions_block(source, position).is_some()
        && after_keyword_value_prefix(trimmed_before, "access_ttl")
    {
        return Some(duration_literal_completion_items(
            AUTH_REFRESH_ACCESS_DURATION_LITERALS,
        ));
    }

    if enclosing_auth_sessions_block(source, position).is_some() && trimmed_before == "rotation" {
        return Some(vec![rotation_block_snippet_completion(leading_spaces(
            line,
        ))]);
    }

    if enclosing_auth_rotation_block(source, position).is_some() {
        if after_keyword_value_prefix(trimmed_before, "refresh_ttl") {
            return Some(duration_literal_completion_items(
                AUTH_REFRESH_REFRESH_DURATION_LITERALS,
            ));
        }
        if after_keyword_value_prefix(trimmed_before, "grace") {
            return Some(duration_literal_completion_items(
                AUTH_REFRESH_GRACE_DURATION_LITERALS,
            ));
        }
        if after_keyword_value_prefix(trimmed_before, "theft_detection_action") {
            return Some(auth_refresh_theft_action_completion_items());
        }

        let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
        let is_partial_child = !trimmed_before.is_empty()
            && !trimmed_before.chars().any(char::is_whitespace)
            && trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if is_blank_indented || is_partial_child {
            return Some(auth_refresh_rotation_clause_completion_items());
        }
    }

    None
}

pub(crate) fn after_keyword_value_prefix(trimmed_before: &str, keyword: &str) -> bool {
    let Some(rest) = trimmed_before.strip_prefix(keyword) else {
        return false;
    };
    rest.starts_with(' ')
        && rest
            .trim_start()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '"' || c == ' ')
}

pub(crate) fn duration_literal_completion_items(values: &[&str]) -> Vec<CompletionItem> {
    values
        .iter()
        .map(|value| CompletionItem {
            label: (*value).to_owned(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Duration literal for auth session rotation.".to_owned()),
            insert_text: Some((*value).to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn auth_refresh_theft_action_completion_items() -> Vec<CompletionItem> {
    AUTH_REFRESH_THEFT_ACTION_VALUES
        .iter()
        .map(|value| CompletionItem {
            label: (*value).to_owned(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: auth_refresh_theft_action_detail(value).map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn rotation_block_snippet_completion(line_indent: usize) -> CompletionItem {
    let child_indent = " ".repeat(line_indent + 2);
    CompletionItem {
        label: "scaffold rotation block".to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(
            "Insert refresh_ttl, grace, and theft_detection_action defaults under rotation."
                .to_owned(),
        ),
        insert_text: Some(format!(
            "\n{child_indent}refresh_ttl \"30 days\" # framework default\n{child_indent}grace \"30 seconds\" # framework default\n{child_indent}theft_detection_action revoke_session_family # framework default"
        )),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    }
}

pub(crate) fn auth_refresh_rotation_clause_completion_items() -> Vec<CompletionItem> {
    [
        (
            "refresh_ttl \"30 days\"",
            "refresh_ttl \"30 days\"",
            "Long-lived refresh token TTL. Framework default: 30 days.",
        ),
        (
            "grace \"30 seconds\"",
            "grace \"30 seconds\"",
            "Two-tab refresh race window. Framework default: 30 seconds.",
        ),
        (
            "theft_detection_action revoke_session_family",
            "theft_detection_action revoke_session_family",
            "Default theft response: revoke this session family.",
        ),
    ]
    .into_iter()
    .map(|(label, insert_text, detail)| CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(insert_text.to_owned()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    })
    .collect()
}

pub(crate) fn enclosing_auth_sessions_block(source: &str, position: Position) -> Option<AuthSessionsBlock> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));

    for idx in 0..=cursor_line_idx {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if is_trivia_line(line) || !is_sessions_line(trimmed) {
            continue;
        }
        let indent = leading_spaces(line);
        if !has_auth_parent(&lines, idx, indent) {
            continue;
        }
        let end_line = block_end_line(&lines, idx, indent);
        if cursor_line_idx >= idx && cursor_line_idx < end_line {
            return Some(AuthSessionsBlock {
                line_idx: idx,
                indent,
                end_line,
            });
        }
    }

    None
}

pub(crate) fn enclosing_auth_rotation_block(source: &str, position: Position) -> Option<AuthRotationBlock> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));

    for idx in 0..=cursor_line_idx {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if is_trivia_line(line) || !is_rotation_line(trimmed) {
            continue;
        }
        if enclosing_auth_sessions_block(
            source,
            Position {
                line: idx as u32,
                character: 0,
            },
        )
        .is_none()
        {
            continue;
        }
        let indent = leading_spaces(line);
        let end_line = block_end_line(&lines, idx, indent);
        if cursor_line_idx >= idx && cursor_line_idx < end_line {
            return Some(AuthRotationBlock {
                line_idx: idx,
                indent,
            });
        }
    }

    None
}

pub(crate) fn is_sessions_line(trimmed: &str) -> bool {
    trimmed.split_whitespace().next() == Some("sessions")
}

pub(crate) fn is_rotation_line(trimmed: &str) -> bool {
    trimmed.split_whitespace().next() == Some("rotation")
}

pub(crate) fn has_auth_parent(lines: &[&str], line_idx: usize, child_indent: usize) -> bool {
    for idx in (0..line_idx).rev() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < child_indent {
            let trimmed = line.trim_start();
            return trimmed == "auth" || trimmed.starts_with("auth ");
        }
    }
    false
}

pub(crate) fn block_end_line(lines: &[&str], start_idx: usize, block_indent: usize) -> usize {
    for idx in (start_idx + 1)..lines.len() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        if leading_spaces(line) <= block_indent {
            return idx;
        }
    }
    lines.len()
}

pub(crate) fn auth_sessions_has_child(source: &str, block: AuthSessionsBlock, keyword: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for idx in (block.line_idx + 1)..block.end_line.min(lines.len()) {
        let line = lines[idx];
        if is_trivia_line(line) || leading_spaces(line) <= block.indent {
            continue;
        }
        if line.trim_start().split_whitespace().next() == Some(keyword) {
            return true;
        }
    }
    false
}

pub(crate) fn auth_rotation_has_children(source: &str, rotation: AuthRotationBlock) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for idx in (rotation.line_idx + 1)..lines.len() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= rotation.indent {
            return false;
        }
        return true;
    }
    false
}

pub(crate) fn error_page_value_completions(
    source: &str,
    position: Position,
    before_cursor: &str,
) -> Option<Vec<CompletionItem>> {
    let trimmed = before_cursor.trim_start();
    if let Some(rest) = trimmed.strip_prefix("error_page ") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
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
    }

    if block_kind_at(source, position).as_deref() == Some("error_page") {
        if let Some(rest) = trimmed.strip_prefix("audience ") {
            if rest
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
        }
    }

    None
}

/// Scan `source` for declared names under the namespace pointed to by
/// the immediately-preceding `@<ns>.` marker before the cursor. Returns
/// `None` when the cursor isn't sitting on such a marker.
///
/// Supported namespaces and how their names are collected from text:
/// - `@policy.<name>` → keys on lines inside a `policies` block
///   (e.g. `create: @role.admin` contributes `create`).
/// - `@validator.<name>` / `@fn.<name>` / `@hook.<name>` →
///   `<keyword> <name>` lines inside an `extensions` block.
/// - `@role.<name>` / `@scope.<name>` / `@actor.<name>` → atom
///   tokens used anywhere in the policies block (declared catalogs
///   live in `app.lzi` policy blocks; the LSP file-locally surfaces
///   what appears in this document).
pub(crate) fn namespace_prefix_completions(source: &str, before_cursor: &str) -> Option<Vec<CompletionItem>> {
    // The token under construction is the run of word chars at the
    // end of `before_cursor`; everything before it should end with
    // `@<ns>.`.
    let token_start = before_cursor
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &before_cursor[..token_start];
    let trimmed_prefix = prefix.trim_end();
    let ns = if trimmed_prefix.ends_with("@policy.") {
        "policy"
    } else if trimmed_prefix.ends_with("@validator.") {
        "validator"
    } else if trimmed_prefix.ends_with("@fn.") {
        "fn"
    } else if trimmed_prefix.ends_with("@hook.") {
        "hook"
    } else if trimmed_prefix.ends_with("@role.") {
        "role"
    } else if trimmed_prefix.ends_with("@scope.") {
        "scope"
    } else if trimmed_prefix.ends_with("@actor.") {
        "actor"
    } else {
        return None;
    };

    let names = collect_namespace_names(source, ns);
    if names.is_empty() {
        return Some(Vec::new());
    }
    Some(
        names
            .into_iter()
            .map(|name| CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::REFERENCE),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Collect declared names for a closed-namespace prefix by scanning
/// the document. Cheap text-based scan rather than a full IR walk —
/// matches the existing LSP convention used elsewhere in this crate.
pub(crate) fn collect_namespace_names(source: &str, ns: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let lines: Vec<&str> = source.lines().collect();

    match ns {
        "policy" => {
            // Walk `policies` blocks: each child line of form
            // `<name>: <atoms>` declares one feature-local category.
            let mut inside_policies = false;
            let mut block_indent: usize = 0;
            for line in &lines {
                let trimmed = line.trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                let indent = leading_spaces(line);
                if trimmed == "policies" || trimmed.starts_with("policies ") {
                    inside_policies = true;
                    block_indent = indent;
                    continue;
                }
                if inside_policies {
                    if indent <= block_indent {
                        inside_policies = false;
                        continue;
                    }
                    // Child line: `<name>: ...` or `policy_for ...`.
                    if let Some(colon) = trimmed.find(':') {
                        let name = trimmed[..colon].trim();
                        if !name.is_empty()
                            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                            && seen.insert(name.to_owned())
                        {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
        "validator" | "fn" | "hook" => {
            // Walk `extensions` blocks: child lines of form
            // `<kind> <name>: ...` declare an extension.
            let mut inside_ext = false;
            let mut block_indent: usize = 0;
            for line in &lines {
                let trimmed = line.trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                let indent = leading_spaces(line);
                if trimmed == "extensions" {
                    inside_ext = true;
                    block_indent = indent;
                    continue;
                }
                if inside_ext {
                    if indent <= block_indent {
                        inside_ext = false;
                        continue;
                    }
                    let kind_prefix = format!("{ns} ");
                    if let Some(rest) = trimmed.strip_prefix(&kind_prefix) {
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                }
            }
        }
        "role" | "scope" | "actor" => {
            // Scan every `@<ns>.<name>` token already used in the
            // document. This is best-effort surfacing rather than a
            // declaration index.
            let needle = format!("@{ns}.");
            for line in &lines {
                let mut rest: &str = line;
                while let Some(idx) = rest.find(&needle) {
                    let after = &rest[idx + needle.len()..];
                    let name_len = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .map(|c| c.len_utf8())
                        .sum::<usize>();
                    if name_len > 0 {
                        let name = after[..name_len].to_owned();
                        if seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                    if name_len == 0 {
                        // No more tokens on this line — break to
                        // avoid an infinite loop on bare `@ns.`.
                        break;
                    }
                    rest = &after[name_len..];
                }
            }
        }
        _ => {}
    }
    names
}

// ── IR Route-Guards — LSP completion / hover / code actions ────────────────

pub(crate) const ROUTE_GUARD_DEFAULT_CLAUSES: &[&str] = &[
    "default_policy",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
];

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardViewBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
    pub(crate) feature_hint: Option<String>,
}

/// Completion for `ir-route-guards` authoring positions. The helper is
/// intentionally text-walk based, matching the existing LSP convention:
/// it gives immediate editor help even before parser/analyzer cells know
/// the new surface.
pub fn route_guard_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();

    if route_guard_redirect_path_trigger(trimmed_before).is_some() {
        return Some(route_path_completion_items(
            source,
            redirect_trigger_has_open_quote(trimmed_before),
        ));
    }

    if let Some(rest) = trimmed_before.strip_prefix("actor_query ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            && at_app_child_completion_line(source, position)
        {
            return Some(query_ref_completion_items(source));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("default_policy ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '.')
            && in_app_route_guard_block(source, position).is_some()
        {
            let feature = route_guard_context_feature(source, position);
            return Some(policy_ref_completion_items(
                source,
                feature.as_deref(),
                true,
            ));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("policy ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '.')
            && in_view_or_audience_guard_context(source, position)
        {
            let feature = route_guard_context_feature(source, position);
            return Some(policy_ref_completion_items(
                source,
                feature.as_deref(),
                true,
            ));
        }
    }

    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word = !trimmed_before.is_empty()
        && trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !(is_blank_indented || is_partial_word) {
        return None;
    }

    if in_app_route_guard_block(source, position).is_some() {
        return Some(route_guard_default_clause_completion_items());
    }

    if in_guard_policy_child_context(source, position) {
        return Some(vec![
            snippet_completion(
                "on_unauthenticated redirect",
                "on_unauthenticated redirect \"${1:/sign-in}\"",
                "Redirect when no actor is signed in.",
            ),
            snippet_completion(
                "on_unauthorized redirect",
                "on_unauthorized redirect \"${1:/403}\"",
                "Redirect when a signed-in actor fails the policy.",
            ),
        ]);
    }

    if in_view_or_audience_guard_context(source, position) {
        return Some(vec![snippet_completion(
            "policy @policy.<name>",
            "policy @policy.${1:name}\n  on_unauthenticated redirect \"${2:/sign-in}\"\n  on_unauthorized redirect \"${3:/403}\"",
            "Declare a route guard policy and per-view redirects.",
        )]);
    }

    if at_app_child_completion_line(source, position) {
        return Some(vec![
            snippet_completion(
                "route_guard",
                "route_guard\n  default_policy @scope.authenticated\n  default_unauthenticated_redirect \"${1:/sign-in}\"\n  default_unauthorized_redirect \"${2:/403}\"",
                "Declare app-level route guard defaults.",
            ),
            snippet_completion(
                "actor_query <feature>.query.<name>",
                "actor_query ${1:account.query.me}",
                "Wire the query that resolves the active actor.",
            ),
        ]);
    }

    None
}

pub(crate) fn route_guard_redirect_path_trigger(trimmed_before: &str) -> Option<&'static str> {
    let triggers = [
        "on_unauthenticated redirect ",
        "on_unauthorized redirect ",
        "default_unauthenticated_redirect ",
        "default_unauthorized_redirect ",
    ];
    triggers
        .into_iter()
        .find(|trigger| trimmed_before.starts_with(trigger))
}

pub(crate) fn redirect_trigger_has_open_quote(trimmed_before: &str) -> bool {
    route_guard_redirect_path_trigger(trimmed_before)
        .map(|trigger| trimmed_before[trigger.len()..].starts_with('"'))
        .unwrap_or(false)
}

pub(crate) fn route_path_completion_items(source: &str, open_quote: bool) -> Vec<CompletionItem> {
    collect_route_paths(source)
        .into_iter()
        .map(|path| CompletionItem {
            label: path.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("Declared route path.".to_owned()),
            insert_text: Some(if open_quote {
                path
            } else {
                format!("\"{path}\"")
            }),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn query_ref_completion_items(source: &str) -> Vec<CompletionItem> {
    collect_query_refs(source)
        .into_iter()
        .map(|query_ref| CompletionItem {
            label: query_ref,
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("Declared query usable as `actor_query`.".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn policy_ref_completion_items(
    source: &str,
    feature_hint: Option<&str>,
    include_atom_prefixes: bool,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    if include_atom_prefixes {
        for prefix in ["@policy.", "@scope.", "@role.", "@actor."] {
            items.push(CompletionItem {
                label: prefix.to_owned(),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some("Route guard policy reference prefix.".to_owned()),
                ..CompletionItem::default()
            });
        }
    }
    items.extend(
        collect_policy_categories_for_feature(source, feature_hint)
            .into_iter()
            .map(|name| CompletionItem {
                label: format!("@policy.{name}"),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some("Feature-local policy category.".to_owned()),
                ..CompletionItem::default()
            }),
    );
    items
}

pub(crate) fn route_guard_default_clause_completion_items() -> Vec<CompletionItem> {
    ROUTE_GUARD_DEFAULT_CLAUSES
        .iter()
        .map(|clause| match *clause {
            "default_policy" => snippet_completion(
                "default_policy",
                "default_policy @scope.authenticated",
                "Fallback policy for unguarded routes.",
            ),
            "default_unauthenticated_redirect" => snippet_completion(
                "default_unauthenticated_redirect",
                "default_unauthenticated_redirect \"${1:/sign-in}\"",
                "Fallback redirect when no actor is signed in.",
            ),
            "default_unauthorized_redirect" => snippet_completion(
                "default_unauthorized_redirect",
                "default_unauthorized_redirect \"${1:/403}\"",
                "Fallback redirect when a signed-in actor fails policy.",
            ),
            _ => CompletionItem::default(),
        })
        .collect()
}

pub(crate) fn snippet_completion(label: &str, body: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(body.to_owned()),
        insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

pub fn route_guard_hover(source: &str, position: Position, word: &str) -> Option<String> {
    if word.starts_with("policy.") {
        if let Some(hover) = route_guard_policy_ref_hover(source, position, word) {
            return Some(hover);
        }
    }

    match word {
        "policy" if in_view_or_audience_guard_context(source, position) => {
            let layer = if enclosing_audience_block(source, position).is_some()
                && enclosing_view_block(source, position).is_none()
            {
                "Per-audience default route guard inherited by views unless they declare their own `policy`."
            } else {
                "Per-view route guard evaluated on every navigation to this view."
            };
            Some(format!(
                "`policy`\n\n{layer} Redirects use `on_unauthenticated` and `on_unauthorized`."
            ))
        }
        "on_unauthenticated" => Some(
            "`on_unauthenticated`\n\nRedirect target when the active actor is not signed in. Falls back through view, audience, then app defaults."
                .to_owned(),
        ),
        "on_unauthorized" => Some(
            "`on_unauthorized`\n\nRedirect target when the signed-in actor fails the guard policy. Falls back through view, audience, then app defaults."
                .to_owned(),
        ),
        "route_guard" => Some(
            "`route_guard`\n\nApp-level fallback route guard block carrying `default_policy`, `default_unauthenticated_redirect`, and `default_unauthorized_redirect`."
                .to_owned(),
        ),
        "actor_query" => Some(
            "`actor_query`\n\nApp-level `<feature>.query.<name>` reference used by the runtime SDK to resolve the current actor for route guards."
                .to_owned(),
        ),
        "default_unauthenticated_redirect" => Some(
            "`default_unauthenticated_redirect`\n\nInside `app.route_guard`, fallback path for unauthenticated users when a view or audience does not override it."
                .to_owned(),
        ),
        "default_unauthorized_redirect" => Some(
            "`default_unauthorized_redirect`\n\nInside `app.route_guard`, fallback path for signed-in users who fail a guard policy when no narrower layer overrides it."
                .to_owned(),
        ),
        _ => None,
    }
}

pub(crate) fn route_guard_policy_ref_hover(source: &str, position: Position, word: &str) -> Option<String> {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let policy_ref = format!("@{word}");
    if !line.contains(&policy_ref) || !in_view_or_audience_guard_context(source, position) {
        return None;
    }
    let feature = route_guard_context_feature(source, position);
    let (atoms, source_label) = resolve_policy_atoms(source, feature.as_deref(), &policy_ref)
        .unwrap_or_else(|| {
            (
                Vec::new(),
                "unresolved policy category in this document".to_owned(),
            )
        });
    let atoms_text = if atoms.is_empty() {
        "unresolved".to_owned()
    } else {
        atoms
            .iter()
            .map(|atom| format!("`{atom}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let alignment = route_guard_backend_alignment(source, position, &policy_ref);
    Some(
        [
            format!("**`{policy_ref}`** — route guard policy reference."),
            String::new(),
            format!("**Resolved atoms**: {atoms_text}"),
            String::new(),
            format!("**Source**: {source_label}"),
            String::new(),
            format!("**Backend alignment**: {alignment}"),
        ]
        .join("\n"),
    )
}


pub(crate) fn collect_route_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("path ") else {
            continue;
        };
        if let Some(path) = first_quoted_value(rest) {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}



pub(crate) fn first_quoted_value(value: &str) -> Option<String> {
    let open = value.find('"')?;
    let rest = &value[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_owned())
}

pub(crate) fn route_guard_context_feature(source: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            for prefix in ["feature ", "surface ", "experience "] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        return Some(name.to_owned());
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn in_app_body_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            return trimmed.starts_with("app ");
        }
        if indent < cursor_indent && trimmed.starts_with("route_guard") {
            return false;
        }
    }
    false
}

pub(crate) fn at_app_child_completion_line(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    leading_spaces(line) == 2 && in_app_body_context(source, position)
}

pub(crate) fn in_app_route_guard_block(source: &str, position: Position) -> Option<RouteGuardBlock> {
    let block = app_route_guard_block(source)?;
    let line_idx = position.line as usize;
    let line = source.lines().nth(line_idx).unwrap_or("");
    let indent = leading_spaces(line);
    if line_idx > block.header_line && line_idx < block.end_line && indent > block.header_indent {
        Some(block)
    } else {
        None
    }
}

pub(crate) fn app_route_guard_block(source: &str) -> Option<RouteGuardBlock> {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed == "route_guard" || trimmed.starts_with("route_guard ") {
            let header_indent = leading_spaces(line);
            let end_line = find_block_end(&lines, idx, header_indent);
            return Some(RouteGuardBlock {
                header_line: idx,
                header_indent,
                end_line,
            });
        }
    }
    None
}

pub(crate) fn in_view_or_audience_guard_context(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let trimmed = line.trim_start();
    if trimmed.starts_with("policy ") {
        return enclosing_view_block(source, position).is_some()
            || enclosing_audience_block(source, position).is_some();
    }
    enclosing_view_block(source, position).is_some()
        || enclosing_audience_block(source, position).is_some()
}

pub(crate) fn in_guard_policy_child_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with("policy ") {
            let pos = Position {
                line: idx as u32,
                character: indent as u32,
            };
            return in_view_or_audience_guard_context(source, pos);
        }
        return false;
    }
    false
}

pub(crate) fn enclosing_view_block(source: &str, position: Position) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "view")
}

pub(crate) fn enclosing_audience_block(source: &str, position: Position) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "audience")
}

pub(crate) fn enclosing_named_block(
    source: &str,
    position: Position,
    keyword: &str,
) -> Option<RouteGuardViewBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if idx == cursor_line_idx {
            if !trimmed.starts_with(&format!("{keyword} ")) {
                continue;
            }
        } else if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with(&format!("{keyword} ")) {
            let end_line = find_block_end(&lines, idx, indent);
            return Some(RouteGuardViewBlock {
                header_line: idx,
                header_indent: indent,
                end_line,
                feature_hint: route_guard_context_feature(
                    source,
                    Position {
                        line: idx as u32,
                        character: indent as u32,
                    },
                ),
            });
        }
        if indent == 0 {
            return None;
        }
    }
    None
}

pub(crate) fn find_block_end(lines: &[&str], header_line: usize, header_indent: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(header_line + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= header_indent {
            return idx;
        }
    }
    lines.len()
}

pub(crate) fn resolve_policy_atoms(
    source: &str,
    feature_hint: Option<&str>,
    policy_ref: &str,
) -> Option<(Vec<String>, String)> {
    let name = policy_ref.strip_prefix("@policy.")?;
    let (feature, category) = if let Some((feature, category)) = name.split_once('.') {
        (Some(feature), category)
    } else {
        (feature_hint, name)
    };
    let mut current_feature: Option<String> = None;
    let mut in_policies = false;
    let mut policies_indent = 0;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            in_policies = false;
            continue;
        }
        let feature_matches = match (feature, current_feature.as_deref()) {
            (Some(expected), Some(current)) => expected == current,
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if !feature_matches {
            continue;
        }
        if trimmed == "policies" || trimmed.starts_with("policies ") {
            in_policies = true;
            policies_indent = indent;
            continue;
        }
        if in_policies {
            if indent <= policies_indent {
                in_policies = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix(&format!("{category}:")) {
                let atoms = rest
                    .split(',')
                    .map(str::trim)
                    .flat_map(|part| part.split_whitespace())
                    .filter(|token| token.starts_with('@'))
                    .map(|token| token.trim_end_matches(',').to_owned())
                    .collect::<Vec<_>>();
                let source_feature = current_feature.as_deref().unwrap_or("<unknown>");
                return Some((
                    atoms,
                    format!("`feature.{source_feature}.policies.{category}`"),
                ));
            }
        }
    }
    None
}

pub(crate) fn route_guard_backend_alignment(source: &str, position: Position, policy_ref: &str) -> String {
    let Some(view) = enclosing_view_block(source, position) else {
        return "No enclosing view found.".to_owned();
    };
    let hosted = hosted_backend_refs_for_view(source, &view);
    if hosted.is_empty() {
        return "No hosted `source` or `submit` backend found in this view.".to_owned();
    }
    let mut mismatches = Vec::new();
    for backend_ref in hosted {
        if let Some(backend_policy) = backend_policy_for_ref(source, &backend_ref) {
            if backend_policy == policy_ref {
                return format!(
                    "view hosts `{backend_ref}` (policy `{backend_policy}`); guard matches backend."
                );
            }
            mismatches.push(format!("`{backend_ref}` uses `{backend_policy}`"));
        }
    }
    if mismatches.is_empty() {
        "Hosted backend declarations have no local policy line in this document.".to_owned()
    } else {
        format!(
            "guard differs from hosted backend policy: {}.",
            mismatches.join(", ")
        )
    }
}

pub(crate) fn hosted_backend_refs_for_view(source: &str, view: &RouteGuardViewBlock) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut refs = Vec::new();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        let trimmed = line.trim_start();
        for prefix in ["source ", "submit "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let reference = rest.split_whitespace().next().unwrap_or("");
                let reference = reference.split('(').next().unwrap_or(reference);
                if reference.contains(".query.") || reference.contains(".command.") {
                    refs.push(reference.to_owned());
                }
            }
        }
    }
    refs
}

pub(crate) fn backend_policy_for_ref(source: &str, backend_ref: &str) -> Option<String> {
    let parts: Vec<&str> = backend_ref.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let feature = parts[0];
    let kind = parts[1];
    let name = parts[2];
    let lines: Vec<&str> = source.lines().collect();
    let mut in_feature = false;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature)
                .unwrap_or(false);
            continue;
        }
        if !in_feature || indent != 2 {
            continue;
        }
        let declaration_matches = match kind {
            "command" => trimmed
                .strip_prefix("command ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
                .unwrap_or(false),
            "query" => ["query.list ", "query.lookup ", "query.sql ", "query.view "]
                .iter()
                .any(|prefix| {
                    trimmed
                        .strip_prefix(prefix)
                        .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
                        .unwrap_or(false)
                }),
            _ => false,
        };
        if !declaration_matches {
            continue;
        }
        let end = find_block_end(&lines, idx, indent);
        for child in lines.iter().take(end).skip(idx + 1) {
            let child_trimmed = child.trim_start();
            if let Some(rest) = child_trimmed.strip_prefix("policy ") {
                return Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            }
        }
    }
    None
}

pub(crate) fn simple_edit_action(
    uri: &Url,
    title: &str,
    kind: CodeActionKind,
    edits: Vec<TextEdit>,
    preferred: bool,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    CodeAction {
        title: title.to_owned(),
        kind: Some(kind),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(preferred),
        disabled: None,
        data: None,
    }
}

// ── IR Lifecycle Route-Gates — LSP completion / hover / code actions ───────

pub(crate) const LIFECYCLE_REQUIRES_HOVER: &str = "Gate this view on the actor's `<Resource>.lifecycle_state`. Codegen emits a TanStack `beforeLoad` that fetches the source query and redirects via `@resume` on mismatch.";
pub(crate) const LIFECYCLE_PENDING_HOVER: &str = "Name of the `resume <name>` block to redirect through when `requires_lifecycle` doesn't match.";
pub(crate) const LIFECYCLE_RESUME_HOVER: &str = "Block declaring how to route a user whose lifecycle state of a particular resource is mid-flow.";
pub(crate) const LIFECYCLE_SOURCE_QUERY_HOVER: &str = "The lookup query that fetches the actor's row of the resource. Must return a single record OR not-found (404).";
pub(crate) const LIFECYCLE_NONE_HOVER: &str =
    "Arm matched when the source query returns 404 (the actor's row doesn't exist yet).";
pub(crate) const LIFECYCLE_WILDCARD_HOVER: &str = "Catch-all arm. Matches any state not explicitly listed. Required when `resume` arms don't cover every state in the lifecycle, OR for forward-compatibility.";
pub(crate) const LIFECYCLE_ARROW_HOVER: &str = "Arrow token mapping a lifecycle state arm to a target view in a `resume` block. Both Unicode `→` and ASCII `->` accepted.";

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResourceInfo {
    feature: Option<String>,
    name: String,
    states: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleLookupQueryInfo {
    feature: String,
    name: String,
    returns: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResumeBlock {
    pub(crate) name: String,
    pub(crate) feature_hint: Option<String>,
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
    pub(crate) source_query: Option<String>,
    pub(crate) arms: Vec<LifecycleResumeArm>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResumeArm {
    pub(crate) state: String,
    pub(crate) line: usize,
}

pub fn lifecycle_gate_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();

    if let Some(resource) = requires_lifecycle_state_trigger(trimmed_before) {
        let feature = route_guard_context_feature(source, position);
        return Some(lifecycle_state_completion_items(
            source,
            feature.as_deref(),
            resource,
        ));
    }

    if let Some(rest) = trimmed_before.strip_prefix("requires_lifecycle ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = route_guard_context_feature(source, position);
            return Some(lifecycle_resource_completion_items(
                source,
                feature.as_deref(),
            ));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("on_lifecycle_pending @resume ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = route_guard_context_feature(source, position);
            return Some(lifecycle_resume_completion_items(
                source,
                feature.as_deref(),
            ));
        }
    }

    if let Some(resume) = enclosing_lifecycle_resume_block(source, position) {
        if let Some(rest) = trimmed_before.strip_prefix("source query.lookup ") {
            if lifecycle_identifier_prefix(rest) {
                return Some(lifecycle_lookup_query_completion_items(
                    source,
                    resume.feature_hint.as_deref(),
                ));
            }
        }

        if resume_arm_view_target_trigger(trimmed_before) {
            return Some(lifecycle_view_completion_items(
                source,
                resume.feature_hint.as_deref(),
            ));
        }

        if resume_arm_view_keyword_trigger(trimmed_before, &resume) {
            return Some(vec![lifecycle_keyword_completion(
                "view ",
                "view ",
                "Map this lifecycle arm to a target view.",
            )]);
        }

        if resume_header_source_trigger(trimmed_before, &resume, position) {
            return Some(vec![snippet_completion(
                "source query.lookup <q>",
                "source query.lookup ${1:lookup_query}",
                "Choose the lookup query that fetches the actor's lifecycle row.",
            )]);
        }

        if resume_arm_start_trigger(source, before, &resume, position) {
            return Some(lifecycle_resume_arm_completion_items(source, &resume));
        }
    }

    if lifecycle_view_slot_trigger(source, position, before) {
        return Some(vec![
            lifecycle_keyword_completion(
                "requires_lifecycle ",
                "requires_lifecycle ",
                "Gate this view on a resource lifecycle state.",
            ),
            lifecycle_keyword_completion(
                "on_lifecycle_pending @resume ",
                "on_lifecycle_pending @resume ",
                "Redirect lifecycle mismatches through a resume router.",
            ),
        ]);
    }

    None
}

pub(crate) fn lifecycle_keyword_completion(label: &str, insert_text: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(detail.to_owned()),
        insert_text: Some(insert_text.to_owned()),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

pub(crate) fn lifecycle_reference_completion(label: String, detail: &str) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::REFERENCE),
        detail: Some(detail.to_owned()),
        ..CompletionItem::default()
    }
}

pub(crate) fn lifecycle_identifier_prefix(rest: &str) -> bool {
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn requires_lifecycle_state_trigger(trimmed_before: &str) -> Option<&str> {
    let rest = trimmed_before.strip_prefix("requires_lifecycle ")?;
    let (resource, value_part) = rest.split_once('=')?;
    let resource = resource.trim();
    if resource.is_empty()
        || !resource
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    if value_part
        .trim_start()
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(resource)
    } else {
        None
    }
}

pub(crate) fn lifecycle_resource_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_resources(source)
        .into_iter()
        .filter(|resource| {
            lifecycle_feature_is_reachable(source, feature_hint, resource.feature.as_deref())
        })
        .map(|resource| CompletionItem {
            label: resource.name,
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("Resource with a declared lifecycle.".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn lifecycle_state_completion_items(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Vec<CompletionItem> {
    lifecycle_resource_for_name(source, feature_hint, resource_name)
        .map(|resource| {
            resource
                .states
                .into_iter()
                .map(|state| CompletionItem {
                    label: state,
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some(format!("Declared state of `{resource_name}.lifecycle`.")),
                    ..CompletionItem::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn lifecycle_resume_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .filter(|resume| {
            lifecycle_feature_is_reachable(source, feature_hint, resume.feature_hint.as_deref())
        })
        .map(|resume| {
            let label =
                lifecycle_scoped_label(feature_hint, resume.feature_hint.as_deref(), &resume.name);
            lifecycle_reference_completion(label, "Declared `resume <name>` router.")
        })
        .collect()
}

pub(crate) fn lifecycle_lookup_query_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_lookup_queries(source)
        .into_iter()
        .filter(|query| lifecycle_feature_is_reachable(source, feature_hint, Some(&query.feature)))
        .map(|query| {
            let label = lifecycle_scoped_label(feature_hint, Some(&query.feature), &query.name);
            lifecycle_reference_completion(label, "Declared `query.lookup` source.")
        })
        .collect()
}

pub(crate) fn lifecycle_view_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_view_names(source, feature_hint)
        .into_iter()
        .map(|view| lifecycle_reference_completion(view, "Declared view in this experience."))
        .collect()
}

pub(crate) fn lifecycle_resume_arm_completion_items(
    source: &str,
    resume: &LifecycleResumeBlock,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let consumed: HashSet<String> = resume.arms.iter().map(|arm| arm.state.clone()).collect();
    items.push(CompletionItem {
        label: "none".to_owned(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some("No-row lifecycle arm.".to_owned()),
        ..CompletionItem::default()
    });
    if let Some(resource) = lifecycle_resource_for_resume(source, resume) {
        for state in resource.states {
            if !consumed.contains(&state) {
                items.push(CompletionItem {
                    label: state,
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("Unconsumed lifecycle state.".to_owned()),
                    ..CompletionItem::default()
                });
            }
        }
    }
    items.push(CompletionItem {
        label: "*".to_owned(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some("Wildcard lifecycle arm.".to_owned()),
        ..CompletionItem::default()
    });
    items
}

pub(crate) fn lifecycle_scoped_label(
    context_feature: Option<&str>,
    item_feature: Option<&str>,
    name: &str,
) -> String {
    match (context_feature, item_feature) {
        (Some(context), Some(feature)) if context != feature => format!("{feature}.{name}"),
        (None, Some(feature)) => format!("{feature}.{name}"),
        _ => name.to_owned(),
    }
}

pub(crate) fn resume_header_source_trigger(
    trimmed_before: &str,
    resume: &LifecycleResumeBlock,
    position: Position,
) -> bool {
    let cursor_line = position.line as usize;
    if cursor_line == resume.header_line && trimmed_before.starts_with("resume ") {
        return true;
    }
    resume.source_query.is_none()
        && cursor_line > resume.header_line
        && (trimmed_before.is_empty()
            || trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'))
}

pub(crate) fn resume_arm_start_trigger(
    source: &str,
    before: &str,
    resume: &LifecycleResumeBlock,
    position: Position,
) -> bool {
    if position.line as usize <= resume.header_line {
        return false;
    }
    let trimmed_before = before.trim_start();
    let line_indent = source
        .lines()
        .nth(position.line as usize)
        .map(leading_spaces)
        .unwrap_or_else(|| leading_spaces(before));
    line_indent == resume.header_indent + 2
        && (trimmed_before.is_empty()
            || trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '*'))
}

pub(crate) fn resume_arm_view_keyword_trigger(trimmed_before: &str, resume: &LifecycleResumeBlock) -> bool {
    let state = trimmed_before.split_whitespace().next().unwrap_or("");
    if !lifecycle_resume_arm_state_known(state, resume) {
        return false;
    }
    if trimmed_before.ends_with(' ') && trimmed_before.split_whitespace().count() == 1 {
        return true;
    }
    if let Some(after_arrow) = lifecycle_after_arrow(trimmed_before) {
        return after_arrow
            .trim_start()
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '_');
    }
    false
}

pub(crate) fn resume_arm_view_target_trigger(trimmed_before: &str) -> bool {
    let Some(after_arrow) = lifecycle_after_arrow(trimmed_before) else {
        return false;
    };
    let after_arrow = after_arrow.trim_start();
    let Some(rest) = after_arrow.strip_prefix("view ") else {
        return false;
    };
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn lifecycle_after_arrow(trimmed_before: &str) -> Option<&str> {
    if let Some(index) = trimmed_before.rfind("->") {
        return Some(&trimmed_before[index + 2..]);
    }
    if let Some(index) = trimmed_before.rfind('→') {
        return Some(&trimmed_before[index + '→'.len_utf8()..]);
    }
    None
}

pub(crate) fn lifecycle_resume_arm_state_known(state: &str, resume: &LifecycleResumeBlock) -> bool {
    !state.is_empty()
        && (state == "none"
            || state == "*"
            || resume.arms.iter().any(|arm| arm.state == state)
            || state.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

pub(crate) fn lifecycle_view_slot_trigger(source: &str, position: Position, before: &str) -> bool {
    if enclosing_lifecycle_resume_block(source, position).is_some() {
        return false;
    }
    let trimmed_before = before.trim_start();
    if !(trimmed_before.is_empty()
        || trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'))
    {
        return false;
    }
    let Some(view) = enclosing_view_block(source, position) else {
        return false;
    };
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    if leading_spaces(line) != view.header_indent + 2 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .enumerate()
        .take(position.line as usize)
        .skip(view.header_line + 1)
        .any(|(_, line)| {
            leading_spaces(line) == view.header_indent + 2
                && matches!(
                    line.trim_start().split_whitespace().next(),
                    Some("policy" | "path" | "submit")
                )
        })
}

pub fn lifecycle_gate_hover(
    source: &str,
    position: Position,
    word: Option<&str>,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    if lifecycle_hover_is_arrow(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`→` / `->`\n\n{LIFECYCLE_ARROW_HOVER}"));
    }

    if lifecycle_hover_is_wildcard(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`*`\n\n{LIFECYCLE_WILDCARD_HOVER}"));
    }

    let word = word?;
    if word == "source" || word == "query.lookup" {
        if line.trim_start().starts_with("source query.lookup ")
            && enclosing_lifecycle_resume_block(source, position).is_some()
        {
            return Some(format!(
                "`source query.lookup`\n\n{LIFECYCLE_SOURCE_QUERY_HOVER}"
            ));
        }
    }

    match word {
        "requires_lifecycle" if enclosing_view_block(source, position).is_some() => Some(format!(
            "`requires_lifecycle`\n\n{LIFECYCLE_REQUIRES_HOVER}"
        )),
        "on_lifecycle_pending" if enclosing_view_block(source, position).is_some() => Some(
            format!("`on_lifecycle_pending`\n\n{LIFECYCLE_PENDING_HOVER}"),
        ),
        "resume" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`resume`\n\n{LIFECYCLE_RESUME_HOVER}"))
        }
        "none" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`none`\n\n{LIFECYCLE_NONE_HOVER}"))
        }
        _ => lifecycle_resolved_gate_hover(source, position, word),
    }
}

pub(crate) fn lifecycle_hover_is_wildcard(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let bytes = line.as_bytes();
    (index < bytes.len() && bytes[index] == b'*') || (index > 0 && bytes[index - 1] == b'*')
}

pub(crate) fn lifecycle_hover_is_arrow(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let before = &line[..index.min(line.len())];
    let after = &line[index.min(line.len())..];
    before.ends_with("->")
        || after.starts_with("->")
        || before.ends_with('→')
        || after.starts_with('→')
}

pub(crate) fn lifecycle_resolved_gate_hover(source: &str, position: Position, word: &str) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("requires_lifecycle ")?;
    let (resource, state) = rest.split_once('=')?;
    let resource = resource.trim();
    let state = state.split_whitespace().next().unwrap_or("");
    if word != state || state.is_empty() {
        return None;
    }
    let view = enclosing_view_block(source, position)?;
    let resume_name =
        lifecycle_pending_resume_for_view(source, &view).unwrap_or_else(|| "<resume>".to_owned());
    let states = lifecycle_resource_for_name(source, view.feature_hint.as_deref(), resource)
        .map(|resource| resource.states.join(", "))
        .unwrap_or_else(|| "unresolved".to_owned());
    Some(format!(
        "currently the view requires `{resource}.lifecycle_state = {state}`. On mismatch, redirects via `resume {resume_name}`. Lifecycle states declared: `{states}`."
    ))
}


#[derive(Debug, Clone)]
pub(crate) struct LifecycleGateCandidate {
    pub(crate) resource: String,
    pub(crate) state: String,
}

pub(crate) fn lifecycle_missing_resume_states(source: &str, resume: &LifecycleResumeBlock) -> Vec<String> {
    if resume.arms.iter().any(|arm| arm.state == "*") {
        return Vec::new();
    }
    let Some(resource) = lifecycle_resource_for_resume(source, resume) else {
        return Vec::new();
    };
    let consumed: HashSet<&str> = resume.arms.iter().map(|arm| arm.state.as_str()).collect();
    resource
        .states
        .into_iter()
        .filter(|state| !consumed.contains(state.as_str()))
        .collect()
}

pub(crate) fn lifecycle_stale_resume_arm_on_line(
    source: &str,
    resume: &LifecycleResumeBlock,
    line_idx: usize,
) -> Option<LifecycleResumeArm> {
    let resource = lifecycle_resource_for_resume(source, resume)?;
    let states: HashSet<&str> = resource.states.iter().map(String::as_str).collect();
    resume
        .arms
        .iter()
        .find(|arm| {
            arm.line == line_idx
                && arm.state != "none"
                && arm.state != "*"
                && !states.contains(arm.state.as_str())
        })
        .cloned()
}

pub(crate) fn lifecycle_resume_arm_insertion_line(resume: &LifecycleResumeBlock) -> usize {
    resume
        .arms
        .iter()
        .map(|arm| arm.line + 1)
        .max()
        .unwrap_or_else(|| resume.end_line)
}

pub(crate) fn lifecycle_gate_candidate_for_view(
    source: &str,
    view: &RouteGuardViewBlock,
) -> Option<LifecycleGateCandidate> {
    for resource in lifecycle_resources_hosted_by_view(source, view) {
        let state = lifecycle_state_from_view_path(source, view, &resource).or_else(|| {
            lifecycle_default_gate_state(source, view.feature_hint.as_deref(), &resource)
        })?;
        return Some(LifecycleGateCandidate { resource, state });
    }
    None
}

pub(crate) fn lifecycle_resources_hosted_by_view(source: &str, view: &RouteGuardViewBlock) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut resources = Vec::new();
    let mut seen = HashSet::new();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        let trimmed = line.trim_start();
        let hosted = if let Some(rest) = trimmed.strip_prefix("source ") {
            lifecycle_resource_for_source_ref(source, view.feature_hint.as_deref(), rest)
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            lifecycle_resource_for_submit_ref(source, view.feature_hint.as_deref(), rest)
        } else {
            None
        };
        if let Some(resource) = hosted {
            if lifecycle_resource_for_name(source, view.feature_hint.as_deref(), &resource)
                .is_some()
                && seen.insert(resource.clone())
            {
                resources.push(resource);
            }
        }
    }
    resources
}

pub(crate) fn lifecycle_resource_for_source_ref(
    source: &str,
    feature_hint: Option<&str>,
    rest: &str,
) -> Option<String> {
    let mut tokens = rest.split_whitespace();
    let first = tokens.next()?;
    let query_ref = if first == "query.lookup" {
        tokens.next()?.to_owned()
    } else {
        first.split('(').next().unwrap_or(first).to_owned()
    };
    resolve_lifecycle_lookup_query(source, feature_hint, &query_ref)?.returns
}

pub(crate) fn lifecycle_resource_for_submit_ref(
    source: &str,
    feature_hint: Option<&str>,
    rest: &str,
) -> Option<String> {
    let command_ref = rest
        .split_whitespace()
        .next()?
        .split('(')
        .next()
        .unwrap_or("");
    resolve_lifecycle_command_resource(source, feature_hint, command_ref)
}

pub(crate) fn lifecycle_default_gate_state(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<String> {
    let resource = lifecycle_resource_for_name(source, feature_hint, resource_name)?;
    resource
        .states
        .iter()
        .find(|state| state.as_str() == "complete")
        .cloned()
        .or_else(|| resource.states.first().cloned())
}

pub(crate) fn lifecycle_state_from_view_path(
    source: &str,
    view: &RouteGuardViewBlock,
    resource_name: &str,
) -> Option<String> {
    let path = lifecycle_view_path(source, view)?;
    let segments = path
        .trim_matches('"')
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    if segments.len() < 3 || segments.first().copied() != Some("onboarding") {
        return None;
    }
    let resource_slug = slug_for_lifecycle_token(resource_name);
    if segments.get(1).copied() != Some(resource_slug.as_str()) {
        return None;
    }
    let state_slug = segments[2..].join("-");
    let resource =
        lifecycle_resource_for_name(source, view.feature_hint.as_deref(), resource_name)?;
    resource.states.into_iter().find(|state| {
        let slug = slug_for_lifecycle_token(state);
        slug == state_slug || slug.strip_suffix("-pending") == Some(state_slug.as_str())
    })
}

pub(crate) fn lifecycle_view_path(source: &str, view: &RouteGuardViewBlock) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) == view.header_indent + 2 {
            if let Some(rest) = line.trim_start().strip_prefix("path ") {
                let trimmed = rest.trim();
                return first_quoted_value(trimmed).or_else(|| {
                    trimmed
                        .split_whitespace()
                        .next()
                        .filter(|path| path.starts_with('/'))
                        .map(str::to_owned)
                });
            }
        }
    }
    None
}

pub(crate) fn lifecycle_gate_insertion_line(source: &str, view: &RouteGuardViewBlock) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    let mut fallback = view.header_line + 1;
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(view.end_line)
        .skip(view.header_line + 1)
    {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("policy ") {
            return idx + 1;
        }
        if trimmed.starts_with("path ") {
            fallback = idx + 1;
        }
    }
    fallback
}

pub(crate) fn view_has_requires_lifecycle(source: &str, view: &RouteGuardViewBlock) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .take(view.end_line)
        .skip(view.header_line + 1)
        .any(|line| {
            leading_spaces(line) == view.header_indent + 2
                && line.trim_start().starts_with("requires_lifecycle ")
        })
}

pub(crate) fn lifecycle_pending_resume_for_view(source: &str, view: &RouteGuardViewBlock) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        if let Some(rest) = line
            .trim_start()
            .strip_prefix("on_lifecycle_pending @resume ")
        {
            return Some(rest.split_whitespace().next()?.to_owned());
        }
    }
    None
}

pub(crate) fn lifecycle_resume_for_resource(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<String> {
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .find(|resume| {
            lifecycle_feature_is_reachable(source, feature_hint, resume.feature_hint.as_deref())
                && lifecycle_resource_for_resume(source, resume)
                    .map(|resource| resource.name == resource_name)
                    .unwrap_or(false)
        })
        .map(|resume| {
            lifecycle_scoped_label(feature_hint, resume.feature_hint.as_deref(), &resume.name)
        })
}

pub(crate) fn lifecycle_resource_for_resume(
    source: &str,
    resume: &LifecycleResumeBlock,
) -> Option<LifecycleResourceInfo> {
    let source_query = resume.source_query.as_deref()?;
    let query =
        resolve_lifecycle_lookup_query(source, resume.feature_hint.as_deref(), source_query)?;
    let resource_name = query.returns?;
    lifecycle_resource_for_name(source, resume.feature_hint.as_deref(), &resource_name)
}

pub(crate) fn resolve_lifecycle_lookup_query(
    source: &str,
    feature_hint: Option<&str>,
    query_ref: &str,
) -> Option<LifecycleLookupQueryInfo> {
    let queries = collect_lifecycle_lookup_queries(source);
    let (feature, name) = if let Some((feature, rest)) = query_ref.split_once(".query.") {
        (Some(feature), rest)
    } else if let Some((feature, name)) = query_ref.split_once('.') {
        (Some(feature), name)
    } else {
        (feature_hint, query_ref)
    };
    queries
        .into_iter()
        .find(|query| query.name == name && feature.map(|f| f == query.feature).unwrap_or(true))
}

pub(crate) fn resolve_lifecycle_command_resource(
    source: &str,
    feature_hint: Option<&str>,
    command_ref: &str,
) -> Option<String> {
    let (feature, name) = if let Some((feature, rest)) = command_ref.split_once(".command.") {
        (Some(feature), rest)
    } else if let Some((feature, name)) = command_ref.split_once('.') {
        (Some(feature), name)
    } else {
        (feature_hint, command_ref)
    };
    let lines: Vec<&str> = source.lines().collect();
    let mut current_feature: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            continue;
        }
        let Some(current) = current_feature.as_deref() else {
            continue;
        };
        if feature.map(|f| f != current).unwrap_or(false) {
            continue;
        }
        if !trimmed
            .strip_prefix("command ")
            .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
            .unwrap_or(false)
        {
            continue;
        }
        let end = find_block_end(&lines, idx, leading_spaces(line));
        for child in lines.iter().take(end).skip(idx + 1) {
            let child_trimmed = child.trim_start();
            for prefix in ["creates ", "updates ", "target "] {
                if let Some(rest) = child_trimmed.strip_prefix(prefix) {
                    let resource = rest.split_whitespace().next().unwrap_or("").to_owned();
                    if !resource.is_empty() {
                        return Some(resource);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn lifecycle_resource_for_name(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<LifecycleResourceInfo> {
    collect_lifecycle_resources(source)
        .into_iter()
        .find(|resource| {
            resource.name == resource_name
                && lifecycle_feature_is_reachable(source, feature_hint, resource.feature.as_deref())
        })
}

pub(crate) fn lifecycle_feature_is_reachable(
    source: &str,
    context_feature: Option<&str>,
    candidate_feature: Option<&str>,
) -> bool {
    let Some(candidate) = candidate_feature else {
        return true;
    };
    let Some(context) = context_feature else {
        return true;
    };
    lifecycle_reachable_features(source, context)
        .iter()
        .any(|feature| feature == candidate)
}

pub(crate) fn lifecycle_reachable_features(source: &str, context_feature: &str) -> Vec<String> {
    let mut features = vec![context_feature.to_owned()];
    let mut seen = HashSet::from([context_feature.to_owned()]);
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        if leading_spaces(line) != 0 {
            continue;
        }
        let trimmed = line.trim_start();
        let Some((_, name)) = lifecycle_top_level_named_header(trimmed) else {
            continue;
        };
        if name != context_feature {
            continue;
        }
        let end = find_block_end(&lines, idx, 0);
        for used in lifecycle_uses_in_block(&lines, idx + 1, end) {
            if seen.insert(used.clone()) {
                features.push(used);
            }
        }
    }
    features
}

pub(crate) fn lifecycle_top_level_named_header(trimmed: &str) -> Option<(&str, &str)> {
    for keyword in ["feature", "experience", "surface"] {
        if let Some(rest) = trimmed.strip_prefix(&format!("{keyword} ")) {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                return Some((keyword, name));
            }
        }
    }
    None
}

pub(crate) fn lifecycle_uses_in_block(lines: &[&str], start: usize, end: usize) -> Vec<String> {
    let mut uses = Vec::new();
    let mut seen = HashSet::new();
    for idx in start..end {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "uses" {
            let indent = leading_spaces(line);
            for child in lines.iter().take(end).skip(idx + 1) {
                if child.trim_start().is_empty() || child.trim_start().starts_with('#') {
                    continue;
                }
                if leading_spaces(child) <= indent {
                    break;
                }
                let name = child.trim_start().split_whitespace().next().unwrap_or("");
                if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                    uses.push(name.to_owned());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            for name in lifecycle_parse_uses_rest(rest) {
                if seen.insert(name.clone()) {
                    uses.push(name);
                }
            }
        }
    }
    uses
}

pub(crate) fn lifecycle_parse_uses_rest(rest: &str) -> Vec<String> {
    let mut names = Vec::new();
    for token in rest.replace(',', " ").split_whitespace() {
        if token == "version" {
            break;
        }
        if matches!(token, "feature" | "experience") {
            continue;
        }
        if lifecycle_ident(token) {
            names.push(token.to_owned());
        }
    }
    names
}

pub(crate) fn lifecycle_ident(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && token
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
}

pub(crate) fn collect_lifecycle_resources(source: &str) -> Vec<LifecycleResourceInfo> {
    let lines: Vec<&str> = source.lines().collect();
    let mut resources = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut feature_end = 0usize;
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            feature_end = if current_feature.is_some() {
                find_block_end(&lines, idx, 0)
            } else {
                idx
            };
        }

        if current_feature.is_some() && idx < feature_end {
            if let Some(rest) = trimmed.strip_prefix("resource ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                let end = find_block_end(&lines, idx, leading_spaces(line));
                if let Some(states) = lifecycle_states_in_resource_block(&lines, idx + 1, end) {
                    resources.push(LifecycleResourceInfo {
                        feature: current_feature.clone(),
                        name,
                        states,
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("lifecycle status of ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                let end = find_block_end(&lines, idx, leading_spaces(line));
                let states =
                    lifecycle_state_children(&lines, idx + 1, end, leading_spaces(line) + 2);
                if !name.is_empty() && !states.is_empty() {
                    resources.push(LifecycleResourceInfo {
                        feature: current_feature.clone(),
                        name,
                        states,
                    });
                }
            }
        }
        idx += 1;
    }
    resources
}

pub(crate) fn lifecycle_states_in_resource_block(
    lines: &[&str],
    start: usize,
    end: usize,
) -> Option<Vec<String>> {
    for (idx, line) in lines.iter().enumerate().take(end).skip(start) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("lifecycle ") {
            let states = lifecycle_state_children(lines, idx + 1, end, leading_spaces(line) + 2);
            if !states.is_empty() {
                return Some(states);
            }
        }
    }
    None
}

pub(crate) fn lifecycle_state_children(
    lines: &[&str],
    start: usize,
    end: usize,
    child_indent: usize,
) -> Vec<String> {
    let mut states = Vec::new();
    let mut seen = HashSet::new();
    for line in lines.iter().take(end).skip(start) {
        if leading_spaces(line) != child_indent {
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("state ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                states.push(name.to_owned());
            }
        }
    }
    states
}

pub(crate) fn collect_lifecycle_lookup_queries(source: &str) -> Vec<LifecycleLookupQueryInfo> {
    let lines: Vec<&str> = source.lines().collect();
    let mut queries = Vec::new();
    let mut current_feature: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            continue;
        }
        let Some(feature) = current_feature.as_deref() else {
            continue;
        };
        let Some(rest) = trimmed.strip_prefix("query.lookup ") else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let end = find_block_end(&lines, idx, leading_spaces(line));
        let returns = lines
            .iter()
            .take(end)
            .skip(idx + 1)
            .find_map(|child| {
                child
                    .trim_start()
                    .strip_prefix("returns ")
                    .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
            })
            .filter(|value| !value.is_empty());
        queries.push(LifecycleLookupQueryInfo {
            feature: feature.to_owned(),
            name: name.to_owned(),
            returns,
        });
    }
    queries
}

pub(crate) fn collect_lifecycle_view_names(source: &str, feature_hint: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut current_top: Option<String> = None;
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_top =
                lifecycle_top_level_named_header(trimmed).map(|(_, name)| name.to_owned());
        }
        let context_matches = feature_hint
            .map(|feature| current_top.as_deref() == Some(feature))
            .unwrap_or(true);
        if !context_matches {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("view ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

pub(crate) fn collect_lifecycle_resume_blocks(source: &str) -> Vec<LifecycleResumeBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    let mut current_top: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_top =
                lifecycle_top_level_named_header(trimmed).map(|(_, name)| name.to_owned());
        }
        let Some(rest) = trimmed.strip_prefix("resume ") else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("").to_owned();
        if name.is_empty() {
            continue;
        }
        let header_indent = leading_spaces(line);
        let end_line = find_block_end(&lines, idx, header_indent);
        let mut source_query = None;
        let mut arms = Vec::new();
        for (child_idx, child) in lines.iter().enumerate().take(end_line).skip(idx + 1) {
            if leading_spaces(child) != header_indent + 2 {
                continue;
            }
            let child_trimmed = child.trim_start();
            if let Some(rest) = child_trimmed.strip_prefix("source query.lookup ") {
                source_query = rest
                    .split_whitespace()
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                continue;
            }
            if let Some(arm) = lifecycle_parse_resume_arm(child_trimmed, child_idx) {
                arms.push(arm);
            }
        }
        blocks.push(LifecycleResumeBlock {
            name,
            feature_hint: current_top.clone(),
            header_line: idx,
            header_indent,
            end_line,
            source_query,
            arms,
        });
    }
    blocks
}

pub(crate) fn lifecycle_parse_resume_arm(trimmed: &str, line: usize) -> Option<LifecycleResumeArm> {
    let state = trimmed.split_whitespace().next()?.to_owned();
    if !(state == "none" || state == "*" || lifecycle_ident(&state)) {
        return None;
    }
    let _after_arrow = lifecycle_after_arrow(trimmed)?;
    Some(LifecycleResumeArm { state, line })
}

pub(crate) fn enclosing_lifecycle_resume_block(
    source: &str,
    position: Position,
) -> Option<LifecycleResumeBlock> {
    let line_idx = position.line as usize;
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .find(|block| line_idx >= block.header_line && line_idx < block.end_line)
}

pub(crate) fn slug_for_lifecycle_token(token: &str) -> String {
    let mut slug = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch == '_' || ch == ' ' {
            slug.push('-');
        } else if ch.is_ascii_uppercase() {
            if idx > 0 {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push(ch.to_ascii_lowercase());
        }
    }
    slug
}

pub(crate) fn snake_case(token: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}


/// Inside a `rate_limit "<N> per <window> per "` value, offer the
/// closed axis catalog. Returns `None` outside that context.
pub(crate) fn rate_limit_axis_completions(before_cursor: &str) -> Option<Vec<CompletionItem>> {
    // We must be inside an open double-quoted string on a line whose
    // first non-space token is `rate_limit`.
    let quote_open = before_cursor.rfind('"')?;
    let already_closed = before_cursor[quote_open + 1..].contains('"');
    if already_closed {
        return None;
    }
    let line_prefix = &before_cursor[..quote_open];
    let trimmed_prefix = line_prefix.trim_start();
    if !trimmed_prefix.starts_with("rate_limit") {
        return None;
    }
    let string_so_far = &before_cursor[quote_open + 1..];
    // We expect `<N> per <window> per ` before the cursor; require at
    // least two `per ` occurrences.
    let per_count = string_so_far.matches(" per ").count();
    let ends_with_per_space = string_so_far.ends_with(" per ")
        || string_so_far
            .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
            .ends_with(" per ");
    if per_count < 2 || !ends_with_per_space {
        return None;
    }
    Some(
        RATE_LIMIT_AXES
            .iter()
            .map(|axis| CompletionItem {
                label: (*axis).to_owned(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(match *axis {
                    "ip" => "Per source IP (rate-limit axis).".to_owned(),
                    "user" => "Per authenticated user (rate-limit axis).".to_owned(),
                    "org" => "Per tenant org (rate-limit axis).".to_owned(),
                    "tenant" => "Per generic tenant axis (rate-limit axis).".to_owned(),
                    _ => String::new(),
                }),
                ..CompletionItem::default()
            })
            .collect(),
    )
}


// Catalog detail-lookup functions (`resource_lock_strategy_detail`,
// `error_page_status_detail`, `auth_catalog_detail`,
// `auth_refresh_theft_action_detail`, `observability_catalog_detail`,
// `notification_digest_template_strategy_detail`,
// `deploy_strategy_detail`) now live in `catalogs.rs` and are
// re-exported via `pub use catalogs::*;`.

// IR Error-Vocab closed catalogs (`ERROR_VOCAB_CODES`,
// `ERROR_VOCAB_EXPOSE_4XX_FIELDS`, `ERROR_VOCAB_EXPOSE_5XX_FIELDS`,
// `ERROR_VOCAB_DEFAULT_VALUES`) now live in `catalogs.rs` and are
// re-exported via `pub use catalogs::*;`. See proposal
// `docs/proposals/ir-error-messages-vocab.md` §2.C, §3.4, §7.

/// Scan the source for a `<code> message @translation.<key>` line inside the
/// feature's `errors` block and return the resolved translation text — the
/// **resolved** hover the proposal §7.2 calls for. Falls back to the
/// built-in English string when no feature-level override is found.
///
/// `source` is the full document; `feature_name` is the name of the feature
/// the hover is rendered for; `code` is the closed-catalog error code.
///
/// Resolution chain mirrored here (best-effort, doc-local):
/// 1. `feature.errors.<code> message @translation.<key>` → look up the key
///    in the same feature's `translation` block and return the first locale
///    variant's text.
/// 2. Built-in English fallback (`error_vocab_code_builtin_en_us`).
///
/// The runtime walks a longer chain (per-command, per-policy first); the LSP
/// hover surfaces the **feature-level** resolution because that's the layer
/// authors edit most. The complete resolution table is visible via
/// `lazuli inspect --expand=error-resolution-table`.
pub fn error_vocab_resolved_text(source: &str, feature_name: &str, code: &str) -> Option<String> {
    if let Some(key) = lookup_feature_error_key(source, feature_name, code) {
        if let Some(text) = lookup_translation_first_variant(source, feature_name, &key) {
            return Some(text);
        }
    }
    error_vocab_code_builtin_en_us(code).map(str::to_owned)
}

/// Walk the source to find `feature <name>` ... `errors` ... `<code> message
/// @translation.<key>` and return the key. Indent-based: looks for the
/// matching feature header at indent 0, then the `errors` block at indent 2,
/// then lines at indent 4 of the form `<code> message @translation.<key>`.
pub(crate) fn lookup_feature_error_key(source: &str, feature_name: &str, code: &str) -> Option<String> {
    let mut in_feature = false;
    let mut in_errors = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false);
            in_errors = false;
            continue;
        }
        if !in_feature {
            continue;
        }
        if indent == 2 {
            in_errors = trimmed == "errors";
            continue;
        }
        if in_errors && indent == 4 {
            // `<code> message @translation.<key>`
            let mut tokens = trimmed.split_whitespace();
            let line_code = tokens.next().unwrap_or("");
            if line_code != code {
                continue;
            }
            if tokens.next() != Some("message") {
                continue;
            }
            let r#ref = tokens.next().unwrap_or("");
            if let Some(key) = r#ref.strip_prefix("@translation.") {
                return Some(key.to_owned());
            }
        }
    }
    None
}

/// Find a `key <name>` declaration inside the surrounding feature's
/// `translation` block and return the **first locale variant's text** as a
/// resolved hover string. Best-effort indent-walk parsing — matches the
/// canonical four-space indent layout the rest of the LSP assumes.
pub(crate) fn lookup_translation_first_variant(source: &str, feature_name: &str, key: &str) -> Option<String> {
    let mut in_feature = false;
    let mut in_translation = false;
    let mut in_key = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature_name)
                .unwrap_or(false);
            in_translation = false;
            in_key = false;
            continue;
        }
        if !in_feature {
            continue;
        }
        if indent == 2 {
            in_translation = trimmed == "translation" || trimmed.starts_with("translation ");
            in_key = false;
            continue;
        }
        if !in_translation {
            continue;
        }
        if indent == 4 {
            // `key <name>` line opens a variant block; `catalog "<path>"`
            // sits at the same indent but is a sibling header — skip.
            in_key = trimmed
                .strip_prefix("key ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == key)
                .unwrap_or(false);
            continue;
        }
        if in_key && indent == 6 {
            // `<locale> "<text>"` — extract the text between the first
            // double-quote pair.
            if let Some(open) = trimmed.find('"') {
                let rest = &trimmed[open + 1..];
                if let Some(close) = rest.find('"') {
                    return Some(rest[..close].to_owned());
                }
            }
        }
    }
    None
}

/// Return the feature name the cursor sits inside, or `None` when the
/// cursor is at the top level / inside `app` / `registry`. Best-effort
/// indent-walk: scans backwards from `position.line` for a `feature <name>`
/// header at indent 0.

/// Collect declared `key <name>` entries from inside the named feature's
/// `translation` block. Used by `@translation.<TAB>` completion (proposal
/// §7.1).

/// Compute a completion list for the IR Error-Vocab trigger positions
/// (proposal §7.1). Returns `None` when the cursor is outside any of the 6
/// recognised positions; returns `Some(items)` when a position is matched.
///
/// The 6 trigger positions:
/// 1. After `when_denied ` (on a `policy` line or under a `policies.<cat>:`
///    line) — autocomplete `@translation.<key>` from the local feature's
///    translation block.
/// 2. Inside `feature.errors` block, new indented line — autocomplete the 8
///    closed-catalog codes.
/// 3. After `<code> message ` inside `errors` — autocomplete
///    `@translation.<key>`.
/// 4. After `expose client 4xx ` — autocomplete `message`/`code`/`data`/
///    `message_key`.
/// 5. After `expose client 5xx ` — autocomplete `code`/`data` (no
///    `message`).
/// 6. After `default ` inside `errors` — autocomplete `hide`/`expose`.
pub fn error_vocab_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    let trimmed_before = before.trim_start();

    // Position 1 — `when_denied ` (`@translation.` keys).
    // Fires both on `when_denied ` and `when_denied @translation.` because
    // the namespace prefix completion already handles the post-`@translation.`
    // case via `namespace_prefix_completions` for `@translation`, but this
    // path also lights up the moment the cursor is one space past
    // `when_denied`, regardless of `@translation.` being typed.
    if let Some(rest) = trimmed_before.strip_prefix("when_denied") {
        // Either right after `when_denied ` (single space) or in the middle
        // of typing `@translation.<key>`. Offer the local feature's
        // translation keys (so the user gets exact key names) and, when no
        // `@` has been typed yet, suggest the namespace prefix first.
        let after = rest.trim_start();
        let feature = enclosing_feature_name(source, position)?;
        let keys = collect_translation_keys_for_feature(source, &feature);
        let mut items: Vec<CompletionItem> = Vec::new();
        if after.is_empty() || !after.starts_with('@') {
            items.push(CompletionItem {
                label: "@translation.".to_owned(),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(
                    "Translation key reference — resolves against this feature's `translation` block."
                        .to_owned(),
                ),
                ..CompletionItem::default()
            });
        }
        items.extend(keys.into_iter().map(|key| CompletionItem {
            label: format!("@translation.{key}"),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("Translation key declared in this feature.".to_owned()),
            ..CompletionItem::default()
        }));
        return Some(items);
    }

    // Position 6 — `default ` inside `errors` block (`hide`/`expose`).
    if let Some(rest) = trimmed_before.strip_prefix("default ") {
        if rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && in_feature_errors_block(source, position)
        {
            return Some(
                ERROR_VOCAB_DEFAULT_VALUES
                    .iter()
                    .map(|value| CompletionItem {
                        label: (*value).to_owned(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(match *value {
                            "hide" => {
                                "Errors omit `message` from the wire response by default; opt-in fields go through `expose client 4xx/5xx`."
                                    .to_owned()
                            }
                            "expose" => {
                                "Errors include the closed-catalog exposable fields by default; tighten per class with `expose client 4xx/5xx`."
                                    .to_owned()
                            }
                            _ => String::new(),
                        }),
                        ..CompletionItem::default()
                    })
                    .collect(),
            );
        }
    }

    // Positions 4 + 5 — `expose client 4xx ` / `expose client 5xx `.
    // Trigger when the cursor sits after the third token; offer the
    // per-class closed catalog. We DON'T require the cursor to be inside an
    // `errors` block here because the LSP-level `expose client ...` shape
    // is the same wherever it appears (proposal §2.G).
    if let Some(rest) = trimmed_before.strip_prefix("expose client ") {
        // rest is `4xx ...` or `5xx ...`; we offer completions after the
        // class token, on the field list (closed catalog).
        let mut tokens = rest.split_whitespace();
        let class = tokens.next().unwrap_or("");
        let after_class_idx = rest
            .find(class)
            .map(|i| i + class.len())
            .unwrap_or(rest.len());
        let after_class = &rest[after_class_idx..];
        // Need at least one space after the class token before offering
        // field completions; the post-cursor cursor position then sits in
        // the comma-separated field list.
        if (class == "4xx" || class == "5xx") && after_class.starts_with(' ') {
            let fields = if class == "4xx" {
                ERROR_VOCAB_EXPOSE_4XX_FIELDS
            } else {
                ERROR_VOCAB_EXPOSE_5XX_FIELDS
            };
            return Some(
                fields
                    .iter()
                    .map(|field| CompletionItem {
                        label: (*field).to_owned(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(match *field {
                            "message" => {
                                "Human-readable headline rendered through the resolver chain. Excluded from 5xx."
                                    .to_owned()
                            }
                            "code" => "Stable string code from the closed error catalog.".to_owned(),
                            "data" => {
                                "Structured envelope payload (per-field validator errors, retry hints, etc.).".to_owned()
                            }
                            "message_key" => {
                                "Resolved `@translation.<key>` token — lets clients with offline catalogs localize independently."
                                    .to_owned()
                            }
                            _ => String::new(),
                        }),
                        ..CompletionItem::default()
                    })
                    .collect(),
            );
        }
    }

    // Position 3 — `<code> message ` inside `errors` block.
    if in_feature_errors_block(source, position) {
        let mut tokens = trimmed_before.split_whitespace();
        let first = tokens.next().unwrap_or("");
        let second = tokens.next().unwrap_or("");
        if ERROR_VOCAB_CODES.contains(&first) && second == "message" {
            // Cursor expected to be right after `message ` (with at least
            // one space).
            let head = format!("{first} message");
            if let Some(after_idx) = trimmed_before.find(&head) {
                let after = &trimmed_before[after_idx + head.len()..];
                if after.starts_with(' ') {
                    let feature = enclosing_feature_name(source, position)?;
                    let keys = collect_translation_keys_for_feature(source, &feature);
                    let mut items: Vec<CompletionItem> = vec![CompletionItem {
                        label: "@translation.".to_owned(),
                        kind: Some(CompletionItemKind::SNIPPET),
                        detail: Some(
                            "Translation key reference — resolves against this feature's `translation` block.".to_owned(),
                        ),
                        ..CompletionItem::default()
                    }];
                    items.extend(keys.into_iter().map(|key| CompletionItem {
                        label: format!("@translation.{key}"),
                        kind: Some(CompletionItemKind::REFERENCE),
                        detail: Some("Translation key declared in this feature.".to_owned()),
                        ..CompletionItem::default()
                    }));
                    return Some(items);
                }
            }
        }

        // Position 2 — bare indented line inside `errors` block: offer the
        // 8 closed-catalog codes. Fires when the line is blank (cursor on
        // indented whitespace) or the user has typed a partial alphanumeric
        // prefix that doesn't already include a space (i.e. they haven't
        // moved past the code token yet).
        let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
        let is_partial_code = !trimmed_before.is_empty()
            && trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if is_blank_indented || is_partial_code {
            return Some(
                ERROR_VOCAB_CODES
                    .iter()
                    .map(|code| CompletionItem {
                        label: (*code).to_owned(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: error_vocab_code_detail(code).map(str::to_owned),
                        ..CompletionItem::default()
                    })
                    .collect(),
            );
        }
    }

    None
}

// Auth-refresh rotation code actions live in `code_actions/auth_refresh.rs`
// and are re-exported above as `pub use code_actions::auth_refresh::auth_refresh_code_actions;`.


/// Position at the start of `line_idx` (character 0). Used as both the
/// start and end of an inserting `TextEdit` (zero-width range).
pub(crate) fn position_at_line_start(line_idx: usize) -> Position {
    Position {
        line: line_idx as u32,
        character: 0,
    }
}

/// Resolved-text hover for the 8 closed-catalog error codes, fired when
/// the cursor sits on one of them inside a feature's `errors` block. Shows
/// the locally-resolved translation (from the same feature's `translation`
/// block, first locale variant) or, if no feature-level override exists,
/// the built-in English fallback shipped by the runtime.
///
/// Returns `None` when the cursor is outside an `errors` block or when
/// `word` is not one of the 12 codes. The rich-markdown one-liner for the
/// codes ships through `keyword_description` instead.
pub fn error_vocab_code_resolved_hover(
    source: &str,
    position: Position,
    word: &str,
) -> Option<String> {
    if !ERROR_VOCAB_CODES.contains(&word) {
        return None;
    }
    if !in_feature_errors_block(source, position) {
        return None;
    }
    let feature = enclosing_feature_name(source, position)?;
    let resolved = error_vocab_resolved_text(source, &feature, word)?;
    // Identify the resolution-chain source so the author / auditor sees
    // **where** the resolved text came from. This mirrors the
    // `--expand=error-resolution-table` projection (proposal §3.6).
    let source_label = if lookup_feature_error_key(source, &feature, word).is_some() {
        format!("`feature.{feature}.errors.{word}`")
    } else {
        "runtime built-in catalog (`en-US` fallback)".to_owned()
    };
    let detail = error_vocab_code_detail(word).unwrap_or("");
    let lines = vec![
        format!("**`{word}`** — closed-catalog error code."),
        String::new(),
        format!("**Resolved**: \"{resolved}\""),
        String::new(),
        format!("**Source**: {source_label}"),
        String::new(),
        detail.to_owned(),
        String::new(),
        "Customize by adding a per-code override inside this feature's `errors` block:".to_owned(),
        "```lazuli".to_owned(),
        format!("errors"),
        format!("  {word} message @translation.<key>"),
        "```".to_owned(),
        String::new(),
        "See `docs/proposals/ir-error-messages-vocab.md` §2.E (resolution chain) and §7.2 (hover)."
            .to_owned(),
    ];
    Some(lines.join("\n"))
}

/// Detect whether the cursor sits inside a `feature.errors` block. The
/// feature `errors` block lives at indent 2 (under a feature header at
/// indent 0); children at indent 4. Cursor must be at indent >= 4 under a
/// closer-than-feature `errors` header.
pub(crate) fn in_feature_errors_block(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    // Walk backwards looking for either an `errors` header at indent 2
    // (we're inside it) or any other indent-2 line / indent-0 line first
    // (we're not).
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip the cursor line itself when it might be the very `errors`
        // header (we want to know whether children would be inside).
        if idx == cursor_line_idx && leading_spaces(line) >= 4 {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 2 {
            return trimmed == "errors";
        }
        if indent == 0 {
            return false;
        }
    }
    false
}


#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
