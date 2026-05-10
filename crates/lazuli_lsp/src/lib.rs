use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lazuli_syntax::{Span, parse_document};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, MarkupContent, MarkupKind, MessageType, OneOf, Position, Range,
    ServerCapabilities, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

pub fn server_name() -> &'static str {
    "lazuli-lsp"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityProfile {
    Prototype,
    Strict,
    Production,
}

pub fn diagnostics_for_source(source: &str) -> Vec<Diagnostic> {
    diagnostics_for_source_with_profile(source, SecurityProfile::Strict)
}

pub fn diagnostics_for_source_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    diagnostics_for_with_profile(source, security_profile)
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

struct Backend {
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
        let Some(word) = word_at_position(source, position) else {
            return Ok(None);
        };
        let Some(description) = keyword_description(&word) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("`{word}`\n\n{description}"),
            }),
            range: None,
        }))
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(
            KEYWORDS
                .iter()
                .map(|keyword| CompletionItem {
                    label: (*keyword).to_owned(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    detail: keyword_description(keyword).map(str::to_owned),
                    ..CompletionItem::default()
                })
                .collect(),
        )))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let Ok(document) = parse_document(source) else {
            return Ok(None);
        };

        let symbols = document
            .aggregates
            .iter()
            .map(|aggregate| {
                make_symbol(
                    aggregate.name.clone(),
                    Some("aggregate".to_owned()),
                    SymbolKind::STRUCT,
                    range_from_span(source, aggregate.span),
                    Some(
                        aggregate
                            .fields
                            .iter()
                            .map(|field| {
                                make_symbol(
                                    field.name.clone(),
                                    Some(field.ty.clone()),
                                    SymbolKind::PROPERTY,
                                    range_from_span(source, field.span),
                                    None,
                                )
                            })
                            .chain(aggregate.commands.iter().map(|command| {
                                make_symbol(
                                    command.name.clone(),
                                    Some("command".to_owned()),
                                    SymbolKind::METHOD,
                                    range_from_span(source, command.span),
                                    None,
                                )
                            }))
                            .chain(aggregate.queries.iter().map(|query| {
                                make_symbol(
                                    query.name.clone(),
                                    Some("query".to_owned()),
                                    SymbolKind::FUNCTION,
                                    range_from_span(source, query.span),
                                    None,
                                )
                            }))
                            .chain(aggregate.surfaces.iter().map(|surface| {
                                make_symbol(
                                    surface.name.clone(),
                                    Some("surface".to_owned()),
                                    SymbolKind::INTERFACE,
                                    range_from_span(source, surface.span),
                                    None,
                                )
                            }))
                            .collect(),
                    ),
                )
            })
            .collect();

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
}

#[allow(deprecated)]
fn make_symbol(
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

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, source: String) {
        let diagnostics = diagnostics_for_uri(&uri, &source);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn diagnostics_for_uri(uri: &Url, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = diagnostics_for(source);

    if is_lzx_source(source) {
        diagnostics.extend(lzx_filename_diagnostics(uri, source));
    }

    diagnostics
}

fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    diagnostics_for_with_profile(source, SecurityProfile::Strict)
}

fn diagnostics_for_with_profile(
    source: &str,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    if is_canonical_source(source) {
        let mut diagnostics = canonical_order_diagnostics(source);
        diagnostics.extend(query_mode_diagnostics(source));
        diagnostics.extend(previously_mode_diagnostics(source));
        diagnostics.extend(app_operational_contract_diagnostics(source));
        diagnostics.extend(generated_summary_diagnostics(source));
        diagnostics.extend(non_goals_shape_diagnostics(source));
        diagnostics.extend(defaults_policy_syntax_diagnostics(source));
        diagnostics.extend(lookup_shorthand_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(refs_block_diagnostics(source));
        diagnostics.extend(policy_namespace_diagnostics(source));
        diagnostics.extend(scope_override_policy_diagnostics(source));
        diagnostics.extend(query_order_default_diagnostics(source));
        diagnostics.extend(query_pagination_diagnostics(source));
        diagnostics.extend(query_filter_index_diagnostics(source));
        diagnostics.extend(query_search_syntax_diagnostics(source));
        diagnostics.extend(active_session_query_diagnostics(source));
        diagnostics.extend(command_rate_limit_contract_diagnostics(source));
        diagnostics.extend(event_job_tenant_from_diagnostics(source));
        diagnostics.extend(scheduled_job_tenancy_diagnostics(source));
        diagnostics.extend(crypto_contract_diagnostics(source));
        diagnostics.extend(file_capability_contract_diagnostics(source));
        diagnostics.extend(sql_return_type_diagnostics(source));
        diagnostics.extend(type_namespace_diagnostics(source));
        diagnostics.extend(validation_syntax_diagnostics(source));
        diagnostics.extend(extension_declaration_diagnostics(source));
        diagnostics.extend(event_payload_reference_diagnostics(source));
        diagnostics.extend(event_kind_diagnostics(source));
        diagnostics.extend(event_trace_trigger_diagnostics(source));
        diagnostics.extend(event_consumer_payload_diagnostics(source));
        diagnostics.extend(event_locator_diagnostics(source));
        diagnostics.extend(target_binding_diagnostics(source));
        diagnostics.extend(rule_self_diagnostics(source));
        diagnostics.extend(required_field_nil_rule_diagnostics(source));
        diagnostics.extend(command_validator_diagnostics(source));
        diagnostics.extend(error_contract_diagnostics(source));
        diagnostics.extend(cache_contract_diagnostics(source));
        diagnostics.extend(api_contract_diagnostics(source));
        diagnostics.extend(anchor_whitelist_diagnostics(source));
        diagnostics.extend(test_block_diagnostics(source));
        diagnostics.extend(command_contract_diagnostics(source));
        diagnostics.extend(field_security_policy_diagnostics(source));
        diagnostics.extend(retention_contract_diagnostics(source));
        diagnostics.extend(write_window_contract_diagnostics(source));
        diagnostics.extend(env_schema_diagnostics(source));
        diagnostics.extend(webhook_security_diagnostics(source));
        diagnostics.extend(webhook_tenant_from_diagnostics(source));
        diagnostics.extend(escape_route_security_diagnostics(source));
        diagnostics.extend(auth_security_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        diagnostics.extend(idempotency_key_diagnostics(source));
        return apply_security_profile(diagnostics, security_profile);
    }

    if is_lzx_source(source) {
        let mut diagnostics = lzx_contract_diagnostics(source);
        diagnostics.extend(lzx_route_contract_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        return diagnostics;
    }

    let document = match parse_document(source) {
        Ok(document) => document,
        Err(error) => {
            return vec![Diagnostic {
                range: range_from_span(source, error.span()),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("lazuli-syntax".to_owned()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                data: None,
            }];
        }
    };

    match lazuli_analyzer::lower_document(&document) {
        Ok(_) => Vec::new(),
        Err(error) => vec![Diagnostic {
            range: first_line_range(source),
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("lazuli-analyzer".to_owned()),
            message: error.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }],
    }
}

fn is_canonical_source(source: &str) -> bool {
    if has_lzx_top_level_contract(source) {
        return false;
    }

    source.lines().any(|line| {
        leading_spaces(line) == 0
            && (line.trim_start().starts_with("feature ") || line.trim_start() == "env")
    }) || has_canonical_app_block(source)
}

fn has_canonical_app_block(source: &str) -> bool {
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

fn is_lzx_source(source: &str) -> bool {
    has_lzx_top_level_contract(source)
}

fn has_lzx_top_level_contract(source: &str) -> bool {
    source.lines().any(|line| {
        leading_spaces(line) == 0
            && matches!(
                line.trim_start().split_whitespace().next(),
                Some("route" | "experience" | "surface")
            )
    })
}

fn lzx_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
struct LzxRouteViewFacts {
    routes: HashSet<String>,
    references: Vec<(usize, String, String)>,
    unbound_target_actions: Vec<(usize, String)>,
}

#[derive(Debug)]
struct LzxAppRouteFacts {
    line_index: usize,
    line: String,
    has_path_or_stack: bool,
    legacy_stack: Option<(usize, String)>,
    has_to: bool,
    has_surface: bool,
    has_audience: bool,
    declared_params: HashSet<String>,
    path_params: Vec<String>,
    path_references: Vec<(usize, String, String)>,
}

fn lzx_route_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                    has_path_or_stack: false,
                    legacy_stack: None,
                    has_to: false,
                    has_surface: false,
                    has_audience: false,
                    declared_params: HashSet::new(),
                    path_params: Vec::new(),
                    path_references: Vec::new(),
                });
            }
            in_experience = trimmed.starts_with("experience ");
            continue;
        }

        if let Some(route) = current_route.as_mut() {
            if leading_spaces(line) == 2 {
                if let Some(path) = trimmed.strip_prefix("path ") {
                    route.has_path_or_stack = true;
                    route
                        .path_params
                        .extend(lzx_declared_path_params(unquote_lzx_literal(path.trim())));
                } else if let Some(stack) = trimmed.strip_prefix("stack ") {
                    route.has_path_or_stack = true;
                    route.legacy_stack = Some((line_index, line.to_owned()));
                    route
                        .path_params
                        .extend(lzx_declared_path_params(unquote_lzx_literal(stack.trim())));
                } else if let Some(params) = trimmed.strip_prefix("params ") {
                    for param in params
                        .split(',')
                        .filter_map(|part| route_slot_name(part.trim()))
                    {
                        route.declared_params.insert(param.to_owned());
                    }
                } else if let Some(target) = trimmed.strip_prefix("to ") {
                    route.has_to = true;
                    for reference in path_references(target, "path.") {
                        route.path_references.push((
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

fn lzx_app_route_diagnostics(route: LzxAppRouteFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !route.has_path_or_stack {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should declare a concrete `path`; `surface <name> web|mobile` decides whether it is a web URL path or mobile route pattern.",
        ));
    }
    if let Some((line_index, line)) = route.legacy_stack {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "lzx-route-stack-legacy",
            "`stack` is legacy route syntax; use `path` for both web URLs and mobile route patterns, with `surface ... mobile` selecting Expo navigation.",
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
        if !route.declared_params.contains(&path_param) {
            diagnostics.push(simple_canonical_diagnostic(
                route.line_index,
                &route.line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route path parameter `{path_param}` should be declared with `params {path_param}: <Type>` so route builders are type-safe.",
                ),
            ));
        }
    }

    for (line_index, line, reference) in route.path_references {
        if !route.declared_params.contains(&reference) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route target references `path.{reference}` but the route does not declare `params {reference}: ...`.",
                ),
            ));
        }
    }

    diagnostics
}

fn lzx_declared_path_params(path: &str) -> Vec<String> {
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

fn unquote_lzx_literal(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

fn is_quoted_lzx_literal(value: &str) -> bool {
    value.starts_with('"') && value.ends_with('"') && value.len() >= 2
}

fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn lzx_route_view_diagnostics(view: LzxRouteViewFacts) -> Vec<Diagnostic> {
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

fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

fn lzx_route_references(source: &str) -> Vec<&str> {
    path_references(source, "route.")
}

fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
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

fn lzx_filename_diagnostics(uri: &Url, source: &str) -> Vec<Diagnostic> {
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

fn lzx_platform_from_file_name(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(".web.lzx") {
        Some("web")
    } else if file_name.ends_with(".mobile.lzx") {
        Some("mobile")
    } else {
        None
    }
}

fn first_lzx_surface_header(source: &str) -> Option<(usize, &str, &str)> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalBlockKind {
    Meta,
    Defaults,
    Uses,
    Refs,
    Domain,
    Policies,
    Errors,
    Auth,
    Command,
    Api,
    Workflow,
    Job,
    Webhook,
    Surface,
    Extensions,
    EscapeRoute,
}

impl CanonicalBlockKind {
    fn rank(self) -> u8 {
        match self {
            Self::Meta => 0,
            Self::Defaults => 1,
            Self::Uses => 2,
            Self::Refs => 3,
            Self::Domain => 4,
            Self::Policies => 5,
            Self::Errors => 6,
            Self::Auth => 7,
            Self::Command => 8,
            Self::Api => 9,
            Self::Workflow => 10,
            Self::Job => 11,
            Self::Webhook => 12,
            Self::Surface => 13,
            Self::Extensions => 14,
            Self::EscapeRoute => 15,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Defaults => "defaults",
            Self::Uses => "uses",
            Self::Refs => "refs",
            Self::Domain => "domain",
            Self::Policies => "policies",
            Self::Errors => "errors",
            Self::Auth => "auth",
            Self::Command => "command",
            Self::Api => "api",
            Self::Workflow => "workflow",
            Self::Job => "job",
            Self::Webhook => "webhook",
            Self::Surface => "surface",
            Self::Extensions => "extensions",
            Self::EscapeRoute => "escape_route",
        }
    }
}

const CANONICAL_FEATURE_ORDER: &str = "meta -> defaults -> uses -> refs -> domain -> policies -> errors -> auth -> command -> api -> workflow -> job -> webhook -> surface -> extensions -> escape_route";

fn canonical_order_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<CanonicalFeatureOrder> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(CanonicalFeatureOrder::new(feature_name(trimmed)));
            continue;
        }

        let Some(feature) = current_feature.as_mut() else {
            continue;
        };

        if leading_spaces(line) != 2 {
            continue;
        }

        let Some(kind) = canonical_block_kind(trimmed) else {
            continue;
        };

        if let Some(previous) = feature.last_kind {
            if kind.rank() < previous.rank() {
                diagnostics.push(canonical_order_diagnostic(
                    line_index,
                    line,
                    &feature.name,
                    kind,
                    previous,
                ));
                continue;
            }
        }

        feature.last_kind = Some(kind);
    }

    diagnostics
}

#[derive(Debug)]
struct CanonicalFeatureOrder {
    name: String,
    last_kind: Option<CanonicalBlockKind>,
}

impl CanonicalFeatureOrder {
    fn new(name: String) -> Self {
        Self {
            name,
            last_kind: None,
        }
    }
}

fn canonical_order_diagnostic(
    line_index: usize,
    line: &str,
    feature_name: &str,
    found: CanonicalBlockKind,
    previous: CanonicalBlockKind,
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
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            "canonical-order".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "non-canonical block order in feature `{feature_name}`: `{}` appears after `{}`. Expected order: {CANONICAL_FEATURE_ORDER}.",
            found.label(),
            previous.label()
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn canonical_block_kind(trimmed_line: &str) -> Option<CanonicalBlockKind> {
    let first = trimmed_line.split_whitespace().next()?;

    match first {
        "purpose" | "non_goals" | "context" => Some(CanonicalBlockKind::Meta),
        "defaults" => Some(CanonicalBlockKind::Defaults),
        "uses" => Some(CanonicalBlockKind::Uses),
        "refs" => Some(CanonicalBlockKind::Refs),
        "domain" => Some(CanonicalBlockKind::Domain),
        "policies" => Some(CanonicalBlockKind::Policies),
        "errors" => Some(CanonicalBlockKind::Errors),
        "auth" => Some(CanonicalBlockKind::Auth),
        "command" => Some(CanonicalBlockKind::Command),
        "api" => Some(CanonicalBlockKind::Api),
        "workflow" => Some(CanonicalBlockKind::Workflow),
        "job" => Some(CanonicalBlockKind::Job),
        "webhook" => Some(CanonicalBlockKind::Webhook),
        "surface" => Some(CanonicalBlockKind::Surface),
        "extensions" => Some(CanonicalBlockKind::Extensions),
        "escape_route" => Some(CanonicalBlockKind::EscapeRoute),
        _ => None,
    }
}

fn query_mode_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };

        if first == "query" && leading_spaces(line) <= 4 {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-mode",
                "query declarations should use an explicit mode: `query.list <name>`, `query.lookup <name>`, or `query.sql <name>`.",
            ));
        } else if let Some(mode) = first.strip_prefix("query.") {
            if !matches!(mode, "list" | "lookup" | "sql") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "query-mode",
                    "unknown query mode. Use `query.list`, `query.lookup`, or `query.sql`.",
                ));
            }
        }
    }

    diagnostics
}

fn previously_mode_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((_, tail)) = trimmed.split_once(" previously ") else {
            continue;
        };

        let tail = tail.trim_start();
        if tail.starts_with("migrated ") || tail.starts_with("alias ") {
            continue;
        }

        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "previously-mode-contract",
            "`previously` should declare `migrated` or `alias` so migration-only history is distinct from compatibility aliases.",
        ));
    }

    diagnostics
}

fn query_order_default_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_query_list = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading <= 4 {
            in_query_list = leading == 4 && trimmed.starts_with("query.list ");
        }

        if in_query_list && leading == 6 && trimmed == "order created_at desc" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-order-default",
                "`query.list` defaults to `order created_at desc`; omit the line unless the query intentionally uses a different order.",
            ));
        }
    }

    diagnostics
}

fn query_pagination_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query_mode: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading <= 4 {
            current_query_mode = if leading == 4 && trimmed.starts_with("query.") {
                trimmed.split_whitespace().next()
            } else {
                None
            };
        }

        let Some(value) = trimmed.strip_prefix("paginate ") else {
            continue;
        };

        if current_query_mode != Some("query.list") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-pagination-scope",
                "`paginate` is a `query.list` contract; lookup and SQL queries should model limits explicitly in their own params or SQL.",
            ));
        }

        if !matches!(value.trim().parse::<u64>(), Ok(page_size) if page_size > 0) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-pagination-size",
                "`paginate` should declare a positive integer default page size, e.g. `paginate 50`.",
            ));
        }
    }

    diagnostics
}

fn query_filter_index_diagnostics(source: &str) -> Vec<Diagnostic> {
    let generated = generated_query_filter_indexes(source);
    if generated.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(index) = trimmed.strip_prefix("index ") else {
            continue;
        };
        let index = normalize_index_value(index);

        if generated.contains(&index) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-filter-index-generated",
                "`query.list` equality filters generate this tenant-aware index; omit the explicit `index` unless the query needs a non-default index shape.",
            ));
        }
    }

    diagnostics
}

fn query_search_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("= params.search") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-search-syntax",
                "text matching should use `search params.search over ...` instead of an equality-looking filter such as `name = params.search`.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
struct ActiveSessionQueryFacts {
    line_index: usize,
    line: String,
    has_temporal_scope: bool,
    expires_not_nil: Option<(usize, String)>,
}

fn active_session_query_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<ActiveSessionQueryFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("query.list ") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(active_session_query_facts_diagnostics(query));
            }

            if trimmed
                .split_whitespace()
                .nth(1)
                .is_some_and(|name| name == "active_sessions")
            {
                current_query = Some(ActiveSessionQueryFacts {
                    line_index,
                    line: line.to_owned(),
                    has_temporal_scope: false,
                    expires_not_nil: None,
                });
            }
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if let Some(query) = current_query.take() {
                diagnostics.extend(active_session_query_facts_diagnostics(query));
            }
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if trimmed.contains("expires_at > ctx.now")
            || trimmed.contains("expires_at >= ctx.now")
            || trimmed.contains("guarantees expires_at > ctx.now")
            || trimmed.contains("guarantees expires_at >= ctx.now")
        {
            query.has_temporal_scope = true;
        }

        if trimmed == "expires_at != nil" {
            query.expires_not_nil = Some((line_index, line.to_owned()));
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(active_session_query_facts_diagnostics(query));
    }

    diagnostics
}

fn active_session_query_facts_diagnostics(query: ActiveSessionQueryFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some((line_index, line)) = query.expires_not_nil {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "active-session-temporal-scope",
            "`active_sessions` should prove temporal validity; `expires_at != nil` can include expired sessions. Use an explicit `expires_at > ctx.now` guard or a modifier `guarantees expires_at > ctx.now` contract.",
        ));
    } else if !query.has_temporal_scope {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "active-session-temporal-scope",
            "`active_sessions` should declare temporal validity with an explicit `expires_at > ctx.now` guard or a modifier `guarantees expires_at > ctx.now` contract.",
        ));
    }

    diagnostics
}

fn generated_query_filter_indexes(source: &str) -> HashSet<String> {
    let lines: Vec<_> = source.lines().collect();
    let tenancy_axis = single_tenancy_axis(&lines);
    let mut indexes = HashSet::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4
            && trimmed.starts_with("query.list ")
            && !query_block_has_scope_override(&lines, index)
        {
            for field in query_block_filter_index_fields(&lines, index) {
                let value = tenancy_axis
                    .as_ref()
                    .map(|tenant| format!("{tenant}, {field}"))
                    .unwrap_or(field);
                indexes.insert(normalize_index_value(&value));
            }
        }

        index += 1;
    }

    indexes
}

fn single_tenancy_axis(lines: &[&str]) -> Option<String> {
    let axes: HashSet<String> = lines
        .iter()
        .filter_map(|line| {
            let axis = line.trim_start().strip_prefix("tenancy ")?.trim();
            (!axis.is_empty() && axis != "none").then(|| axis.to_owned())
        })
        .collect();

    if axes.len() == 1 {
        axes.into_iter().next()
    } else {
        None
    }
}

fn query_block_has_scope_override(lines: &[&str], start: usize) -> bool {
    let mut index = start + 1;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && leading_spaces(line) <= 4 {
            break;
        }
        if trimmed == "scope override" {
            return true;
        }
        index += 1;
    }

    false
}

fn query_block_filter_index_fields(lines: &[&str], start: usize) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_filters = false;
    let mut index = start + 1;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if !trimmed.is_empty() && leading <= 4 {
            break;
        }

        if !trimmed.is_empty() {
            if leading == 6 {
                in_filters = trimmed == "filters";
            } else if in_filters
                && leading == 8
                && let Some(field) = filter_index_field(trimmed)
            {
                fields.push(field);
            }
        }

        index += 1;
    }

    fields
}

fn filter_index_field(filter: &str) -> Option<String> {
    if filter.contains(" has ")
        || filter.contains(" != ")
        || filter.contains(" = nil")
        || filter.contains(" != nil")
    {
        return None;
    }

    if let Some((field, param)) = filter.split_once(" when ") {
        let field = field.trim();
        let param = param.trim().strip_prefix("params.")?;
        if is_identifier(field) && field == param {
            return Some(field.to_owned());
        }
        return None;
    }

    if let Some((left, right)) = filter.split_once(" = ") {
        let left = left.trim();
        let param = right.trim().strip_prefix("params.")?;

        if is_identifier(left) && left == param {
            return Some(left.to_owned());
        }

        if let Some(relation) = left.strip_suffix(".id")
            && is_identifier(relation)
            && param == format!("{relation}_id")
        {
            return Some(relation.to_owned());
        }
    }

    None
}

fn normalize_index_value(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn generated_summary_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn non_goals_shape_diagnostics(source: &str) -> Vec<Diagnostic> {
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

            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "non-goals-shape",
                "`non_goals` should group entries under `delegated_to` or `out_of_scope`; direct keys and `anti_pattern.*` are legacy.",
            ));
        }
    }

    diagnostics
}

fn defaults_policy_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
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

#[derive(Debug)]
struct LookupQueryFacts {
    line_index: usize,
    line: String,
    params: Vec<(String, String)>,
    key: Option<(String, String)>,
}

fn lookup_shorthand_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<LookupQueryFacts> = None;
    let mut current_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("query.lookup ") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(lookup_query_diagnostics(query));
            }

            current_query = if trimmed.contains(" by ") {
                None
            } else {
                Some(LookupQueryFacts {
                    line_index,
                    line: line.to_owned(),
                    params: Vec::new(),
                    key: None,
                })
            };
            current_child = None;
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some(query) = current_query.take() {
                diagnostics.extend(lookup_query_diagnostics(query));
            }
            current_child = None;
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 6 {
            if trimmed == "params" {
                current_child = Some("params");
            } else if let Some((lhs, rhs)) = lookup_key_assignment(trimmed) {
                query.key = Some((lhs.to_owned(), rhs.to_owned()));
                current_child = None;
            } else {
                current_child = None;
            }
        } else if leading_spaces(line) == 8 && current_child == Some("params") {
            if let Some((name, ty)) = typed_param(trimmed) {
                query.params.push((name.to_owned(), ty.to_owned()));
            }
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(lookup_query_diagnostics(query));
    }

    diagnostics
}

fn lookup_query_diagnostics(query: LookupQueryFacts) -> Vec<Diagnostic> {
    let Some((key_field, key_param)) = query.key.as_ref() else {
        return Vec::new();
    };

    if query.params.len() == 1 && query.params[0].0 == *key_field && query.params[0].0 == *key_param
    {
        vec![simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "query-lookup-shorthand",
            "single-key lookup queries should use shorthand, e.g. `query.lookup by_id by id: ID`.",
        )]
    } else {
        Vec::new()
    }
}

fn typed_param(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.trim().split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

fn lookup_key_assignment(trimmed_line: &str) -> Option<(&str, &str)> {
    let rest = trimmed_line.strip_prefix("key ")?;
    let (lhs, rhs) = rest.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim().strip_prefix("params.")?.trim();

    if lhs.is_empty() || rhs.is_empty() {
        None
    } else {
        Some((lhs, rhs))
    }
}

fn namespace_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        for namespace in namespace_references(line) {
            if !is_allowed_reference_namespace(namespace) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "namespace-catalog",
                    "unknown `@...` namespace. Allowed namespaces are `@role`, `@scope`, `@actor`, `@policy`, `@semantic`, `@cap`, `@pii`, `@key`, `@fn`, `@hook`, `@validator`, `@adapter`, `@client`, `@query_modifier`, and `@anchor`.",
                ));
                break;
            }
        }
    }

    diagnostics
}

fn namespace_references(line: &str) -> Vec<&str> {
    let mut namespaces = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find('@') {
        let after_at = &rest[start + 1..];
        let Some(dot) = after_at.find('.') else {
            rest = after_at;
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

        rest = &after_at[dot + 1..];
    }

    namespaces
}

fn is_allowed_reference_namespace(namespace: &str) -> bool {
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
    )
}

#[derive(Debug, Default)]
struct FeatureRefsFacts {
    name: String,
    refs_line: Option<(usize, String)>,
    declared: HashSet<String>,
    used: HashSet<String>,
}

fn refs_block_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current: Option<FeatureRefsFacts> = None;
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(facts) = current.take() {
                diagnostics.extend(refs_facts_diagnostics(facts));
            }
            current = Some(FeatureRefsFacts {
                name: feature_name(trimmed),
                ..FeatureRefsFacts::default()
            });
            current_top = None;
            continue;
        }

        let Some(facts) = current.as_mut() else {
            continue;
        };

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            if current_top == Some("refs") {
                facts.refs_line = Some((line_index, line.to_owned()));
            }
            continue;
        }

        if current_top == Some("refs") && leading_spaces(line) == 4 {
            if let Some((_, namespaces)) = trimmed.split_once(':') {
                for namespace in namespaces
                    .split(',')
                    .map(str::trim)
                    .filter_map(|namespace| namespace.strip_prefix('@'))
                {
                    facts.declared.insert(namespace.to_owned());
                }
            }
            continue;
        }

        for namespace in namespace_references(line) {
            facts.used.insert(namespace.to_owned());
        }
    }

    if let Some(facts) = current {
        diagnostics.extend(refs_facts_diagnostics(facts));
    }

    diagnostics
}

fn refs_facts_diagnostics(facts: FeatureRefsFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some((line_index, line)) = facts.refs_line else {
        return diagnostics;
    };

    let mut missing: Vec<_> = facts.used.difference(&facts.declared).cloned().collect();
    let mut unused: Vec<_> = facts.declared.difference(&facts.used).cloned().collect();
    missing.sort();
    unused.sort();

    if !missing.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "refs-missing",
            &format!(
                "refs for feature `{}` is missing used namespaces: {}.",
                facts.name,
                missing
                    .iter()
                    .map(|namespace| format!("@{namespace}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    if !unused.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "refs-unused",
            &format!(
                "refs for feature `{}` declares unused namespaces: {}.",
                facts.name,
                unused
                    .iter()
                    .map(|namespace| format!("@{namespace}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    diagnostics
}

fn policy_namespace_diagnostics(source: &str) -> Vec<Diagnostic> {
    let policy_categories = collect_policy_categories(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            _ => {}
        }

        if current_top == Some("policies") {
            for atom in policy_atoms_from_dictionary_line(trimmed) {
                if !is_namespaced_atom(atom) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "policy-atom-namespace",
                        "policy atoms should be namespaced by category, e.g. `@role.admin`, `@scope.same_org`, `@actor.system`, or `@scope.public`.",
                    ));
                    break;
                }
            }
        }

        let Some(policy_ref) = policy_statement_ref(trimmed) else {
            continue;
        };

        let is_policy_category_ref = policy_ref.strip_prefix("@policy.").unwrap_or(policy_ref);

        let is_local_category = current_feature
            .as_ref()
            .and_then(|feature| policy_categories.get(feature))
            .is_some_and(|categories| categories.contains(is_policy_category_ref));

        if matches!(current_top, Some("command" | "workflow"))
            && !policy_ref.starts_with("@policy.")
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "commands and workflows should reference feature-local policy categories with `@policy.*`; put `@role.*`, `@scope.*`, or `@actor.*` atoms in the `policies` dictionary.",
            ));
            continue;
        }

        if policy_ref.starts_with("@policy.") {
            if !is_local_category {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "policy-ref-namespace",
                    "`@policy.*` references should resolve to a feature-local policy category.",
                ));
            }
            continue;
        }

        if is_namespaced_atom(policy_ref) {
            continue;
        }

        if policy_ref.contains('.') && !policy_ref.starts_with('@') {
            continue;
        }

        if is_local_category {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "feature-local policy categories should be referenced with `@policy.*`, e.g. `policy @policy.create`, to distinguish them from built-in actors, roles, and scopes.",
            ));
            continue;
        }

        if !is_local_category {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "direct policy atoms should be namespaced, e.g. `policy @actor.system` or `policy @role.admin`. Feature-local policy categories use `@policy.*`, e.g. `policy @policy.create`.",
            ));
        }
    }

    diagnostics
}

fn collect_policy_categories(source: &str) -> HashMap<String, HashSet<String>> {
    let mut categories: HashMap<String, HashSet<String>> = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            4 if current_top == Some("policies") => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let Some((name, _)) = trimmed.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() || name == "fields" || name.contains(' ') {
                    continue;
                }
                categories
                    .entry(feature_name.to_owned())
                    .or_default()
                    .insert(name.to_owned());
            }
            _ => {}
        }
    }

    categories
}

fn policy_atoms_from_dictionary_line(trimmed_line: &str) -> Vec<&str> {
    let Some((_, rhs)) = trimmed_line.split_once(':') else {
        return Vec::new();
    };

    if rhs.trim_start().starts_with('"') {
        return Vec::new();
    }

    rhs.split(',')
        .map(str::trim)
        .filter(|atom| !atom.is_empty())
        .collect()
}

fn policy_statement_ref(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "policy" {
        parts.next()
    } else {
        None
    }
}

fn is_namespaced_atom(atom: &str) -> bool {
    matches!(
        atom.strip_prefix('@').and_then(|rest| rest.split_once('.')),
        Some(("role" | "scope" | "actor", name)) if !name.is_empty()
    )
}

#[derive(Debug)]
struct QuerySecurityFacts {
    line_index: usize,
    line: String,
    has_policy: bool,
    has_scope_override: bool,
    has_scope_override_reason: bool,
}

fn scope_override_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<QuerySecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            current_query = Some(QuerySecurityFacts {
                line_index,
                line: line.to_owned(),
                has_policy: false,
                has_scope_override: false,
                has_scope_override_reason: false,
            });
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 6 && trimmed.starts_with("policy ") {
            query.has_policy = true;
        } else if leading_spaces(line) == 6 && trimmed.starts_with("scope override") {
            query.has_scope_override = true;
        } else if leading_spaces(line) == 8 && trimmed.starts_with("reason ") {
            query.has_scope_override_reason = true;
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(query_scope_override_diagnostics(query));
    }

    diagnostics
}

fn query_scope_override_diagnostics(query: QuerySecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if query.has_scope_override && !query.has_policy {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-policy",
            "`scope override` replaces inherited tenant/soft-delete safety scope; the query must declare an explicit `policy @policy.*`.",
        ));
    }

    if query.has_scope_override && !query.has_scope_override_reason {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-reason",
            "`scope override` should include a `reason \"...\"` child explaining why inherited tenant/soft-delete scope is replaced.",
        ));
    }

    diagnostics
}

#[derive(Debug)]
struct CommandSecurityFacts {
    feature: String,
    line_index: usize,
    line: String,
    policy: Option<String>,
    has_write_effect: bool,
    has_rate_limit: bool,
    rate_limit_none: Option<(usize, String)>,
    rate_limit_none_has_reason: bool,
}

fn command_rate_limit_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let policies = collect_policy_atom_map(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_command: Option<CommandSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            current_command = current_feature
                .as_ref()
                .map(|feature| CommandSecurityFacts {
                    feature: feature.clone(),
                    line_index,
                    line: line.to_owned(),
                    policy: None,
                    has_write_effect: false,
                    has_rate_limit: false,
                    rate_limit_none: None,
                    rate_limit_none_has_reason: false,
                });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if let Some(policy) = policy_statement_ref(trimmed) {
                command.policy = Some(policy.to_owned());
            } else if trimmed == "rate_limit none" {
                command.has_rate_limit = true;
                command.rate_limit_none = Some((line_index, line.to_owned()));
            } else if trimmed.starts_with("rate_limit ") {
                command.has_rate_limit = true;
            } else if command_write_effect(trimmed).is_some() {
                command.has_write_effect = true;
            }
        } else if leading_spaces(line) == 6
            && command.rate_limit_none.is_some()
            && trimmed.starts_with("reason ")
        {
            command.rate_limit_none_has_reason = true;
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
    }

    diagnostics
}

fn command_rate_limit_diagnostics(
    command: CommandSecurityFacts,
    policies: &HashMap<(String, String), Vec<String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let is_public = command
        .policy
        .as_deref()
        .is_some_and(|policy| policy_ref_is_public(&command.feature, policy, policies));

    if (is_public || command.has_write_effect) && !command.has_rate_limit {
        diagnostics.push(simple_canonical_diagnostic(
            command.line_index,
            &command.line,
            DiagnosticSeverity::WARNING,
            "command-rate-limit",
            "commands that are public or mutate state must declare a command-level `rate_limit` or `rate_limit none` with a `reason` child.",
        ));
    }

    if let Some((line_index, line)) = command.rate_limit_none {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "security-opt-out",
            "`rate_limit none` is an explicit security opt-out. Strict profile allows it for reviewed drafts; production profile treats it as a release blocker.",
        ));

        if !command.rate_limit_none_has_reason {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "security-opt-out-reason",
                "`rate_limit none` must include a `reason \"...\"` child.",
            ));
        }
    }

    diagnostics
}

fn policy_ref_is_public(
    feature: &str,
    policy_ref: &str,
    policies: &HashMap<(String, String), Vec<String>>,
) -> bool {
    if policy_ref == "@scope.public" {
        return true;
    }

    let Some(category) = policy_ref.strip_prefix("@policy.") else {
        return false;
    };

    policies
        .get(&(feature.to_owned(), category.to_owned()))
        .is_some_and(|atoms| atoms.iter().any(|atom| atom == "@scope.public"))
}

fn collect_policy_atom_map(source: &str) -> HashMap<(String, String), Vec<String>> {
    let mut policies = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            4 if current_top == Some("policies") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some((name, atoms)) = trimmed.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() || name == "fields" || name.contains(' ') {
                    continue;
                }
                policies.insert(
                    (feature.to_owned(), name.to_owned()),
                    atoms
                        .split(',')
                        .map(str::trim)
                        .filter(|atom| !atom.is_empty())
                        .map(str::to_owned)
                        .collect(),
                );
            }
            _ => {}
        }
    }

    policies
}

fn crypto_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if line.contains("@cap.Secret") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "crypto-tier",
                "`@cap.Secret` is legacy; choose an explicit tier such as `@cap.Hashed(...)`, `@cap.Encrypted(key:@key.*)`, or `@cap.Token(...)`.",
            ));
        }

        let hashed_args = capability_args(line, "Hashed");
        if line.contains("@cap.Hashed")
            && !hashed_args
                .as_deref()
                .is_some_and(|args| capability_arg(args, "algorithm").is_some())
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "crypto-hash-algorithm",
                "`@cap.Hashed` should declare `algorithm:<name>` so the hash contract is audit-visible.",
            ));
        }
        if let Some(args) = hashed_args.as_deref() {
            warn_unknown_capability_args(
                &mut diagnostics,
                line_index,
                line,
                "@cap.Hashed",
                args,
                &["algorithm"],
            );
            if let Some(algorithm) = capability_arg(args, "algorithm")
                && !matches!(algorithm, "argon2id" | "bcrypt")
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "crypto-hash-algorithm",
                    "canonical v0 hash algorithms are `argon2id` or `bcrypt` for legacy migration.",
                ));
            }
        }

        for capability in ["Encrypted", "E2ee"] {
            let args = capability_args(line, capability);
            if line.contains(&format!("@cap.{capability}"))
                && !args
                    .as_deref()
                    .is_some_and(|args| capability_arg(args, "key").is_some())
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "crypto-key-scope",
                    &format!(
                        "`@cap.{capability}` should declare `key:@key.<scope>` so key blast radius is audit-visible."
                    ),
                ));
            }
            if let Some(args) = args.as_deref() {
                warn_unknown_capability_args(
                    &mut diagnostics,
                    line_index,
                    line,
                    &format!("@cap.{capability}"),
                    args,
                    &["key"],
                );
                if let Some(key) = capability_arg(args, "key")
                    && !is_key_scope(key)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-key-scope",
                        "encryption capability keys should use `key:@key.<scope>`.",
                    ));
                }
            }
        }

        if line.contains("@cap.Token") {
            let token_args = capability_args(line, "Token");
            for (required, message) in [
                (
                    "ttl",
                    "`@cap.Token` should declare `ttl:<duration>` for expiry.",
                ),
                (
                    "single_use",
                    "`@cap.Token` should declare `single_use:true|false`.",
                ),
                (
                    "store",
                    "`@cap.Token` should declare `store:hashed` or another explicit storage strategy.",
                ),
            ] {
                if !token_args
                    .as_deref()
                    .is_some_and(|args| capability_arg(args, required).is_some())
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        message,
                    ));
                }
            }
            if let Some(args) = token_args.as_deref() {
                warn_unknown_capability_args(
                    &mut diagnostics,
                    line_index,
                    line,
                    "@cap.Token",
                    args,
                    &["ttl", "single_use", "store"],
                );
                if let Some(ttl) = capability_arg(args, "ttl")
                    && !is_duration_literal(ttl)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` ttl should use `ttl:<duration>` such as `30s`, `10m`, `1h`, or `7d`.",
                    ));
                }
                if let Some(single_use) = capability_arg(args, "single_use")
                    && !matches!(single_use, "true" | "false")
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` single_use should be `true` or `false`.",
                    ));
                }
                if let Some(store) = capability_arg(args, "store")
                    && store != "hashed"
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` store should be `hashed` in canonical v0.",
                    ));
                }
            }
        }
    }

    diagnostics
}

fn file_capability_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
            &["max_size", "accept"],
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

fn capability_args(line: &str, capability: &str) -> Option<Vec<(String, String)>> {
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

fn capability_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(arg_key, _)| arg_key == key)
        .map(|(_, value)| value.as_str())
}

fn warn_unknown_capability_args(
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

fn is_duration_literal(value: &str) -> bool {
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

fn is_file_size_literal(value: &str) -> bool {
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

fn is_retention_duration_literal(value: &str) -> bool {
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

fn is_key_scope(value: &str) -> bool {
    value
        .strip_prefix("@key.")
        .is_some_and(|scope| is_identifier(scope))
}

fn type_namespace_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_env = false;
    let mut in_app = false;
    let mut app_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            in_env = trimmed == "env";
            in_app = trimmed.starts_with("app ");
            app_child = None;
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

fn sql_return_type_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                in_sql_query = trimmed.starts_with("query.sql ");
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
                            "`query.sql` return type `{return_type}` should resolve to a local `record` or `resource`; SQL result shapes are not inferred from the SQL file."
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    diagnostics
}

fn collect_declared_type_names_by_feature(source: &str) -> HashMap<String, HashSet<String>> {
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

fn canonical_return_type_name(return_type: &str) -> &str {
    return_type
        .strip_suffix("[]")
        .unwrap_or(return_type)
        .trim_end_matches('?')
}

fn is_builtin_return_type(return_type: &str) -> bool {
    matches!(
        return_type,
        "Text" | "Integer" | "Decimal" | "Boolean" | "ID" | "DateTime" | "JSON"
    ) || return_type.starts_with("@semantic.")
        || return_type.starts_with("@cap.")
}

fn typed_line_type(trimmed_line: &str) -> Option<&str> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let ty = rhs.trim().split_whitespace().next()?;

    if ty.starts_with('"') || ty.is_empty() {
        None
    } else {
        Some(ty)
    }
}

fn validation_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                "whole-resource validators should use `validates resource ...`.",
            ));
        } else if trimmed.starts_with("validates ")
            && !trimmed.starts_with("validates resource ")
            && !trimmed.starts_with("validates field ")
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "field validators should use `validates field <name> ...`.",
            ));
        }
    }

    diagnostics
}

fn extension_declaration_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn extension_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
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

fn expected_extension_keyword(contract: &str) -> Option<&'static str> {
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

fn event_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn event_trace_trigger_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn collect_trace_events(source: &str) -> HashSet<String> {
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

fn event_locator_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn target_binding_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn rule_self_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn required_field_nil_rule_diagnostics(source: &str) -> Vec<Diagnostic> {
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

#[derive(Debug)]
struct CommandValidatorFacts {
    validators: Vec<(String, usize, String)>,
    requirements: HashSet<String>,
    has_blocking_validate: bool,
}

fn command_validator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_command: Option<CommandValidatorFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_validator_facts_diagnostics(command));
            }
            current_command = Some(CommandValidatorFacts {
                validators: Vec::new(),
                requirements: HashSet::new(),
                has_blocking_validate: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_validator_facts_diagnostics(command));
            }
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("validate @validator.") {
                command.has_blocking_validate = true;
            } else if let Some((binding, expression)) = trimmed
                .strip_prefix("let ")
                .and_then(|rest| rest.split_once('='))
            {
                if expression.trim().starts_with("@validator.") {
                    command.validators.push((
                        binding.trim().to_owned(),
                        line_index,
                        line.to_owned(),
                    ));
                }
            } else if let Some(requirement) = trimmed.strip_prefix("requires ") {
                command.requirements.insert(requirement.trim().to_owned());
            }
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_validator_facts_diagnostics(command));
    }

    diagnostics
}

fn command_validator_facts_diagnostics(command: CommandValidatorFacts) -> Vec<Diagnostic> {
    if command.has_blocking_validate {
        return Vec::new();
    }

    command
        .validators
        .into_iter()
        .filter(|(binding, _, _)| !command.requirements.contains(binding))
        .map(|(binding, line_index, line)| {
            simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "command-validator-result",
                &format!(
                    "validator result `{binding}` is computed but not required; use `validate @validator...` or `requires {binding}` so the command cannot continue after validation fails.",
                ),
            )
        })
        .collect()
}

#[derive(Debug)]
struct ApiContractFacts {
    line_index: usize,
    line: String,
    has_method: bool,
    has_path: bool,
    has_output: bool,
    has_policy: bool,
    has_handler: bool,
    routes: HashSet<String>,
    path_params: Vec<String>,
}

fn api_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_api: Option<ApiContractFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            if let Some(api) = current_api.take() {
                diagnostics.extend(api_facts_diagnostics(api));
            }
            current_api = Some(ApiContractFacts {
                line_index,
                line: line.to_owned(),
                has_method: false,
                has_path: false,
                has_output: false,
                has_policy: false,
                has_handler: false,
                routes: HashSet::new(),
                path_params: Vec::new(),
            });
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(api) = current_api.take() {
                diagnostics.extend(api_facts_diagnostics(api));
            }
            continue;
        }

        let Some(api) = current_api.as_mut() else {
            continue;
        };

        if leading_spaces(line) != 4 {
            continue;
        }

        if let Some(method) = trimmed.strip_prefix("method ") {
            api.has_method = true;
            if !matches!(method.trim(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "api-contract",
                    "api methods should be one of `GET`, `POST`, `PUT`, `PATCH`, or `DELETE`.",
                ));
            }
        } else if let Some(path) = trimmed.strip_prefix("path ") {
            api.has_path = true;
            let path = unquote_lzx_literal(path.trim());
            if !path.starts_with('/') {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "api-contract",
                    "api paths should be absolute and start with `/`.",
                ));
            }
            api.path_params.extend(lzx_declared_path_params(path));
        } else if let Some(route) = trimmed.strip_prefix("route ") {
            if let Some(name) = route_slot_name(route) {
                api.routes.insert(name.to_owned());
            }
        } else if let Some(output) = trimmed.strip_prefix("output ") {
            api.has_output = true;
            if output.trim_start().starts_with("stream ") {
                let stream_type = output.trim_start().trim_start_matches("stream ").trim();
                if stream_type.is_empty() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "api-contract",
                        "streaming APIs use `output stream <Type>` so generated clients know the stream item shape.",
                    ));
                }
            }
        } else if trimmed.starts_with("policy ") {
            api.has_policy = true;
        } else if trimmed.starts_with("handler ") {
            api.has_handler = true;
        }
    }

    if let Some(api) = current_api {
        diagnostics.extend(api_facts_diagnostics(api));
    }

    diagnostics
}

fn api_facts_diagnostics(api: ApiContractFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut missing = Vec::new();

    if !api.has_method {
        missing.push("method");
    }
    if !api.has_path {
        missing.push("path");
    }
    if !api.has_output {
        missing.push("output");
    }
    if !api.has_policy {
        missing.push("policy");
    }
    if !api.has_handler {
        missing.push("handler");
    }

    if !missing.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            api.line_index,
            &api.line,
            DiagnosticSeverity::ERROR,
            "api-contract",
            &format!(
                "custom APIs should declare {} so HTTP shape, authorization, generated clients, and handler boundaries are explicit.",
                missing.join(", ")
            ),
        ));
    }

    for path_param in api.path_params {
        if !api.routes.contains(&path_param) {
            diagnostics.push(simple_canonical_diagnostic(
                api.line_index,
                &api.line,
                DiagnosticSeverity::WARNING,
                "api-route-contract",
                &format!(
                    "api path parameter `{path_param}` should be declared with `route {path_param}: <Type>` so generated handlers and clients are type-safe.",
                ),
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
struct QueryCacheFacts {
    line_index: usize,
    line: String,
    has_key: bool,
    has_ttl: bool,
}

#[derive(Debug)]
struct CommandInvalidationFacts {
    line_index: usize,
    line: String,
    entries: usize,
}

fn cache_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_query = false;
    let mut in_command = false;
    let mut current_cache: Option<QueryCacheFacts> = None;
    let mut current_invalidates: Option<CommandInvalidationFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some(cache) = current_cache.take() {
                diagnostics.extend(query_cache_diagnostics(cache));
            }
        }
        if leading_spaces(line) <= 4 {
            if let Some(invalidates) = current_invalidates.take() {
                diagnostics.extend(command_invalidation_diagnostics(invalidates));
            }
        }

        match leading_spaces(line) {
            2 => {
                in_command = trimmed.starts_with("command ");
                in_query = false;
            }
            4 => {
                in_query = trimmed.starts_with("query.");
                if !trimmed.starts_with("command ") {
                    in_command = in_command && !trimmed.starts_with("api ");
                }
            }
            _ => {}
        }

        if leading_spaces(line) == 6 && trimmed == "cache" {
            if !in_query {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-contract",
                    "`cache` belongs under a `query.*` declaration.",
                ));
                continue;
            }
            current_cache = Some(QueryCacheFacts {
                line_index,
                line: line.to_owned(),
                has_key: false,
                has_ttl: false,
            });
            continue;
        }

        if let Some(cache) = current_cache.as_mut()
            && leading_spaces(line) == 8
        {
            if trimmed.starts_with("key ") {
                cache.has_key = true;
            } else if let Some(ttl) = trimmed.strip_prefix("ttl ") {
                cache.has_ttl = true;
                let value = unquote_lzx_literal(ttl.trim());
                if !ttl.trim().starts_with('"') && !is_duration_literal(value) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "cache-contract",
                        "cache ttl should be quoted prose or a duration literal such as `30s`, `10m`, `1h`, or `7d`.",
                    ));
                }
            }
            continue;
        }

        if leading_spaces(line) == 4 && trimmed == "invalidates" {
            if !in_command {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-invalidation-contract",
                    "`invalidates` belongs as a command child.",
                ));
                continue;
            }
            current_invalidates = Some(CommandInvalidationFacts {
                line_index,
                line: line.to_owned(),
                entries: 0,
            });
            continue;
        }

        if let Some(invalidates) = current_invalidates.as_mut()
            && leading_spaces(line) == 6
        {
            if !trimmed.contains(".query.") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-invalidation-contract",
                    "cache invalidation entries should target explicit queries such as `customer.query.list` or `customer.query.by_id(id: route.id)`.",
                ));
            }
            invalidates.entries += 1;
        }
    }

    if let Some(cache) = current_cache {
        diagnostics.extend(query_cache_diagnostics(cache));
    }
    if let Some(invalidates) = current_invalidates {
        diagnostics.extend(command_invalidation_diagnostics(invalidates));
    }

    diagnostics
}

fn query_cache_diagnostics(cache: QueryCacheFacts) -> Vec<Diagnostic> {
    if cache.has_key && cache.has_ttl {
        return Vec::new();
    }

    let mut missing = Vec::new();
    if !cache.has_key {
        missing.push("key");
    }
    if !cache.has_ttl {
        missing.push("ttl");
    }

    vec![simple_canonical_diagnostic(
        cache.line_index,
        &cache.line,
        DiagnosticSeverity::WARNING,
        "cache-contract",
        &format!(
            "query cache contracts should declare {} so generated clients can share stable cache keys and stale-time behavior.",
            missing.join(", ")
        ),
    )]
}

fn command_invalidation_diagnostics(invalidates: CommandInvalidationFacts) -> Vec<Diagnostic> {
    if invalidates.entries > 0 {
        return Vec::new();
    }

    vec![simple_canonical_diagnostic(
        invalidates.line_index,
        &invalidates.line,
        DiagnosticSeverity::WARNING,
        "cache-invalidation-contract",
        "`invalidates` should list at least one query target.",
    )]
}

fn error_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature_errors = false;
    let mut in_command_errors = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) <= 2 {
            in_feature_errors = leading_spaces(line) == 2 && trimmed == "errors";
            in_command_errors = false;
            continue;
        }

        if leading_spaces(line) == 4 && trimmed == "errors" {
            in_command_errors = true;
            continue;
        }

        if leading_spaces(line) <= 4 && in_command_errors {
            in_command_errors = false;
        }

        if in_feature_errors && leading_spaces(line) == 4 {
            if let Some(mode) = trimmed.strip_prefix("default ") {
                if !matches!(mode.trim(), "hide" | "expose") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "error-contract",
                        "feature error defaults use `default hide` or `default expose`.",
                    ));
                }
            } else if !valid_error_exposure_line(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    "error exposure uses `expose client 4xx|5xx message, code, data` so public/private error payloads are explicit.",
                ));
            }
        }

        if trimmed.starts_with("error ") {
            if let Some(message) = error_case_contract_error(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    message,
                ));
            }
        }

        if in_command_errors && leading_spaces(line) == 6 {
            let candidate = format!("error {trimmed}");
            if let Some(message) = error_case_contract_error(&candidate) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    message,
                ));
            }
        }
    }

    diagnostics
}

fn valid_error_exposure_line(line: &str) -> bool {
    let parts: Vec<_> = line.split_whitespace().collect();
    parts.len() >= 4
        && parts[0] == "expose"
        && parts[1] == "client"
        && matches!(parts[2], "4xx" | "5xx")
        && error_exposure_fields_valid(parts[3..].join(" ").as_str())
}

fn error_case_contract_error(line: &str) -> Option<&'static str> {
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() < 6 || parts[0] != "error" || parts[2] != "status" || parts[4] != "expose" {
        return Some(
            "error cases use `error <Name> status <http-status> expose message, code, data`.",
        );
    }

    if !is_http_status_code(parts[3]) {
        return Some("error status should be an HTTP status code from 100 to 599.");
    }

    if !error_exposure_fields_valid(parts[5..].join(" ").as_str()) {
        return Some("error exposure fields are limited to `message`, `code`, and `data`.");
    }

    None
}

fn error_exposure_fields_valid(fields: &str) -> bool {
    fields
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .all(|field| matches!(field, "message" | "code" | "data"))
}

fn is_http_status_code(value: &str) -> bool {
    matches!(value.parse::<u16>(), Ok(status) if (100..=599).contains(&status))
}

fn collect_required_resource_fields(source: &str) -> HashSet<(String, String, String)> {
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

fn predicate_references_nil_self_field(predicate: &str, field: &str) -> bool {
    let left = format!("self.{field}");
    predicate.contains(&format!("{left} = nil")) || predicate.contains(&format!("{left} != nil"))
}

fn legacy_rule_subject_alias(predicate: &str) -> Option<&str> {
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
struct AnchorWhitelistEntry {
    anchor: String,
    feature: String,
    line_index: usize,
    line: String,
}

fn anchor_whitelist_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn test_block_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut current_test_context: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = leading_spaces(line);

        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }

        if current_test_context.is_some() && indent <= 4 {
            current_test_context = None;
        }

        if trimmed == "tests" {
            let context = test_context(&stack);
            if let Some(context) = context {
                current_test_context = Some(context);
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

fn stack_kind(trimmed_line: &str) -> Option<&'static str> {
    let first = trimmed_line.split_whitespace().next()?;

    if first == "command" {
        Some("command")
    } else if first == "rule" {
        Some("rule")
    } else if view_anchor(trimmed_line).is_some() {
        Some("anchor")
    } else if is_transition_line(trimmed_line) {
        Some("transition")
    } else {
        None
    }
}

fn test_context(stack: &[(usize, String)]) -> Option<String> {
    stack
        .last()
        .filter(|(_, kind)| matches!(kind.as_str(), "command" | "transition" | "rule" | "anchor"))
        .map(|(_, kind)| kind.clone())
}

fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((lhs, rhs)) = trimmed_line.split_once(':') else {
        return false;
    };

    !lhs.trim().is_empty() && rhs.contains("->")
}

fn is_valid_test_assertion(context: &str, trimmed_line: &str) -> bool {
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

fn view_anchor(trimmed_line: &str) -> Option<&str> {
    let marker = " id @anchor.";
    let (_, rest) = trimmed_line.split_once(marker)?;
    rest.split_whitespace().next()
}

fn extends_anchor(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line.strip_prefix("extends @anchor.")?;
    rest.split_whitespace().next()
}

fn extensible_by_features(trimmed_line: &str) -> Vec<String> {
    let Some(rest) = trimmed_line.strip_prefix("extensible_by ") else {
        return Vec::new();
    };

    rest.split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect()
}

fn extension_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn idempotency_key_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("idempotency ") && !trimmed.starts_with("idempotency by ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "idempotency-by",
                "`idempotency` should declare its source with `by`, e.g. `idempotency by envelope.id` for event jobs or `idempotency by payload.external_id` for webhooks.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
struct SensitiveFieldFacts {
    feature: String,
    resource: String,
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug, Default)]
struct FieldPolicyFacts {
    read: bool,
    write: bool,
}

fn field_security_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn collect_sensitive_fields(source: &str) -> Vec<SensitiveFieldFacts> {
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

fn is_sensitive_field_line(line: &str) -> bool {
    line.contains("@pii.")
        || line.contains("@cap.Encrypted")
        || line.contains("@cap.E2ee")
        || line.contains("@cap.Hashed")
        || line.contains("@cap.Token")
}

#[derive(Debug)]
struct PiiResourceFacts {
    feature: String,
    resource: String,
    line_index: usize,
    line: String,
}

fn retention_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
struct RetentionFacts {
    feature_defaults: HashSet<String>,
    resources: HashSet<(String, String)>,
}

fn collect_retention_facts(source: &str, diagnostics: &mut Vec<Diagnostic>) -> RetentionFacts {
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

fn retention_contract_error(trimmed_line: &str) -> Option<&'static str> {
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

fn collect_pii_resource_facts(source: &str) -> Vec<PiiResourceFacts> {
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

fn write_window_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
struct AppOperationalFacts {
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
struct AppRuntimeUnitFacts {
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
struct AppServiceFacts {
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
struct AppIntegrationFacts;

impl AppIntegrationFacts {
    fn new() -> Self {
        Self
    }
}

fn app_operational_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                        "app manifests own app/runtime contracts: use `uses`, `targets`, `environments`, `urls`, `env`, `integrations`, `capabilities`, `architecture`, `services`, `communication`, `runtime`, or `deploy` blocks.",
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
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "six-space app manifest declarations are only valid inside `env group`, `integrations`, `runtime unit`, or `services service` blocks.",
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
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "eight-space app manifest declarations are only valid inside `integrations credentials` or `services service exposes` blocks.",
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

fn app_child_block(trimmed: &str) -> Option<&'static str> {
    let first = trimmed.split_whitespace().next()?;
    match first {
        "uses" => Some("uses"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

fn is_app_scalar_child(trimmed: &str) -> bool {
    matches!(
        trimmed.split_whitespace().next(),
        Some(
            "title"
                | "version"
                | "default_locale"
                | "default_timezone"
                | "auth_failed_redirect"
                | "not_found"
        )
    )
}

fn validate_app_child_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    if matches!(
        first,
        "targets"
            | "environments"
            | "urls"
            | "env"
            | "integrations"
            | "capabilities"
            | "architecture"
            | "services"
            | "communication"
            | "runtime"
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

fn validate_app_scalar_child(
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

fn validate_app_architecture_line(
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

fn validate_app_communication_line(
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

fn validate_app_service_child(
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

fn validate_app_service_exposure_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2 || !matches!(parts[0], "query" | "command" | "api" | "workflow") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-service-contract",
            "service exposures use `query|command|api|workflow <feature>.<kind>.<name>`.",
        ));
    }
}

fn validate_app_target_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn validate_app_url_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn validate_app_env_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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
    if parts[0] == "client" && !name.starts_with("PUBLIC_") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "env-client-exposure",
            "client env names should use a `PUBLIC_` prefix so secret/server-only values are not accidentally bundled.",
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

fn valid_env_declaration_parts(parts: &[&str]) -> bool {
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

fn parse_env_group_name(trimmed: &str) -> Option<&str> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "group" && is_identifier(parts[1]) {
        Some(parts[1])
    } else {
        None
    }
}

fn validate_app_integration_header(
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

fn parse_app_integration_header(trimmed: &str) -> Option<(&str, &str)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name, kind))
    } else {
        None
    }
}

fn validate_app_integration_child(
    diagnostics: &mut Vec<Diagnostic>,
    current_integration_child: &mut Option<&'static str>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["adapter", adapter] if adapter.starts_with("@adapter.") => {
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
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration children use `adapter @adapter.<name>`, `environments ...`, `credentials platform|tenant|actor`, or `data_classification @pii.<class>`.",
        )),
    }
}

fn validate_app_integration_credential_line(
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

fn validate_app_capability_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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
                | "integration"
                | "payment_gateway"
                | "credit_bureau"
        )
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-capability-contract",
            "app capabilities declare intent such as `database postgres`, `queue background_jobs`, `object_storage files`, or `integration crm`; providers stay in Drusa adapters.",
        ));
    }
}

fn validate_app_deploy_line(
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
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-deploy-contract",
            "deploy contracts use `migrations before_deploy|manual|disabled`, `migration_lock required|optional`, `destructive_migrations require_approval|forbidden`, and `rollback on_failed_healthcheck|manual|disabled`.",
        )),
    }
}

fn validate_app_runtime_unit_child(
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

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "app-runtime-contract",
        "runtime unit children use `serves ...`, `runs ...`, `healthcheck \"...\"`, or `readiness \"...\"`.",
    ));
}

fn app_operational_block_diagnostics(app: AppOperationalFacts) -> Vec<Diagnostic> {
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
            "app manifests must declare `targets` so Drusa can materialize backend, web, and mobile outputs deterministically.",
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

fn env_schema_diagnostics(source: &str) -> Vec<Diagnostic> {
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

        if parts[0] == "client" && !name.starts_with("PUBLIC_") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "env-client-exposure",
                "client env names should use a `PUBLIC_` prefix so secret/server-only values are not accidentally bundled.",
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
                        "environment reference `env.{reference}` should be declared in an app or top-level `env` block with scope, type, and requiredness.",
                    ),
                ));
            }
        }
    }

    diagnostics
}

fn collect_field_policy_facts(source: &str) -> HashMap<(String, String, String), FieldPolicyFacts> {
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

#[derive(Debug)]
struct WebhookSecurityFacts {
    line_index: usize,
    line: String,
    has_verify: bool,
    verify_none: Option<(usize, String)>,
    verify_none_has_reason: bool,
    has_idempotency: bool,
}

fn webhook_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_webhook: Option<WebhookSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_diagnostics(webhook));
            }
            current_webhook = Some(WebhookSecurityFacts {
                line_index,
                line: line.to_owned(),
                has_verify: false,
                verify_none: None,
                verify_none_has_reason: false,
                has_idempotency: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_diagnostics(webhook));
            }
            continue;
        }

        let Some(webhook) = current_webhook.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed == "verify none" {
                webhook.has_verify = true;
                webhook.verify_none = Some((line_index, line.to_owned()));
            } else if trimmed.starts_with("verify ") {
                webhook.has_verify = true;
            } else if trimmed.starts_with("idempotency by ") {
                webhook.has_idempotency = true;
            }
        } else if leading_spaces(line) == 6
            && webhook.verify_none.is_some()
            && trimmed.starts_with("reason ")
        {
            webhook.verify_none_has_reason = true;
        }
    }

    if let Some(webhook) = current_webhook {
        diagnostics.extend(webhook_diagnostics(webhook));
    }

    diagnostics
}

fn webhook_diagnostics(webhook: WebhookSecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !webhook.has_verify {
        diagnostics.push(simple_canonical_diagnostic(
            webhook.line_index,
            &webhook.line,
            DiagnosticSeverity::WARNING,
            "webhook-verify",
            "webhooks are inbound trust boundaries and must declare `verify ...` or explicit `verify none` with a `reason` child.",
        ));
    }

    if !webhook.has_idempotency {
        diagnostics.push(simple_canonical_diagnostic(
            webhook.line_index,
            &webhook.line,
            DiagnosticSeverity::WARNING,
            "webhook-idempotency",
            "webhooks must declare `idempotency by payload.<business_key>` so verified inbound deliveries cannot be replayed silently.",
        ));
    }

    if let Some((line_index, line)) = webhook.verify_none {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "security-opt-out",
            "`verify none` is an explicit security opt-out. Strict profile allows it for reviewed drafts; production profile treats it as a release blocker.",
        ));

        if !webhook.verify_none_has_reason {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "security-opt-out-reason",
                "`verify none` must include a `reason \"...\"` child.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
struct WebhookTenantFacts {
    feature: String,
    line_index: usize,
    line: String,
    has_tenant_from: bool,
    has_global_scope: bool,
}

fn webhook_tenant_from_diagnostics(source: &str) -> Vec<Diagnostic> {
    let tenant_axes = collect_feature_tenant_axes(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_webhook: Option<WebhookTenantFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            current_webhook = current_feature.as_ref().map(|feature| WebhookTenantFacts {
                feature: feature.clone(),
                line_index,
                line: line.to_owned(),
                has_tenant_from: false,
                has_global_scope: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            continue;
        }

        let Some(webhook) = current_webhook.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("tenant_from ") {
                webhook.has_tenant_from = true;
            } else if trimmed.starts_with("scope global") {
                webhook.has_global_scope = true;
            }
        }
    }

    if let Some(webhook) = current_webhook {
        diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
    }

    diagnostics
}

fn webhook_tenant_from_facts_diagnostics(
    webhook: WebhookTenantFacts,
    tenant_axes: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    if webhook.has_tenant_from || webhook.has_global_scope {
        return Vec::new();
    }

    let Some(axes) = tenant_axes
        .get(&webhook.feature)
        .filter(|axes| !axes.is_empty())
    else {
        return Vec::new();
    };
    let mut axes: Vec<_> = axes.iter().cloned().collect();
    axes.sort();
    let payload_hints: Vec<_> = axes
        .iter()
        .map(|axis| format!("`tenant_from payload.{axis}_id`"))
        .collect();

    vec![simple_canonical_diagnostic(
        webhook.line_index,
        &webhook.line,
        DiagnosticSeverity::WARNING,
        "webhook-tenant-from",
        &format!(
            "webhook in tenant-scoped feature `{}` should declare {} or explicit `scope global` with a reason.",
            webhook.feature,
            payload_hints.join(" or ")
        ),
    )]
}

#[derive(Debug)]
struct EscapeRouteSecurityFacts {
    line_index: usize,
    line: String,
    has_policy: bool,
    has_tenant: bool,
}

fn escape_route_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_escape_route: Option<EscapeRouteSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 && trimmed.starts_with("escape_route ") {
            if let Some(route) = current_escape_route.take() {
                diagnostics.extend(escape_route_diagnostics(route));
            }
            current_escape_route = Some(EscapeRouteSecurityFacts {
                line_index,
                line: line.to_owned(),
                has_policy: false,
                has_tenant: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(route) = current_escape_route.take() {
                diagnostics.extend(escape_route_diagnostics(route));
            }
            continue;
        }

        let Some(route) = current_escape_route.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("policy ") {
                route.has_policy = true;
            } else if trimmed.starts_with("tenant ") {
                route.has_tenant = true;
            }
        }
    }

    if let Some(route) = current_escape_route {
        diagnostics.extend(escape_route_diagnostics(route));
    }

    diagnostics
}

fn escape_route_diagnostics(route: EscapeRouteSecurityFacts) -> Vec<Diagnostic> {
    let mut missing = Vec::new();
    if !route.has_policy {
        missing.push("policy");
    }
    if !route.has_tenant {
        missing.push("tenant");
    }

    if missing.is_empty() {
        Vec::new()
    } else {
        vec![simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::WARNING,
            "escape-route-security",
            &format!(
                "`escape_route` is outside generated UI ownership and must declare {}.",
                missing.join(" and ")
            ),
        )]
    }
}

#[derive(Debug, Default)]
struct AuthSecurityFacts {
    password_line: Option<(usize, String)>,
    password_algorithm: bool,
    password_rate_limit: bool,
    sessions_line: Option<(usize, String)>,
    sessions_ttl: bool,
}

fn auth_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_top: Option<&str> = None;
    let mut auth = AuthSecurityFacts::default();
    let mut auth_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            if current_top == Some("auth") {
                diagnostics.extend(auth_diagnostics(std::mem::take(&mut auth)));
            }
            current_top = trimmed.split_whitespace().next();
            auth_child = None;
            continue;
        }

        if current_top != Some("auth") {
            continue;
        }

        if leading_spaces(line) == 4 {
            if trimmed == "password" {
                auth.password_line = Some((line_index, line.to_owned()));
                auth_child = Some("password");
            } else if trimmed == "sessions" {
                auth.sessions_line = Some((line_index, line.to_owned()));
                auth_child = Some("sessions");
            } else {
                auth_child = None;
            }
        } else if leading_spaces(line) == 6 {
            match auth_child {
                Some("password") => {
                    if trimmed.starts_with("algorithm ") {
                        auth.password_algorithm = true;
                    } else if trimmed.starts_with("rate_limit ") {
                        auth.password_rate_limit = true;
                    }
                }
                Some("sessions") => {
                    if trimmed.starts_with("ttl ") {
                        auth.sessions_ttl = true;
                    }
                }
                _ => {}
            }
        }
    }

    if current_top == Some("auth") {
        diagnostics.extend(auth_diagnostics(auth));
    }

    diagnostics
}

fn auth_diagnostics(auth: AuthSecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some((line_index, line)) = auth.password_line {
        if !auth.password_algorithm {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "auth-password-algorithm",
                "`auth password` must declare `algorithm <name>` so the password hash contract is audit-visible.",
            ));
        }
        if !auth.password_rate_limit {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "auth-password-rate-limit",
                "`auth password` must declare a `rate_limit` for credential guessing protection.",
            ));
        }
    }

    if let Some((line_index, line)) = auth.sessions_line
        && !auth.sessions_ttl
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "auth-session-ttl",
            "`auth sessions` must declare `ttl` so generated session lifetime is explicit.",
        ));
    }

    diagnostics
}

fn apply_security_profile(
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

fn diagnostic_code(diagnostic: &Diagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        tower_lsp::lsp_types::NumberOrString::String(code) => Some(code.as_str()),
        tower_lsp::lsp_types::NumberOrString::Number(_) => None,
    }
}

fn is_security_enforcement_code(code: &str) -> bool {
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
            | "security-opt-out-reason"
    )
}

fn is_security_opt_out_code(code: &str) -> bool {
    matches!(code, "security-opt-out")
}

fn simple_canonical_diagnostic(
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
struct CanonicalFeatureFacts {
    default_tenancy: Option<String>,
    default_timestamps: bool,
    resources: HashMap<String, CanonicalResourceFacts>,
}

#[derive(Debug)]
struct CanonicalResourceFacts {
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

fn event_payload_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
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
struct EventPayloadGroup {
    prefix: String,
    fields: HashSet<String>,
}

#[derive(Debug)]
struct JobPayloadReference {
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug)]
struct EventTriggeredJobFacts {
    feature: String,
    trigger: Option<String>,
    payload_references: Vec<JobPayloadReference>,
}

fn event_consumer_payload_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn event_triggered_job_payload_diagnostics(
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
struct EventJobTenantFacts {
    feature: String,
    line_index: usize,
    line: String,
    trigger: Option<String>,
    tenant_from: Option<String>,
}

fn event_job_tenant_from_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn event_job_tenant_from_diagnostic(
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
struct ScheduledJobFacts {
    feature: String,
    line_index: usize,
    line: String,
    is_scheduled: bool,
    has_tenant_fanout: bool,
    has_global_scope: bool,
}

fn scheduled_job_tenancy_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn scheduled_job_tenancy_facts_diagnostics(
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

fn collect_feature_tenant_axes(source: &str) -> HashMap<String, HashSet<String>> {
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

fn insert_tenant_axis(
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

fn collect_event_contracts(source: &str) -> HashMap<String, HashSet<String>> {
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

fn event_decl_name(trimmed_line: &str) -> Option<&str> {
    if trimmed_line.starts_with("event.trace ") || trimmed_line.starts_with("event ") {
        trimmed_line.split_whitespace().nth(1)
    } else {
        None
    }
}

fn event_group_prefix(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }
    parts.next()?.strip_suffix('*')
}

fn qualify_group_event_name(prefix: &str, raw_name: &str) -> String {
    if raw_name.starts_with(prefix) {
        raw_name.to_owned()
    } else {
        format!("{prefix}{raw_name}")
    }
}

fn payload_assignment_field(trimmed_line: &str) -> Option<&str> {
    let (field, _) = trimmed_line.split_once('=')?;
    let field = field.trim();
    (!field.is_empty()).then_some(field)
}

fn payload_field_references(line: &str) -> Vec<String> {
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

fn event_consumer_payload_diagnostic(
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

fn collect_canonical_feature_facts(source: &str) -> HashMap<String, CanonicalFeatureFacts> {
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

fn tenancy_axis(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "tenancy" {
        parts.next()
    } else {
        None
    }
}

fn resource_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "resource" {
        parts.next()
    } else {
        None
    }
}

fn events_resource_name(trimmed_line: &str) -> Option<&str> {
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

fn field_name(trimmed_line: &str) -> Option<&str> {
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

fn payload_assignment_rhs(trimmed_line: &str) -> Option<&str> {
    let (_, rhs) = trimmed_line.split_once('=')?;
    Some(rhs.trim())
}

fn resource_field_reference(expression: &str) -> Option<&str> {
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

fn event_payload_reference_diagnostic(
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

#[derive(Debug)]
struct CanonicalCommandFacts {
    feature_name: Option<String>,
    name: String,
    line_index: usize,
    line: String,
    route_slots: HashSet<String>,
    route_references: Vec<CommandRouteReference>,
    short_inputs: Vec<CommandShortInput>,
    typed_inputs: Vec<CommandShortInput>,
    input_inference_resources: Vec<String>,
    from_input_creates: Option<(String, usize, String)>,
    create_assignment_references: HashSet<String>,
    has_policy: bool,
    has_target: bool,
    has_write_effect: bool,
    needs_default_route_target: bool,
}

impl CanonicalCommandFacts {
    fn new(feature_name: Option<String>, name: String, line_index: usize, line: &str) -> Self {
        Self {
            feature_name,
            name,
            line_index,
            line: line.to_owned(),
            route_slots: HashSet::new(),
            route_references: Vec::new(),
            short_inputs: Vec::new(),
            typed_inputs: Vec::new(),
            input_inference_resources: Vec::new(),
            from_input_creates: None,
            create_assignment_references: HashSet::new(),
            has_policy: false,
            has_target: false,
            has_write_effect: false,
            needs_default_route_target: false,
        }
    }
}

#[derive(Debug)]
struct CommandRouteReference {
    name: String,
    line_index: usize,
    line: String,
}

#[derive(Debug)]
struct CommandShortInput {
    name: String,
    line_index: usize,
    line: String,
}

fn command_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let features = collect_canonical_feature_facts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_command: Option<CanonicalCommandFacts> = None;
    let mut current_command_child: Option<&str> = None;
    let mut current_create_from_input = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }

            current_feature = Some(feature_name(trimmed));
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }

            current_command = Some(CanonicalCommandFacts::new(
                current_feature.clone(),
                command_name(trimmed),
                line_index,
                line,
            ));
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if let Some(route_slot) = command_route_slot(trimmed) {
                command.route_slots.insert(route_slot.to_owned());
            }

            if trimmed.starts_with("policy ") {
                command.has_policy = true;
                current_command_child = None;
                current_create_from_input = false;
            } else if trimmed.starts_with("target ") {
                command.has_target = true;
                current_command_child = None;
                current_create_from_input = false;
            } else if let Some(input_fields) = command_short_input_fields(trimmed) {
                command
                    .short_inputs
                    .extend(
                        input_fields
                            .into_iter()
                            .map(|field_name| CommandShortInput {
                                name: field_name,
                                line_index,
                                line: line.to_owned(),
                            }),
                    );
                current_command_child = None;
                current_create_from_input = false;
            } else if trimmed == "input" {
                current_command_child = Some("input");
                current_create_from_input = false;
            } else if let Some((effect, resource_name)) = command_write_effect(trimmed) {
                command.has_write_effect = true;
                command.needs_default_route_target = matches!(effect, "updates" | "deletes");
                if matches!(effect, "creates" | "updates") {
                    command
                        .input_inference_resources
                        .push(resource_name.to_owned());
                }
                current_create_from_input = false;
                current_command_child = None;
                if effect == "creates" && trimmed.contains(" from input") {
                    command.from_input_creates =
                        Some((resource_name.to_owned(), line_index, line.to_owned()));
                    current_command_child = Some("creates");
                    current_create_from_input = true;
                }
            } else {
                current_command_child = None;
                current_create_from_input = false;
            }
        } else if leading_spaces(line) == 6 {
            if current_command_child == Some("input") {
                if let Some((name, _)) = typed_param(trimmed) {
                    command.typed_inputs.push(CommandShortInput {
                        name: name.to_owned(),
                        line_index,
                        line: line.to_owned(),
                    });
                }
            } else if current_command_child == Some("creates") && current_create_from_input {
                for input_name in input_references(line) {
                    command.create_assignment_references.insert(input_name);
                }
            }
        }

        for route_reference in route_references(line) {
            command.route_references.push(CommandRouteReference {
                name: route_reference,
                line_index,
                line: line.to_owned(),
            });
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_diagnostics(command, &features));
    }

    diagnostics
}

fn command_diagnostics(
    command: CanonicalCommandFacts,
    features: &HashMap<String, CanonicalFeatureFacts>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !command.has_policy {
        diagnostics.push(command_policy_diagnostic(
            command.line_index,
            &command.line,
            &command.name,
        ));
    }

    for reference in command.route_references {
        if !command.route_slots.contains(&reference.name) {
            diagnostics.push(command_route_diagnostic(
                reference.line_index,
                &reference.line,
                &command.name,
                &reference.name,
            ));
        }
    }

    if command.has_write_effect
        && command.needs_default_route_target
        && !command.has_target
        && !command.route_slots.contains("id")
    {
        diagnostics.push(command_default_route_diagnostic(
            command.line_index,
            &command.line,
            &command.name,
        ));
    }

    if !command.short_inputs.is_empty() {
        let local_feature = command
            .feature_name
            .as_deref()
            .and_then(|feature_name| features.get(feature_name));
        let inference_resources: Vec<_> = command
            .input_inference_resources
            .iter()
            .filter_map(|resource_name| {
                local_feature
                    .and_then(|feature| feature.resources.get(resource_name))
                    .map(|resource| (resource_name.as_str(), resource))
            })
            .collect();

        if command.input_inference_resources.len() > 1 || inference_resources.len() > 1 {
            for input in &command.short_inputs {
                diagnostics.push(command_short_input_ambiguous_resource_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                ));
            }

            return diagnostics;
        }

        if inference_resources.is_empty() {
            for input in &command.short_inputs {
                diagnostics.push(command_short_input_without_resource_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                ));
            }

            return diagnostics;
        }

        let (resource_name, resource) = inference_resources[0];

        for input in &command.short_inputs {
            if !resource.fields.contains(&input.name) {
                diagnostics.push(command_short_input_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                    resource_name,
                ));
            }
        }
    }

    if let Some((resource_name, _, _)) = command.from_input_creates.as_ref() {
        let local_feature = command
            .feature_name
            .as_deref()
            .and_then(|feature_name| features.get(feature_name));
        let resource = local_feature.and_then(|feature| feature.resources.get(resource_name));
        let all_inputs = command
            .short_inputs
            .iter()
            .chain(command.typed_inputs.iter());

        if let Some(resource) = resource {
            for input in all_inputs {
                if !resource.fields.contains(&input.name)
                    && !command.create_assignment_references.contains(&input.name)
                {
                    diagnostics.push(command_from_input_unconsumed_diagnostic(
                        input.line_index,
                        &input.line,
                        &command.name,
                        &input.name,
                        resource_name,
                    ));
                }
            }
        }
    }

    diagnostics
}

fn command_name(trimmed_line: &str) -> String {
    trimmed_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("<anonymous>")
        .to_owned()
}

fn command_route_slot(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? != "route" {
        return None;
    }

    Some(parts.next()?.trim_end_matches(':'))
}

fn command_write_effect(trimmed_line: &str) -> Option<(&str, &str)> {
    let mut parts = trimmed_line.split_whitespace();
    let effect = parts.next()?;
    if matches!(effect, "creates" | "updates" | "deletes") {
        Some((effect, parts.next()?))
    } else {
        None
    }
}

fn command_short_input_fields(trimmed_line: &str) -> Option<Vec<String>> {
    let rest = trimmed_line.strip_prefix("input ")?;
    let fields: Vec<String> = rest
        .split(',')
        .map(str::trim)
        .filter(|field| {
            !field.is_empty()
                && !field.contains(':')
                && field
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned)
        .collect();

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

fn route_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("route.") {
        let after_prefix = &rest[start + "route.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let name = &after_prefix[..end];

        if !name.is_empty() {
            references.push(name.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}

fn input_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("input.") {
        let after_prefix = &rest[start + "input.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let name = &after_prefix[..end];

        if !name.is_empty() {
            references.push(name.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}

fn command_policy_diagnostic(line_index: usize, line: &str, command_name: &str) -> Diagnostic {
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
            "command-policy".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` should declare `policy` explicitly; canonical commands do not rely on effect-derived policy defaults."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_route_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
    route_name: &str,
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
            "command-route".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` references `route.{route_name}` but does not declare `route {route_name}: ...`."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_default_route_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
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
            "command-route-target".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` omits `target`; declare `route id: ID` when relying on the default `query.by_id(id: route.id)` target."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_short_input_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
    input_name: &str,
    resource_name: &str,
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
            "command-input-inference".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` uses short input `{input_name}`, but `{resource_name}` has no field named `{input_name}`. Use short `input a, b` only for fields inferred from a local `creates` or `updates` resource; use a typed input block for locator, adapter, optional, or reshaped data."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_short_input_without_resource_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
    input_name: &str,
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
            "command-input-inference".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` uses short input `{input_name}`, but short inputs require a local `creates` or `updates` resource for type inference. Use a typed input block for returns-only commands, locator values, adapter data, or fields whose shape differs from a resource field."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_short_input_ambiguous_resource_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
    input_name: &str,
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
            "command-input-inference".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` uses short input `{input_name}`, but short inputs require exactly one local `creates` or `updates` resource for type inference. Use a typed input block when multiple resources are involved."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn command_from_input_unconsumed_diagnostic(
    line_index: usize,
    line: &str,
    command_name: &str,
    input_name: &str,
    resource_name: &str,
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
            "command-from-input".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "command `{command_name}` uses `creates {resource_name} from input`, but input `{input_name}` is neither a `{resource_name}` field nor referenced explicitly in that creates block."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn format_canonical_source(source: &str) -> Option<String> {
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
struct FeatureBlockSegment {
    kind: Option<CanonicalBlockKind>,
    ordinal: usize,
    lines: Vec<String>,
}

fn format_feature_lines(lines: &[&str]) -> Vec<String> {
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

fn format_workflow_lines(lines: Vec<String>) -> Vec<String> {
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

fn is_trivia_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn feature_name(trimmed_line: &str) -> String {
    trimmed_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("<anonymous>")
        .to_owned()
}

fn range_from_span(source: &str, span: Span) -> Range {
    let len = source.len();
    let start = span.start.min(len);
    let end = span.end.max(span.start.saturating_add(1)).min(len);

    Range {
        start: position_for_offset(source, start),
        end: position_for_offset(source, end),
    }
}

fn first_line_range(source: &str) -> Range {
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

fn full_document_range(source: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: position_for_offset(source, source.len()),
    }
}

fn position_for_offset(source: &str, offset: usize) -> Position {
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

fn word_at_position(source: &str, position: Position) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let target = position.character as usize;
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

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "app" => Some("Declares the `.lzi` application entrypoint and operational contract."),
        "environments" => {
            Some("Declares deployment/runtime environments such as local, staging, and production.")
        }
        "urls" => {
            Some("Declares public app URLs used by clients, CORS, emails, callbacks, and webhooks.")
        }
        "capabilities" => Some(
            "Declares required runtime capabilities without choosing concrete infrastructure providers.",
        ),
        "integrations" => Some(
            "Declares external integration registry entries without provider-specific operation details.",
        ),
        "credentials" => {
            Some("Declares credential scope and bindings for an external integration.")
        }
        "data_classification" => Some("Declares the broad PII class returned by an integration."),
        "architecture" => Some(
            "Declares provider-neutral architecture mode and service-boundary enforcement intent.",
        ),
        "services" => Some("Declares logical service ownership boundaries for the app."),
        "service" => Some("Declares one logical app service boundary under `services`."),
        "owns" => Some("Declares which Lazuli features a logical service owns."),
        "exposes" => Some("Groups commands, queries, APIs, or workflows exposed by a service."),
        "publishes" => Some("Declares event patterns a logical service publishes."),
        "consumes" => Some("Declares external or cross-service events a logical service consumes."),
        "communication" => {
            Some("Declares sync/async intent and context propagation across service boundaries.")
        }
        "internal" => Some("Declares the internal sync communication contract."),
        "external" => Some("Declares the external communication contract."),
        "async" => Some("Declares the asynchronous communication contract."),
        "propagate" => Some("Declares context values propagated across service boundaries."),
        "timeout" => Some("Declares a default service-boundary timeout."),
        "runtime" => {
            Some("Declares generated runtime units such as api, web, worker, and scheduler.")
        }
        "unit" => Some("Declares one app runtime unit under the app manifest `runtime` block."),
        "mode" => Some("Declares the app architecture mode."),
        "service_ready" => Some(
            "Marks whether the app keeps service boundaries visible for future split deployments.",
        ),
        "enforce_service_boundaries" => {
            Some("Marks whether cross-service ownership boundaries should be enforced by tooling.")
        }
        "serves" => Some("Declares which contracts a runtime unit serves."),
        "runs" => Some("Declares which jobs or schedules a runtime unit runs."),
        "healthcheck" => Some("Declares a runtime healthcheck path for deploy safety."),
        "readiness" => Some("Declares a runtime readiness path for deploy safety."),
        "deploy" => {
            Some("Declares provider-neutral deploy gates such as migrations and rollback behavior.")
        }
        "migrations" => Some("Declares when deploy applies database migrations."),
        "migration_lock" => Some("Declares whether deploy must hold a migration lock."),
        "destructive_migrations" => Some("Declares how deploy handles destructive schema changes."),
        "rollback" => Some("Declares rollback behavior for failed deploy health checks."),
        "env" => Some("Declares typed environment variables and client/server exposure."),
        "aggregate" | "entity" => Some("Declares a domain resource with fields and behavior."),
        "record" => Some("Declares a non-persisted typed result/DTO shape."),
        "command" => Some("Declares a write operation for an aggregate."),
        "query" => Some("Declares a read operation for an aggregate."),
        "query.list" => Some("Declares a generated collection query."),
        "query.lookup" => Some("Declares a generated single-record lookup query."),
        "query.sql" => Some("Declares a query backed by an external SQL file."),
        "defaults" => Some("Declares repeated feature defaults such as tenancy and timestamps."),
        "domain" => Some("Groups resources, records, queries, rules, and events."),
        "policies" => Some("Declares feature-local policy categories and field policies."),
        "auth" => Some("Declares identity, credential, session, OAuth, or MFA contracts."),
        "errors" => Some("Declares public/private client error exposure defaults or cases."),
        "api" => {
            Some("Declares a custom typed HTTP endpoint outside command/query/webhook semantics.")
        }
        "event_group" => Some("Declares a shared same-feature event payload template."),
        "event.trace" => {
            Some("Declares an observability-only event that is outside the feature reaction graph.")
        }
        "tenancy" => Some("Declares the tenant axis for generated scope and indexes."),
        "timestamps" => Some("Adds generated created/updated timestamp fields."),
        "soft_delete" => Some("Adds generated soft-delete scope and delete semantics."),
        "retention" => Some("Declares data retention for a resource or feature default."),
        "paginate" => Some("Declares the positive default page size for a `query.list`."),
        "surface" => Some("Declares UI projections for list, form, and detail views."),
        "input" => Some("Lists fields accepted by a command."),
        "route" => Some(
            "Declares route or context values accepted by a command/view, or a top-level typed app route in `.lzx`.",
        ),
        "previously" => {
            Some("Declares identity continuity with an explicit `migrated` or `alias` mode.")
        }
        "migrated" => Some(
            "Marks a previous name as migration-only history, not a generated compatibility alias.",
        ),
        "alias" => Some(
            "Marks a previous name as a temporary compatibility alias still accepted by generated surfaces.",
        ),
        "path" => Some("Declares a concrete URL path for app routes, APIs, or webhooks."),
        "stack" => Some(
            "Legacy top-level `.lzx` mobile route syntax; prefer canonical `path` plus `surface ... mobile`.",
        ),
        "params" => Some("Declares typed route, API, or query parameters."),
        "to" => Some("Binds a top-level `.lzx` route to an abstract experience view."),
        "let" => Some("Binds a derived value for later command, job, or event expressions."),
        "policy" => Some("Associates a command with an authorization policy capability."),
        "policy_for" => Some("Declares a feature default policy for specific construct families."),
        "rate_limit" => Some("Declares a generated throttle policy for a command or auth flow."),
        "method" => Some("Declares the HTTP method for a custom API endpoint."),
        "output" => Some("Declares the response shape for a custom API endpoint."),
        "cache" => Some("Declares generated query cache identity and stale-time behavior."),
        "key" => Some("Declares a cache key, lookup key, or dedupe key depending on context."),
        "ttl" => Some("Declares a time-to-live contract."),
        "invalidates" => Some("Declares queries that become stale after a command succeeds."),
        "error" => Some("Declares a named public error case with status and exposure fields."),
        "expose" => Some("Declares which error fields are visible to generated clients."),
        "write_window" => Some("Declares the temporal write window checked before a command runs."),
        "idempotency" => Some("Declares a dedupe key for jobs and webhooks."),
        "job" => Some("Declares asynchronous or scheduled work."),
        "webhook" => Some("Declares a verified inbound HTTP integration boundary."),
        "trigger" => Some("Declares the event or schedule that starts a job."),
        "retry" => Some("Declares retry attempts and backoff for jobs or webhooks."),
        "queue" => Some("Declares an async queue lane for event-triggered jobs."),
        "tenant_from" => Some("Pins an event-triggered job tenant context from the event payload."),
        "fanout" => Some("Declares per-tenant expansion for scheduled jobs."),
        "reason" => Some("Documents why a dangerous declarative override is intentional."),
        "requires" => {
            Some("Declares an additional authority requirement for a workflow transition.")
        }
        "algorithm" => Some("Declares the current password/hash algorithm contract."),
        "secret" => Some("Declares the secret source for declarative webhook verification."),
        "header" => Some("Declares the signature header for declarative webhook verification."),
        "modifier" => Some("Attaches a query modifier extension to a generated query."),
        "from" => Some("Copies matching input fields into a create assignment."),
        "emits" => Some("Declares a domain event emitted by a command."),
        "anchor" => Some("Declares the extension anchor for a routed abstract view."),
        "extensible_by" => Some("Whitelists features allowed to extend a view anchor."),
        "tests" => Some(
            "Declares inline IR assertions for a command, transition, rule, or view extension.",
        ),
        "permits" => Some(
            "Generated command authorization assertion; authored command policy matrices are redundant with `policy @policy.*`.",
        ),
        "forbids" => Some(
            "Generated command authorization assertion; authored command policy matrices are redundant with `policy @policy.*`.",
        ),
        "allows" => Some("Declares a positive predicate or transition test assertion."),
        "deny" => Some("Declares a rule precondition that rejects an operation."),
        "denies" => Some("Declares a negative predicate or transition test assertion."),
        "accepted" => {
            Some("Declares that a view extension should be accepted by an anchor whitelist.")
        }
        "rejected" => {
            Some("Declares that a view extension should be rejected by an anchor whitelist.")
        }
        "search" => Some("Lists fields used by a query search index."),
        "filter" => Some("Lists fields available as query filters."),
        "list" => Some("Declares table/list fields for a surface."),
        "form" => Some("Declares editable form fields for a surface."),
        "detail" => Some("Declares read-only detail fields for a surface."),
        "columns" => Some("Introduces list columns."),
        "fields" => Some("Introduces form or detail fields."),
        "validate" => Some(
            "Runs a blocking command validator with `validate @validator.*`; legacy whole-resource validators should use `validates resource`.",
        ),
        "validates" => Some(
            "Attaches a scoped validator implementation: `validates resource` or `validates field <name>`.",
        ),
        "client" => Some("Declares a reusable client-side extension contract."),
        "fn" => Some("Declares a reusable server-side pure function extension contract."),
        "hook" => Some("Declares a reusable lifecycle hook extension contract."),
        "validator" => Some("Declares a reusable validator extension contract."),
        "adapter" => Some("Declares a reusable integration adapter extension contract."),
        "query_modifier" => Some("Declares a reusable query modifier extension contract."),
        "escape_route" => Some("Declares a custom route outside generated UI ownership."),
        "group" => Some("Groups related app env declarations without creating a namespace."),
        "required" => Some("Marks a field as required."),
        "unique" => Some("Marks a field as unique."),
        "default" => Some("Declares a default field value."),
        _ => None,
    }
}

const KEYWORDS: &[&str] = &[
    "app",
    "env",
    "aggregate",
    "entity",
    "record",
    "command",
    "query",
    "query.list",
    "query.lookup",
    "query.sql",
    "defaults",
    "domain",
    "policies",
    "errors",
    "auth",
    "api",
    "event_group",
    "event.trace",
    "tenancy",
    "timestamps",
    "soft_delete",
    "retention",
    "paginate",
    "experience",
    "surface",
    "imports",
    "uses",
    "targets",
    "environments",
    "urls",
    "group",
    "in",
    "integrations",
    "credentials",
    "data_classification",
    "capabilities",
    "architecture",
    "services",
    "service",
    "mode",
    "service_ready",
    "enforce_service_boundaries",
    "owns",
    "exposes",
    "publishes",
    "consumes",
    "communication",
    "internal",
    "external",
    "async",
    "propagate",
    "timeout",
    "runtime",
    "unit",
    "serves",
    "runs",
    "healthcheck",
    "readiness",
    "deploy",
    "migrations",
    "migration_lock",
    "destructive_migrations",
    "rollback",
    "view",
    "audience",
    "extends",
    "input",
    "route",
    "previously",
    "migrated",
    "alias",
    "path",
    "stack",
    "params",
    "to",
    "let",
    "policy",
    "policy_for",
    "rate_limit",
    "method",
    "output",
    "cache",
    "key",
    "ttl",
    "invalidates",
    "error",
    "expose",
    "write_window",
    "idempotency",
    "job",
    "webhook",
    "trigger",
    "retry",
    "queue",
    "tenant_from",
    "fanout",
    "reason",
    "requires",
    "algorithm",
    "secret",
    "header",
    "modifier",
    "from",
    "emits",
    "anchor",
    "extensible_by",
    "slot",
    "platforms",
    "tests",
    "allows",
    "permits",
    "forbids",
    "deny",
    "denies",
    "accepted",
    "rejected",
    "search",
    "filter",
    "list",
    "form",
    "detail",
    "columns",
    "fields",
    "validate",
    "validates",
    "client",
    "fn",
    "hook",
    "validator",
    "adapter",
    "query_modifier",
    "escape_route",
    "required",
    "unique",
    "default",
];

#[cfg(test)]
mod tests {
    use super::{
        SecurityProfile, diagnostics_for, diagnostics_for_uri, diagnostics_for_with_profile,
        format_canonical_source,
    };
    use tower_lsp::lsp_types::{DiagnosticSeverity, Url};

    #[test]
    fn canonical_order_accepts_feature_blocks_in_order() {
        let source = r#"
env
  server INBOUND_WEBHOOK_SECRET: Secret required

feature customer
  purpose "Customers"

  defaults
    tenancy org

  uses org

  domain
    resource Customer

  policies
    create: @role.admin
    update: @role.admin
    read: @scope.same_org

  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer

  api export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv)
    policy @policy.read
    handler "./api/export.go"

  workflow lifecycle on Customer.status
    policy @policy.update

  job sync
    trigger schedule "0 2 * * *"
    fanout tenants org

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    tenant_from payload.org_id
    idempotency by payload.id

  surface web admin
    view list Table

  extensions
    hook before_create: Hook[CreateCustomer]

  escape_route "/admin/customer-debug"
    at "./pages/customer_debug.tsx"
    policy @role.admin
    tenant org
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_order_accepts_full_capsule_fixture() {
        let diagnostics = diagnostics_for(include_str!(
            "../../../examples/full-capsule/full-capsule.lzi"
        ));

        assert!(
            diagnostics.is_empty(),
            "expected no canonical ordering diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_examples_satisfy_lsp_contracts() {
        let examples = [
            (
                "full-capsule-app.lzi",
                include_str!("../../../examples/full-capsule/app.lzi"),
            ),
            (
                "audit-log.lzi",
                include_str!("../../../examples/audit-log.lzi"),
            ),
            ("billing.lzi", include_str!("../../../examples/billing.lzi")),
            ("comment.lzi", include_str!("../../../examples/comment.lzi")),
            (
                "customer-capsule.lzi",
                include_str!("../../../examples/customer-capsule.lzi"),
            ),
            (
                "extension-points.lzi",
                include_str!("../../../examples/extension-points.lzi"),
            ),
            (
                "field-permissions.lzi",
                include_str!("../../../examples/field-permissions.lzi"),
            ),
            (
                "full-capsule.lzi",
                include_str!("../../../examples/full-capsule/full-capsule.lzi"),
            ),
            (
                "import-csv.lzi",
                include_str!("../../../examples/import-csv.lzi"),
            ),
            (
                "linear-issue.lzi",
                include_str!("../../../examples/linear-issue.lzi"),
            ),
            (
                "notification.lzi",
                include_str!("../../../examples/notification.lzi"),
            ),
            (
                "org-team.lzi",
                include_str!("../../../examples/org-team.lzi"),
            ),
            (
                "user-auth.lzi",
                include_str!("../../../examples/user-auth.lzi"),
            ),
        ];

        for (name, source) in examples {
            let diagnostics = diagnostics_for(source);
            assert!(
                diagnostics.is_empty(),
                "expected {name} to satisfy canonical LSP diagnostics, got: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn canonical_accepts_app_operational_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  version "0.1.0"

  uses
    customer

  targets
    backend go
    web react

  environments
    local
    production

  urls
    web local "http://localhost:3000"
    api production "https://api.acme.example"

  env
    server DATABASE_URL: Secret required
    group webhooks
      server CRM_WEBHOOK_SECRET: Secret required in production
    group public
      client PUBLIC_API_URL: Url required
    group mailer
      server MAILER_API_KEY: Secret required in production

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET

  capabilities
    database postgres
    queue background_jobs
    integration crm

  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true

  services
    service crm
      owns customer
      exposes
        query customer.query.list
        command customer.command.create
      publishes customer.*

  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id
    timeout default "2s"
    retry default 2 backoff exponential

  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"

    unit worker
      runs jobs *

  deploy
    migrations before_deploy
    migration_lock required
    destructive_migrations require_approval
    rollback on_failed_healthcheck
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_warns_for_incomplete_app_operational_manifest() {
        let source = r#"
app AcmeCRM
  targets
    backend go

  runtime
    unit api
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("app manifests should declare `uses`") })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("app manifests should declare `deploy`") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("runtime units should declare what they `serves` or `runs`")
        }));
    }

    #[test]
    fn lzx_examples_satisfy_lsp_contracts() {
        let examples = [
            (
                "customer-capsule.lzx",
                include_str!("../../../examples/customer-capsule.lzx"),
            ),
            (
                "customer-capsule.web.lzx",
                include_str!("../../../examples/customer-capsule.web.lzx"),
            ),
            (
                "full-capsule.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.lzx"),
            ),
            (
                "full-capsule.admin.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.admin.web.lzx"),
            ),
            (
                "full-capsule.public.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.public.web.lzx"),
            ),
            (
                "full-capsule.account.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.account.web.lzx"),
            ),
            (
                "full-capsule.sales.mobile.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.sales.mobile.lzx"),
            ),
        ];

        for (name, source) in examples {
            let diagnostics = diagnostics_for(source);
            assert!(
                diagnostics.is_empty(),
                "expected {name} to satisfy LZX diagnostics, got: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn canonical_events_payload_warns_for_unknown_resource_field() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
        team_id = team.id

    event customer_created
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("resource `Customer` has no field named `team`")
        );
    }

    #[test]
    fn canonical_command_warns_for_missing_policy() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  command create
    rate_limit "30 per hour per user"
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("command `create` should declare `policy` explicitly")
        );
    }

    #[test]
    fn canonical_refs_warn_when_manifest_drifts() {
        let source = r#"
feature customer
  purpose "Customers"

  refs
    core: @role

  policies
    create: @role.admin

  command create
    policy @policy.create
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("refs for feature `customer` is missing used namespaces: @policy")
        }));
    }

    #[test]
    fn canonical_warns_for_unknown_local_policy_reference() {
        let source = r#"
feature customer
  purpose "Customers"

  policies
    create: @role.admin

  command create
    policy @policy.update
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "`@policy.*` references should resolve to a feature-local policy category",
            )
        }));
    }

    #[test]
    fn canonical_warns_for_direct_policy_atom_in_command() {
        let source = r#"
feature customer
  purpose "Customers"

  command create
    policy @role.admin
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands and workflows should reference feature-local policy categories")
        }));
    }

    #[test]
    fn canonical_warns_for_scope_override_without_query_policy() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    query.list global_search
      scope override
        deleted_at = nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`scope override` replaces inherited tenant/soft-delete safety scope")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`scope override` should include a `reason")
        }));
    }

    #[test]
    fn canonical_warns_for_event_job_without_tenant_from() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    event customer_activated
      customer_id: ID
      org_id: ID

feature outreach
  purpose "Outreach"

  uses customer

  job send_welcome
    trigger event customer.customer_activated
    idempotency by envelope.id
    handler "./jobs/send_welcome.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `tenant_from payload.org_id`")
        }));
    }

    #[test]
    fn canonical_warns_for_public_command_without_rate_limit() {
        let source = r#"
feature user
  purpose "Users"

  policies
    login: @scope.public

  command login
    input
      email: @semantic.Email
      password: Text
    policy @policy.login
    returns AuthSession
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands that are public or mutate state must declare")
        }));
    }

    #[test]
    fn strict_profile_promotes_security_omissions_to_errors() {
        let source = r#"
feature customer
  purpose "Customers"

  command create
    creates Customer
"#;

        let prototype = diagnostics_for_with_profile(source, SecurityProfile::Prototype);
        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);

        assert!(prototype.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::WARNING)
                && diagnostic
                    .message
                    .contains("should declare `policy` explicitly")
        }));
        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("should declare `policy` explicitly")
        }));
    }

    #[test]
    fn canonical_requires_field_policies_for_sensitive_fields() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("must declare field-level `read` and `write` policies")
        }));
    }

    #[test]
    fn canonical_requires_webhook_verify_and_idempotency() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    handler "./integrations/stripe.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("webhooks are inbound trust boundaries and must declare `verify")
        }));
        assert!(
            messages.iter().any(|message| {
                message.contains("webhooks must declare `idempotency by payload.")
            })
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    #[test]
    fn canonical_warns_for_tenant_webhook_without_tenant_from() {
        let source = r#"
feature billing
  purpose "Billing"

  defaults
    tenancy org

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    verify hmac sha256
      secret env.STRIPE_SECRET
      header "Stripe-Signature"
    idempotency by payload.org_id, payload.provider_event_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `tenant_from payload.org_id`")
        }));
    }

    #[test]
    fn strict_profile_rejects_security_opt_out_without_reason() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook inbound
    path "/webhooks/inbound"
    verify none
    idempotency by payload.id
"#;

        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);

        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`verify none` must include a `reason")
        }));
    }

    #[test]
    fn production_profile_rejects_reasoned_security_opt_out() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook inbound
    path "/webhooks/inbound"
    verify none
      reason "Internal tunnel in development only."
    idempotency by payload.id
"#;

        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);
        let production = diagnostics_for_with_profile(source, SecurityProfile::Production);

        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::WARNING)
                && diagnostic
                    .message
                    .contains("`verify none` is an explicit security opt-out")
        }));
        assert!(production.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`verify none` is an explicit security opt-out")
        }));
    }

    #[test]
    fn canonical_requires_escape_route_security_envelope() {
        let source = r#"
feature customer
  purpose "Customers"

  escape_route "/admin/raw"
    at "./pages/raw.tsx"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`escape_route` is outside generated UI ownership")
        }));
    }

    #[test]
    fn canonical_requires_auth_password_and_session_contracts() {
        let source = r#"
feature auth
  purpose "Auth"

  auth
    password
      hash @fn.hash_password

    sessions
      resource Session
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("`auth password` must declare `algorithm"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("credential guessing protection"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("`auth sessions` must declare `ttl`"))
        );
    }

    #[test]
    fn canonical_warns_for_incomplete_crypto_contracts() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      legacy_secret: @cap.Secret required
      refresh_token_hash: @cap.Hashed required
      api_key: @cap.Encrypted required
      reset_token: @cap.Token(ttl:1h,single_use:true) required
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`@cap.Secret` is legacy") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Hashed` should declare `algorithm:<name>`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Encrypted` should declare `key:@key.<scope>`")
        }));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`@cap.Token` should declare `store:hashed`") })
        );
    }

    #[test]
    fn canonical_warns_for_invalid_crypto_capability_arguments() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:md5,pepper:true) required
      api_key: @cap.Encrypted(key:tenant) required
      private_note: @cap.E2ee(key:tenant) optional
      reset_token: @cap.Token(ttl:"one hour",single_use:yes,store:plain) required
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("canonical v0 hash algorithms are `argon2id` or `bcrypt`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("@cap.Hashed only accepts canonical arguments: algorithm")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("encryption capability keys should use `key:@key.<scope>`")
        }));
        assert!(
            messages.iter().any(|message| {
                message.contains("`@cap.Token` ttl should use `ttl:<duration>`")
            })
        );
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Token` single_use should be `true` or `false`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Token` store should be `hashed` in canonical v0")
        }));
    }

    #[test]
    fn canonical_warns_for_authored_summary() {
        let source = r#"
feature customer
  purpose "Customers"

  summary
    resources: Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`summary` is generated by `lazuli inspect --expand=summary`")
        }));
    }

    #[test]
    fn canonical_warns_for_env_reference_without_schema() {
        let source = r#"
feature integration
  purpose "Integration"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("environment reference `env.INBOUND_WEBHOOK_SECRET`")
        }));
    }

    #[test]
    fn canonical_accepts_declared_env_reference() {
        let source = r#"
env
  group webhooks
    server INBOUND_WEBHOOK_SECRET: Secret required in production
  group public_clients
    client PUBLIC_APP_URL: Url required
    mobile EXPO_PUBLIC_API_URL: Url required

feature integration
  purpose "Integration"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("environment reference `env.INBOUND_WEBHOOK_SECRET`")
        }));
    }

    #[test]
    fn canonical_warns_for_incomplete_api_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  api stream_summary
    method POST
    path "/api/customers/:id/summary"
    output stream Text
    policy @policy.read
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| message.contains("handler")));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("api path parameter `id` should be declared") })
        );
    }

    #[test]
    fn canonical_warns_for_incomplete_cache_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

    query.list list
      cache
        key customer.list(params)
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("query cache contracts should declare ttl")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_error_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  errors
    default leak
    expose client 4xx stack

  command archive
    policy @policy.update
    rate_limit "30 per minute per user"
    error CustomerGone status 900 expose stack
    deletes Customer
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("feature error defaults use `default hide`") })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("error exposure uses `expose client") })
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("HTTP status code from 100 to 599"))
        );
    }

    #[test]
    fn lzx_accepts_experience_and_platform_surface_layers() {
        let experience = r#"
experience customer
  imports customer

  view list
    source customer.query.list
    action create -> customer.command.create
"#;

        let surface = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      columns name, email, tier
"#;

        assert!(diagnostics_for(experience).is_empty());
        assert!(diagnostics_for(surface).is_empty());
    }

    #[test]
    fn lzx_warns_for_untyped_top_level_route_params() {
        let source = r#"
route customer_detail
  path "/customers/:id"
  to customer.view.detail(id: path.id)
  surface customer web
  audience admin
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("route path parameter `id` should be declared")
        }));
    }

    #[test]
    fn lzx_accepts_typed_top_level_routes() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  targets
    backend go
    web react
  uses customer

route customer_detail
  path "/customers/:id"
  params id: Customer.ID
  to customer.view.detail(id: path.id)
  surface customer web
  audience admin
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn lzx_warns_for_legacy_stack_top_level_routes() {
        let source = r#"
route customer_detail
  stack "customers/[id]"
  params id: Customer.ID
  to customer.view.detail(id: path.id)
  surface customer mobile
  audience sales
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`stack` is legacy route syntax")
        }));
    }

    #[test]
    fn lzx_rejects_cascade_and_unscoped_platform_views() {
        let source = r#"
surface web
  view list Table
    columns += score
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("put the experience name before the platform") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("concrete `.lzx` surfaces must declare `uses experience <name>`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("concrete platform views live under `audience ...` blocks")
        }));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`.lzx` forbids partial overrides") })
        );
    }

    #[test]
    fn lzx_warns_for_implicit_navigation_and_submit_targets() {
        let source = r#"
experience customer
  imports customer

  view list
    source customer.query.list
    opens detail

surface customer web
  uses experience customer

  audience public
    view capture Form
      fields name, email
      submit create
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("view navigation should bind route arguments explicitly")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("platform form submits should use an explicit command reference")
        }));
    }

    #[test]
    fn lzx_warns_for_route_references_without_view_route_contract() {
        let source = r#"
experience customer
  imports customer

  view detail
    source customer.query.by_id(id: route.id)
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("does not declare `route id: ...`")
        }));
    }

    #[test]
    fn lzx_warns_for_routed_actions_without_route_arguments() {
        let source = r#"
experience customer
  imports customer

  view detail
    route id: Customer.ID
    source customer.query.by_id(id: route.id)
    action archive -> customer.workflow.lifecycle.archive
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("actions in routed views should pass route arguments explicitly")
        }));
    }

    #[test]
    fn lzx_warns_for_web_primitives_in_mobile_projection() {
        let source = r#"
surface customer mobile
  uses experience customer

  audience sales
    view list Table
      columns name

    view detail SidePanel
      sections header
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.message.contains("mobile-native primitives"))
                .count(),
            2
        );
    }

    #[test]
    fn lzx_warns_for_legacy_extension_blocks_without_slot() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    block @client.tag_editor
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("view extensions should place blocks under an explicit slot")
        }));
    }

    #[test]
    fn lzx_filename_suffix_must_match_surface_header() {
        let source = r#"
surface customer mobile
  uses experience customer

  audience sales
    view list CardList
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.web.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`customer.web.lzx` is a `web` projection")
        }));
    }

    #[test]
    fn lzx_platform_suffix_must_be_terminal() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.web.admin.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("abstract `.lzx` files declare `experience <name>`")
        }));
    }

    #[test]
    fn lzx_abstract_file_cannot_declare_concrete_surface() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("abstract `.lzx` files declare `experience <name>`")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_non_goals_shape() {
        let source = r#"
feature customer
  purpose "Customers"

  non_goals
    user: "staff authentication"
    anti_pattern.generic_etl: "generic ETL"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should group entries under `delegated_to` or `out_of_scope`")
        }));
    }

    #[test]
    fn canonical_warns_for_unscoped_defaults_policy() {
        let source = r#"
feature outreach
  purpose "Outreach"

  defaults
    policy @actor.system
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("feature-level policy defaults should use `policy_for")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_validation_syntax() {
        let source = r#"
feature import
  purpose "Import"

  domain
    resource ImportRow
      raw: JSON required
      validate "./domain/validate_row.go"

    resource Customer
      tier: Text required
      validates tier "./hooks/validate_tier.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("whole-resource validators should use `validates resource")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("field validators should use `validates field <name>")
        }));
    }

    #[test]
    fn canonical_warns_for_self_in_command_target_context() {
        let source = r#"
feature customer_auth
  purpose "Auth"

  command enable_mfa
    route customer_id: ID
    target customer.query.by_id(id: route.customer_id)
    policy @actor.system
    creates CustomerMfaConfig
      customer = self
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands and declarative jobs should use `target`")
        }));
    }

    #[test]
    fn canonical_warns_when_required_field_is_checked_against_nil() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      owner: User required
      tier: CustomerTier = enterprise

    rule "enterprise customers require owner"
      deny Customer.activate when self.tier = CustomerTier.enterprise AND self.owner = nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`Customer.owner` is declared `required`")
        }));
    }

    #[test]
    fn canonical_warns_when_trace_event_is_used_as_trigger() {
        let source = r#"
feature customer_import
  purpose "Import"

  domain
    event.trace customer_webhook_received
      external_id: Text

  job react_to_trace
    trigger event customer_webhook_received
    handler "./jobs/react.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`event.trace` declarations are outside the reaction graph")
        }));
    }

    #[test]
    fn canonical_warns_for_event_consumer_payload_not_declared_by_producer() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

    event customer_created
      email: @semantic.Email

feature audit
  purpose "Audit"

  uses customer

  domain
    resource AuditEvent
      subject_id: ID required

  job record_customer_created
    trigger event customer.customer_created
    idempotency by envelope.id
    creates AuditEvent
      subject_id = payload.account_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "`payload.account_id` is not declared by event `customer.customer_created`",
            )
        }));
    }

    #[test]
    fn canonical_event_group_can_own_short_event_declarations() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

      event created
        email: @semantic.Email

feature audit
  purpose "Audit"

  uses customer

  domain
    resource AuditEvent
      subject_id: ID required

  job record_customer_created
    trigger event customer.customer_created
    tenant_from payload.org_id
    idempotency by envelope.id
    creates AuditEvent
      subject_id = payload.customer_id
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_warns_for_unknown_sql_return_type() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    query.sql lifetime_value
      returns CustomerLtv[]
      sql "./queries/customer_lifetime_value.sql"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("return type `CustomerLtv` should resolve")
        }));
    }

    #[test]
    fn canonical_accepts_sql_return_record_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.sql lifetime_value
      returns CustomerLtv[]
      sql "./queries/customer_lifetime_value.sql"
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_command_warns_for_undeclared_route_reference() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    update: @role.admin

  command rename
    input name
    target query.by_id(id: route.id)
    policy @policy.update
    rate_limit "30 per minute per user"
    updates Customer
      name = input.name
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("references `route.id` but does not declare `route id: ...`")
        );
    }

    #[test]
    fn canonical_command_accepts_short_input_when_fields_exist() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email required

  policies
    create: @role.admin

  command create
    input name, email
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name
      email = input.email
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_command_warns_for_short_input_not_on_resource() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    create: @role.admin

  command create
    input display_name
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.display_name
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("uses short input `display_name`")
        );
    }

    #[test]
    fn canonical_command_warns_for_short_input_without_inference_resource() {
        let source = r#"
feature user
  purpose "User auth"

  policies
    login: @scope.public

  command login
    input email, password
    policy @policy.login
    rate_limit "5 per 10 minutes per ip"
    returns AuthSession
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic
                .message
                .contains("short inputs require a local `creates` or `updates` resource")
        }));
    }

    #[test]
    fn canonical_command_warns_for_short_input_on_delete_only_command() {
        let source = r#"
feature customer_tags
  purpose "Customer tags"

  domain
    resource CustomerTagAssignment
      customer: Customer required
      tag: CustomerTag required

    query.lookup assignment_by_customer_tag

  policies
    update: @role.admin

  command remove_tag
    input customer_id, tag_id
    target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
    policy @policy.update
    rate_limit "60 per minute per user"
    deletes CustomerTagAssignment
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic
                .message
                .contains("short inputs require a local `creates` or `updates` resource")
        }));
    }

    #[test]
    fn canonical_command_warns_for_short_input_with_multiple_inference_resources() {
        let source = r#"
feature inventory
  purpose "Inventory transfers"

  domain
    resource SourceStock
      amount: Integer required

    resource TargetStock
      amount: Integer required

  policies
    update: @role.admin

  command transfer
    route id: ID
    input amount
    policy @policy.update
    rate_limit "60 per minute per user"
    updates SourceStock
      amount = input.amount
    updates TargetStock
      amount = input.amount
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("short inputs require exactly one local `creates` or `updates` resource")
        );
    }

    #[test]
    fn canonical_command_accepts_typed_input_not_on_resource() {
        let source = r#"
feature customer_tags
  purpose "Customer tags"

  domain
    resource CustomerTagAssignment
      customer: Customer required
      tag: CustomerTag required

    query.lookup assignment_by_customer_tag

  policies
    update: @role.admin

  command remove_tag
    input
      customer_id: ID
      tag_id: ID
    target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
    policy @policy.update
    rate_limit "60 per minute per user"
    deletes CustomerTagAssignment
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_warns_when_validator_result_does_not_block_command() {
        let source = r#"
feature customer_auth
  purpose "Customer auth"

  domain
    resource CustomerMfaConfig

  policies
    update: @role.admin

  command enable_mfa
    input
      totp_code: Text required
    let totp_verified = @validator.verify_customer_totp(code: input.totp_code)
    policy @policy.update
    rate_limit "10 per minute per user"
    creates CustomerMfaConfig

  extensions
    validator verify_customer_totp: Validator[TotpVerifyInput]
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("is computed but not required") })
        );
    }

    #[test]
    fn canonical_warns_for_previously_without_mode() {
        let legacy = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer previously Account
"#;
        let canonical = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer previously migrated Account
"#;

        assert!(diagnostics_for(legacy).iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`previously` should declare `migrated` or `alias`")
        }));
        assert!(!diagnostics_for(canonical).iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`previously` should declare `migrated` or `alias`")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_ergonomic_syntax() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: Email required

    query list

  policies
    create: role_admin

  command create
    input name, email
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name
      email = input.email

  job sync
    trigger event customer.customer_created
    idempotency event.id
    policy @actor.system
    handler "./jobs/sync.go"

  surface web admin
    view list Table
      source query.list

      cells
        email ext.email_cell
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages.iter().any(|message| {
                message.contains("query declarations should use an explicit mode")
            })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("policy atoms should be namespaced") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("semantic types should use the `@semantic.*` namespace")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("extension references should use capability namespaces")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`idempotency` should declare its source with `by`")
        }));
    }

    #[test]
    fn canonical_formatter_preserves_full_capsule_fixture() {
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzi");
        let formatted = format_canonical_source(source).expect("canonical source");

        assert_eq!(formatted, source);
    }

    #[test]
    fn canonical_warns_for_authored_command_policy_matrix_tests() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    update: @role.admin

  command rename
    input
      name: Text
    policy @policy.update
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name

    tests
      permits @role.admin
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("policy actor-matrix tests are generated")
        );
    }

    #[test]
    fn canonical_warns_for_explicit_default_list_order() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

    query.list list
      order created_at desc
      paginate 50
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("defaults to `order created_at desc`")
        );
    }

    #[test]
    fn canonical_warns_for_explicit_generated_filter_index() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      status: CustomerStatus = lead

    constraints
      index org, status

    query.list list
      params
        status: CustomerStatus optional

      filters
        status when params.status
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("filters generate this tenant-aware index")
        );
    }

    #[test]
    fn canonical_warns_for_search_encoded_as_filter_equality() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

    query.list list
      params
        search: Text optional

      filters
        name = params.search when params.search
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("text matching should use `search params.search over ...`")
        );
    }

    #[test]
    fn canonical_warns_for_invalid_pagination_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

    query.lookup by_id by id: ID
      paginate 0
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`paginate` is a `query.list` contract") })
        );
        assert!(
            messages.iter().any(|message| {
                message.contains("`paginate` should declare a positive integer")
            })
        );
    }

    #[test]
    fn canonical_warns_for_file_capability_without_contract() {
        let source = r#"
feature import_csv
  purpose "Import CSV"

  domain
    resource ImportBatch
      file: @cap.File required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`@cap.File` should declare `max_size:<size>` and `accept:<mime>`")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_file_capability_size() {
        let source = r#"
feature import_csv
  purpose "Import CSV"

  domain
    resource ImportBatch
      file: @cap.File(max_size:large,accept:text/csv) required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`@cap.File` max_size should use a positive size literal")
        }));
    }

    #[test]
    fn canonical_warns_for_pii_resource_without_retention() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      email: @semantic.Email @pii.contact required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("stores `@pii.*` fields and should declare `retention")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_retention_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      email: @semantic.Email @pii.contact required
      retention seven-years then purge
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("retention duration should be `forever`")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_write_window_contract() {
        let source = r#"
feature billing
  purpose "Billing"

  command create
    write_window input.issued_at billing.open_period
    policy @role.admin
    rate_limit "30 per minute per user"
    creates Invoice
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("write-window guards use `write_window by")
        }));
    }

    #[test]
    fn canonical_warns_for_active_sessions_without_temporal_scope() {
        let source = r#"
feature user_auth
  purpose "User auth"

  domain
    query.list active_sessions
      params
        user_id: ID

      filters
        user.id = params.user_id
        expires_at != nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("can include expired sessions") })
        );
    }

    #[test]
    fn canonical_warns_when_active_session_modifier_has_no_temporal_contract() {
        let source = r#"
feature user_auth
  purpose "User auth"

  domain
    query.list active_sessions
      modifier @query_modifier.active_session_scope

      params
        user_id: ID

      filters
        user.id = params.user_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare temporal validity")
        }));
    }

    #[test]
    fn canonical_warns_for_tenant_scheduled_job_without_fanout() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  job recompute_scores
    trigger schedule "0 2 * * *"
    handler "./jobs/recompute_scores.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `fanout tenants org`")
        }));
    }

    #[test]
    fn canonical_formatter_removes_blank_before_transition_children() {
        let source = r#"
feature customer
  purpose "Customers"

  workflow lifecycle on Customer.status
    policy @policy.update

    resume: paused -> active

      tests
        allows from paused
"#;

        let formatted = format_canonical_source(source).expect("canonical source");

        assert!(
            formatted.contains("    resume: paused -> active\n      tests"),
            "transition children should stay contiguous with the header:\n{formatted}"
        );
    }

    #[test]
    fn canonical_order_reports_late_uses() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  uses org
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("`uses` appears after `domain`")
        );
    }

    #[test]
    fn canonical_order_reports_late_webhook_after_surface() {
        let source = r#"
env
  server STRIPE_WEBHOOK_SECRET: Secret required

feature billing
  purpose "Billing"

  domain
    resource Invoice

  surface web admin
    view list Table

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    verify hmac sha256
      secret env.STRIPE_WEBHOOK_SECRET
      header "Stripe-Signature"
    idempotency by payload.provider_event_id
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("`webhook` appears after `surface`")
        );
    }

    #[test]
    fn canonical_formatter_reorders_feature_blocks() {
        let source = r#"
env
  server INBOUND_WEBHOOK_SECRET: Secret required

feature customer
  purpose "Customers"

  surface web admin
    view list Table

  uses org

  domain
    resource Customer

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let formatted = format_canonical_source(source).expect("canonical source");

        assert!(
            formatted.find("  uses org").unwrap() < formatted.find("  domain").unwrap(),
            "uses should move before domain:\n{formatted}"
        );
        assert!(
            formatted.find("  webhook inbound").unwrap()
                < formatted.find("  surface web admin").unwrap(),
            "webhook should move before surface:\n{formatted}"
        );
        assert!(
            diagnostics_for(&formatted).is_empty(),
            "formatter should produce canonical order"
        );
    }

    #[test]
    fn legacy_aggregate_still_uses_parser_diagnostics() {
        let source = r#"
aggregate Customer {
  name: Text

  command Create {
    input email
  }
}
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].source.as_deref(), Some("lazuli-analyzer"));
    }
}
