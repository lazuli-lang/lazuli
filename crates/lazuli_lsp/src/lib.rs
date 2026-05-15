use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lazuli_syntax::{Span, parse_document};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, Documentation, Hover, HoverContents, HoverParams, InitializeParams,
    InitializeResult, InitializedParams, MarkupContent, MarkupKind, MessageType, OneOf, Position,
    Range, ServerCapabilities, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

mod lzx_completion;

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
        // Rich Markdown hover for the LSP-extended kinds
        // (`command`/`query.*`/`api`/`policy`/`effect`/`audit`/
        // `rate_limit`) — fall back to the brief one-line
        // description for every other keyword so unrelated tooling
        // stays unaffected.
        let hover_markdown = if !is_design_lzi_uri(&uri) {
            if let Some(markdown) = rich_keyword_hover(&word) {
                Some(markdown)
            } else {
                keyword_description(&word).map(|d| format!("`{word}`\n\n{d}"))
            }
        } else {
            design_keyword_description(&word)
                .or_else(|| keyword_description(&word))
                .map(|d| format!("`{word}`\n\n{d}"))
        };
        let Some(value) = hover_markdown else {
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

fn completion_items_for_uri(uri: &Url) -> Vec<CompletionItem> {
    if is_design_lzi_uri(uri) {
        return design_keyword_completion_items();
    }

    lazuli_keyword_completion_items()
}

fn lazuli_keyword_completion_items() -> Vec<CompletionItem> {
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
    items
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
        diagnostics.extend(registry_contract_diagnostics(source));
        diagnostics.extend(profile_contract_diagnostics(source));
        diagnostics.extend(workspace_contract_diagnostics(source));
        diagnostics.extend(external_contract_diagnostics(source));
        diagnostics.extend(feature_requirements_contract_diagnostics(source));
        diagnostics.extend(external_call_contract_diagnostics(source));
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
        diagnostics.extend(derived_field_diagnostics(source));
        diagnostics.extend(has_many_diagnostics(source));
        diagnostics.extend(agent_contract_diagnostics(source));
        diagnostics.extend(agent_tools_diagnostics(source));
        diagnostics.extend(agent_evals_diagnostics(source));
        diagnostics.extend(agent_discriminator_diagnostics(source));
        diagnostics.extend(agent_expose_diagnostics(source));
        diagnostics.extend(reserved_trace_event_diagnostics(source));
        diagnostics.extend(approval_contract_diagnostics(source));
        diagnostics.extend(cors_contract_diagnostics(source));
        diagnostics.extend(notification_contract_diagnostics(source));
        diagnostics.extend(emits_derived_diagnostics(source));
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
        diagnostics.extend(env_top_level_legacy_diagnostics(source));
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
        || has_canonical_registry_block(source)
        || has_canonical_profile_block(source)
        || has_canonical_workspace_block(source)
        || has_canonical_contract_block(source)
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

fn has_canonical_registry_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start() == "registry")
}

fn has_canonical_profile_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("profile "))
}

fn has_canonical_workspace_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("workspace "))
}

fn has_canonical_contract_block(source: &str) -> bool {
    source
        .lines()
        .any(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("contract "))
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
    has_path: bool,
    has_to: bool,
    has_surface: bool,
    has_audience: bool,
    declared_routes: HashSet<String>,
    path_params: Vec<String>,
    route_references: Vec<(usize, String, String)>,
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

fn lzx_app_route_diagnostics(route: LzxAppRouteFacts) -> Vec<Diagnostic> {
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

        // Only validate query declarations, not references. Declarations live
        // at indent 2 (legacy top-level) or 4 (canonical, inside `domain`)
        // inside a feature; references appear in `invalidates`, `source`,
        // `target`, `let`, etc. at deeper indents.
        let leading = leading_spaces(line);
        if leading != 2 && leading != 4 {
            continue;
        }

        if first == "query" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-mode",
                "query declarations should use an explicit mode: `query.list <name>`, `query.lookup <name>`, or `query.sql <name>`. The kind belongs in the header so cold-readers see it before the body.",
            ));
        } else if let Some(mode) = first.strip_prefix("query.") {
            // Strip parens/args used in references like `query.by_id(id: route.id)`.
            let mode = mode
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
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

        let Some((head, tail)) = trimmed.split_once(" previously ") else {
            continue;
        };

        let tail = tail.trim_start();
        if !tail.starts_with("migrated ") && !tail.starts_with("alias ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "previously-mode-contract",
                "`previously` should declare `migrated` or `alias` so migration-only history is distinct from compatibility aliases.",
            ));
            continue;
        }

        match inline_previously_kind(head, tail) {
            InlinePreviouslyKind::Field => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-field-inline",
                    "field-level `previously migrated|alias <old>` should be a child of the field, not inline before `:`. Keep `<name>: <Type> = <value>` contiguous and put `previously migrated <old>` on the next line indented one level deeper.",
                ));
            }
            InlinePreviouslyKind::Header => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-header-inline",
                    "header-level `previously migrated|alias <old>` should be a child of the block, not inline. Keep the kind + name on the header line and put `previously migrated <old>` on the next line indented one level deeper so cold-readers see one concept per line.",
                ));
            }
            InlinePreviouslyKind::Transition => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-transition-inline",
                    "workflow transitions should keep the `<name>: <state> -> <state>` shape contiguous; declare `previously migrated <old>` as a transition child on the next line.",
                ));
            }
            InlinePreviouslyKind::Other => {}
        }
    }

    diagnostics
}

#[derive(Debug, PartialEq, Eq)]
enum InlinePreviouslyKind {
    Field,
    Header,
    Transition,
    Other,
}

fn inline_previously_kind(head: &str, tail: &str) -> InlinePreviouslyKind {
    let head = head.trim();
    if head.is_empty() {
        return InlinePreviouslyKind::Other;
    }
    let first = head.split_whitespace().next().unwrap_or("");

    // Block headers (`resource <Name>`, `command <name>`, etc.) — the
    // identifier comes first, then `previously migrated <old>`. Tail has
    // *no* `:` (no field/transition shape) and the head is two tokens
    // (kind + name).
    if matches!(
        first,
        "resource"
            | "record"
            | "enum"
            | "command"
            | "workflow"
            | "job"
            | "webhook"
            | "api"
            | "view"
            | "rule"
            | "agent"
            | "feature"
            | "notification"
    ) {
        return InlinePreviouslyKind::Header;
    }

    // Transition shape: `<name>: <state> -> <state>` (with optional `previously
    // migrated <old>` between name and `:`). Detected by the `->` token in
    // tail.
    if tail.contains(" -> ") {
        return InlinePreviouslyKind::Transition;
    }

    // Field shape: a single identifier head followed by `previously migrated
    // <old>: <Type>`.
    if head.contains(' ') {
        return InlinePreviouslyKind::Other;
    }
    if tail.contains(':') {
        return InlinePreviouslyKind::Field;
    }

    InlinePreviouslyKind::Other
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
                    "unknown `@...` namespace. Allowed namespaces are `@role`, `@scope`, `@actor`, `@policy`, `@semantic`, `@cap`, `@pii`, `@key`, `@fn`, `@hook`, `@validator`, `@adapter`, `@client`, `@query_modifier`, `@anchor`, `@llm`, `@tool`, and `@trace`.",
                ));
                break;
            }
        }
    }

    diagnostics
}

fn namespace_references(line: &str) -> Vec<&str> {
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

        // RB.S6 — structured `policy <expr>` form. The first token may
        // be `authenticated`, `has_role`, `has_permission`, `not`, or
        // `(` — all valid expression heads. Skip the legacy single-atom
        // check; the parser already validated the expression shape.
        if matches!(
            policy_ref,
            "authenticated" | "has_role" | "has_permission" | "not"
        ) || policy_ref.starts_with('(')
        {
            continue;
        }

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
            in_registry = trimmed == "registry";
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

fn is_float_in_range(value: &str, min: f64, max: f64) -> bool {
    value
        .parse::<f64>()
        .map(|v| v >= min && v <= max)
        .unwrap_or(false)
}

fn derived_field_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn agent_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading != 2 || !trimmed.starts_with("agent ") {
            index += 1;
            continue;
        }

        let header_index = index;
        let mut has_policy = false;
        let mut has_output = false;
        let mut has_model = false;
        let mut has_prompt = false;
        let mut model_value: Option<&str> = None;
        let mut bad_config: Vec<(usize, String, String)> = Vec::new();

        index += 1;
        while index < lines.len() {
            let inner = lines[index];
            let inner_trimmed = inner.trim_start();
            let inner_leading = leading_spaces(inner);

            if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                index += 1;
                continue;
            }
            if inner_leading <= 2 {
                break;
            }
            if inner_leading == 4 {
                if inner_trimmed.starts_with("policy ") {
                    has_policy = true;
                } else if inner_trimmed.starts_with("output ") {
                    has_output = true;
                } else if let Some(rest) = inner_trimmed.strip_prefix("model ") {
                    has_model = true;
                    model_value = Some(rest.trim());
                } else if inner_trimmed.starts_with("prompt ") {
                    has_prompt = true;
                } else if let Some(rest) = inner_trimmed.strip_prefix("temperature ") {
                    let value = rest.trim();
                    if !is_float_in_range(value, 0.0, 2.0) {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`temperature` requires a float in [0.0, 2.0]".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("top_p ") {
                    let value = rest.trim();
                    if !is_float_in_range(value, 0.0, 1.0) {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`top_p` requires a float in [0.0, 1.0]".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("max_tokens ") {
                    let value = rest.trim();
                    let valid = value.parse::<u32>().map(|v| v >= 1).unwrap_or(false);
                    if !valid {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`max_tokens` requires a positive integer".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("seed ") {
                    let value = rest.trim();
                    if value.parse::<i64>().is_err() {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`seed` requires an integer".to_owned(),
                        ));
                    }
                }
            }
            index += 1;
        }

        if !has_policy {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare an explicit `policy @policy.<name>`.",
            ));
        }
        if !has_output {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare an `output [stream] <Type>`.",
            ));
        }
        if !has_model {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare a `model @llm.<name>`.",
            ));
        } else if let Some(value) = model_value {
            if !value.starts_with("@llm.") {
                diagnostics.push(simple_canonical_diagnostic(
                    header_index,
                    lines[header_index],
                    DiagnosticSeverity::ERROR,
                    "agent-contract",
                    "`model` on an `agent` must be a `@llm.<name>` reference.",
                ));
            }
        }
        if !has_prompt {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare a `prompt \"./path\"` template.",
            ));
        }
        for (idx, owned_line, message) in bad_config {
            diagnostics.push(simple_canonical_diagnostic(
                idx,
                &owned_line,
                DiagnosticSeverity::ERROR,
                "agent-contract",
                &message,
            ));
        }
    }

    diagnostics
}

// =============================================================================
// Cut A — file-local additions for `tools`, `evals`, and discriminator scoping.
//
// These are intentionally file-local: cross-feature resolution of tool
// targets, policy compatibility, and discriminator enum/record lookup
// lives in `crates/lazuli_cli/src/doctor.rs` (Phase 3). The LSP is the
// fast inner loop; doctor is the workspace pass.
//
// See docs/proposals/ai-primitives-v0-implementation.md §6.
// =============================================================================

/// Iterate every `agent <name>` block in the source, yielding the
/// header line index and the body slice (one-based inclusive on the
/// header, exclusive on the next sibling). The caller decides which
/// children to inspect. Shared helper for the three Cut A LSP checks.
fn iter_agent_blocks(source: &str) -> Vec<(usize, Vec<usize>)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);
        if leading == 2 && trimmed.starts_with("agent ") {
            let header = index;
            let mut body = Vec::new();
            index += 1;
            while index < lines.len() {
                let inner = lines[index];
                let inner_trimmed = inner.trim_start();
                if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                    body.push(index);
                    index += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                body.push(index);
                index += 1;
            }
            blocks.push((header, body));
            continue;
        }
        index += 1;
    }
    blocks
}

/// Reject tool entries whose *shape* is invalid. Cross-feature
/// reachability is doctor's job — this layer only catches malformed
/// shorthand (e.g. `query.list` with no name; `customer..by_id`).
fn agent_tools_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (_, body) in iter_agent_blocks(source) {
        let mut in_tools = false;
        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);
            if leading == 4 {
                in_tools = trimmed == "tools";
                continue;
            }
            if !in_tools {
                continue;
            }
            if leading != 6 {
                continue;
            }
            if let Some(message) = validate_tool_reference_shape(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    raw,
                    DiagnosticSeverity::ERROR,
                    "agent_tools_diagnostics",
                    &message,
                ));
            }
        }
    }

    diagnostics
}

/// Validate one tool-entry source token. The closed shapes:
///   - `@tool.<seg>(.<seg>)*` — adapter tool
///   - `query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>`
///   - `query.<name>` (unspecified subkind — doctor narrows)
///   - `command.<name>` / `api.<name>`
///   - `<feature>.<above>` cross-feature prefix
fn validate_tool_reference_shape(text: &str) -> Option<String> {
    if text.split_whitespace().count() != 1 {
        return Some("each tool entry is a single qualified reference (one per line)".to_owned());
    }
    let token = text.trim();
    if token.contains("..") {
        return Some(format!("tool reference `{token}` has an empty segment"));
    }

    if let Some(rest) = token.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Some("`@tool.` requires a name (e.g. `@tool.web_search`)".to_owned());
        }
        if rest.split('.').any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "`@tool.<...>` segments must be lower_snake idents; got `{token}`"
            ));
        }
        return None;
    }

    let segments: Vec<&str> = token.split('.').collect();
    let valid_local = matches!(
        segments.as_slice(),
        ["query", "list", _name]
            | ["query", "lookup", _name]
            | ["query", "sql", _name]
            | ["query", _name]
            | ["command", _name]
            | ["api", _name]
    );
    if valid_local {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    let valid_cross = matches!(
        segments.as_slice(),
        [_feature, "query", "list", _name]
            | [_feature, "query", "lookup", _name]
            | [_feature, "query", "sql", _name]
            | [_feature, "query", _name]
            | [_feature, "command", _name]
            | [_feature, "api", _name]
    );
    if valid_cross {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    Some(format!(
        "tool reference `{token}` is not a recognised shape; expected `<feature>.<kind>.<name>`, `<kind>.<name>`, or `@tool.<dotted>` where kind is `query[.list|.lookup|.sql]`, `command`, or `api`"
    ))
}

/// Reject eval cases whose *predicate language* or *vocabulary* is
/// malformed. Cases without `temperature 0` + `seed <int>` also surface
/// a warning here so the inner loop catches non-determinism without
/// waiting on `lazuli doctor`.
fn agent_evals_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (header, body) in iter_agent_blocks(source) {
        let mut in_evals = false;
        let mut has_evals_block = false;
        let mut temperature_zero = false;
        let mut seed_present = false;

        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);
            if leading == 4 {
                if let Some(rest) = trimmed.strip_prefix("temperature ") {
                    temperature_zero = rest.trim().parse::<f64>().ok() == Some(0.0);
                } else if trimmed.starts_with("seed ") {
                    seed_present = true;
                }
                in_evals = trimmed == "evals";
                if in_evals {
                    has_evals_block = true;
                }
                continue;
            }
            if !in_evals {
                continue;
            }
            if leading == 6 {
                if trimmed.starts_with("given ") || trimmed == "given" {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`given` is legacy vocabulary; eval blocks use `case <name>` then `requires`/`forbids` clauses.",
                    ));
                } else if !trimmed.starts_with("case ") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval children must be `case <name>` blocks at six-space indentation.",
                    ));
                } else if trimmed
                    .strip_prefix("case ")
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`case` requires a name (e.g. `case redacts_email`).",
                    ));
                }
            }
            if leading == 8 {
                if trimmed.starts_with("expect ") || trimmed == "expect" {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`expect` is legacy vocabulary; eval assertions are `requires <predicate>` or `forbids <predicate>`.",
                    ));
                    continue;
                }
                // Cut A.10: `golden "./path.jsonl" [min_score N]` is a
                // valid case child alongside requires/forbids.
                if trimmed.starts_with("golden ") {
                    let rest = trimmed.strip_prefix("golden ").unwrap_or("").trim();
                    if !rest.starts_with('"') {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            raw,
                            DiagnosticSeverity::ERROR,
                            "agent_evals_diagnostics",
                            "`golden` requires a quoted file path: `golden \"./path.jsonl\"`.",
                        ));
                    }
                    continue;
                }
                let predicate = trimmed
                    .strip_prefix("requires ")
                    .or_else(|| trimmed.strip_prefix("forbids "));
                let Some(predicate) = predicate else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval children are `requires <predicate>`, `forbids <predicate>`, or `golden \"./path\"`.",
                    ));
                    continue;
                };
                if predicate.trim().is_empty() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval assertion is missing its predicate body.",
                    ));
                    continue;
                }
                if let Some(message) = validate_eval_predicate_shape(predicate) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        &message,
                    ));
                }
            }
        }

        if has_evals_block && (!temperature_zero || !seed_present) {
            let reason = if !temperature_zero {
                "missing `temperature 0`"
            } else {
                "missing `seed <int>`"
            };
            diagnostics.push(simple_canonical_diagnostic(
                header,
                lines[header],
                DiagnosticSeverity::WARNING,
                "eval_nondeterministic_warning",
                &format!(
                    "agent declares `evals` but is non-deterministic ({reason}); cases run as informational results until both `temperature 0` and `seed <int>` are pinned."
                ),
            ));
        }
    }

    diagnostics
}

/// File-local predicate-shape check. The full closed-predicate AST
/// lives in `lazuli_analyzer`; this layer only catches obviously
/// malformed bodies (missing rhs after `contains`, unknown ordered
/// operators, dangling `tools.calls`). Anything that looks like a
/// `<path> <op> <value>` shape passes through — doctor and analyzer
/// own the deeper validation.
fn validate_eval_predicate_shape(body: &str) -> Option<String> {
    let body = body.trim();
    if let Some(rest) = body.strip_prefix("tools.calls ") {
        let mut parts = rest.split_whitespace();
        let op = parts.next();
        let target = parts.next();
        if !matches!(op, Some("includes" | "excludes")) {
            return Some(
                "`tools.calls` operator must be `includes` or `excludes` followed by a tool reference"
                    .to_owned(),
            );
        }
        if target.is_none() {
            return Some("`tools.calls <op>` requires a tool reference target".to_owned());
        }
        if parts.next().is_some() {
            return Some("`tools.calls <op> <ref>` accepts a single tool reference".to_owned());
        }
        return None;
    }

    if let Some(idx) = body.find(" contains ") {
        let lhs = body[..idx].trim();
        let rhs = body[idx + " contains ".len()..].trim();
        if lhs.is_empty() {
            return Some("`contains` predicate requires a left-hand reference".to_owned());
        }
        if rhs.is_empty() {
            return Some("`contains` predicate requires a right-hand value".to_owned());
        }
        if !(rhs.starts_with('"') || rhs.starts_with("@semantic.")) {
            return Some(
                "`contains` rhs must be a quoted string literal or a `@semantic.<Type>` reference"
                    .to_owned(),
            );
        }
        return None;
    }

    None
}

/// Reject the `discriminator` field marker when it appears outside a
/// `record <Name>` block. Per proposal §A2 the marker is record-only;
/// authors who attach it to other constructs (agent input, command
/// input, query params) get a fast LSP error.
fn agent_discriminator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut record_starts: Vec<(usize, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("record ") {
            record_starts.push((index, leading_spaces(line)));
        }
    }

    // Build the half-open ranges that each record occupies. A record
    // ends at the next line whose indent is <= the record's own.
    let mut record_ranges: Vec<(usize, usize)> = Vec::new();
    for (start, record_indent) in record_starts {
        let mut end = lines.len();
        for (offset, line) in lines.iter().enumerate().skip(start + 1) {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if leading_spaces(line) <= record_indent {
                end = offset;
                break;
            }
        }
        record_ranges.push((start, end));
    }

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // `output discriminator <Enum>` is the agent-side form; not a
        // misuse, skip.
        if trimmed.starts_with("output discriminator ") {
            continue;
        }
        // Look for `discriminator` as a tail modifier on a field-like
        // line: `<name>: <type> ... discriminator`.
        if !contains_token(trimmed, "discriminator") {
            continue;
        }
        if !trimmed.contains(':') {
            continue;
        }
        let in_record = record_ranges
            .iter()
            .any(|(start, end)| index > *start && index < *end);
        if !in_record {
            diagnostics.push(simple_canonical_diagnostic(
                index,
                line,
                DiagnosticSeverity::ERROR,
                "agent_discriminator_diagnostics",
                "`discriminator` is a field marker that only applies inside a `record <Name>` block; it cannot appear elsewhere.",
            ));
        }
    }

    diagnostics
}

/// Stand-alone `discriminator` token (not a substring of a longer
/// identifier). Used to avoid false positives on names like
/// `discriminators_list`.
fn contains_token(line: &str, token: &str) -> bool {
    line.split(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
        .any(|word| word == token)
}

/// Cut A.7 — file-local checks on `expose http` blocks. Cross-feature
/// path collisions live in doctor; this layer handles same-file path
/// duplicates, missing path slots, slot-shape misuse, and the
/// GET-streaming warning.
fn agent_expose_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Pass 1: collect every (method, path) declared by agents + apis
    // in this file. Used for the same-file collision check.
    let mut local_paths: Vec<LocalExpose> = Vec::new();

    for (header, body) in iter_agent_blocks(source) {
        let agent_name = lines[header]
            .trim_start()
            .strip_prefix("agent ")
            .map(|n| n.trim().to_owned())
            .unwrap_or_default();
        let mut output_streaming = false;
        let mut input_slot_names: Vec<String> = Vec::new();
        let mut in_input = false;
        let mut in_expose = false;
        let mut expose_header_line: Option<usize> = None;
        let mut expose_method: Option<String> = None;
        let mut expose_path: Option<(usize, String)> = None;
        let mut expose_route_slots: Vec<String> = Vec::new();

        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);

            if leading == 4 {
                in_input = trimmed == "input";
                in_expose = trimmed == "expose http";
                if in_expose {
                    expose_header_line = Some(line_index);
                }
                if let Some(rest) = trimmed.strip_prefix("output ") {
                    let body = rest.trim();
                    if body.starts_with("stream") {
                        output_streaming = true;
                    }
                }
                continue;
            }

            if in_input && leading == 6 {
                if let Some((name_part, _)) = trimmed.split_once(':') {
                    let name = name_part.trim().to_owned();
                    if !name.is_empty() {
                        input_slot_names.push(name);
                    }
                }
            }

            if in_expose && leading == 6 {
                if let Some(rest) = trimmed.strip_prefix("method ") {
                    expose_method = Some(rest.trim().to_ascii_uppercase());
                } else if let Some(rest) = trimmed.strip_prefix("path ") {
                    let unquoted = rest
                        .trim()
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(rest.trim());
                    expose_path = Some((line_index, unquoted.to_owned()));
                } else if let Some(rest) = trimmed.strip_prefix("route ") {
                    if let Some((name_part, _)) = rest.split_once(':') {
                        expose_route_slots.push(name_part.trim().to_owned());
                    }
                }
            }
        }

        let Some(expose_line) = expose_header_line else {
            continue;
        };
        let (path_line, path_str) = match expose_path {
            Some(p) => p,
            None => continue,
        };

        // Slot-unbound check: every `:slot` in the path must have a
        // matching `route` declaration inside expose http.
        let path_slots = extract_path_slots(&path_str);
        for slot in &path_slots {
            if !expose_route_slots.iter().any(|r| r == slot) {
                diagnostics.push(simple_canonical_diagnostic(
                    path_line,
                    lines[path_line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_slot_unbound_diagnostics",
                    &format!(
                        "agent `{agent_name}` declares path slot `:{slot}` but the `expose http` block has no matching `route {slot}: <Type>` declaration."
                    ),
                ));
            }
        }

        // Slot-must-use-route check: if a path slot's name collides
        // with an `input` slot name and no `route` declaration covers
        // it, the author meant `route` instead of `input`.
        for slot in &path_slots {
            let in_input = input_slot_names.iter().any(|n| n == slot);
            let in_route = expose_route_slots.iter().any(|r| r == slot);
            if in_input && !in_route {
                diagnostics.push(simple_canonical_diagnostic(
                    path_line,
                    lines[path_line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_slot_must_use_route_diagnostics",
                    &format!(
                        "agent `{agent_name}` path slot `:{slot}` is declared as `input` — use `route {slot}: <Type>` inside `expose http` for URL slots."
                    ),
                ));
            }
        }

        // Method/streaming mismatch: GET + output stream warns.
        if expose_method.as_deref() == Some("GET") && output_streaming {
            diagnostics.push(simple_canonical_diagnostic(
                expose_line,
                lines[expose_line],
                DiagnosticSeverity::WARNING,
                "agent_expose_method_streaming_mismatch_warning",
                &format!(
                    "agent `{agent_name}` exposes `method GET` but `output stream`; streaming responses typically use POST so clients can send body context."
                ),
            ));
        }

        if let Some(method) = expose_method {
            local_paths.push(LocalExpose {
                line: expose_line,
                method,
                path_normalised: lsp_normalise_path(&path_str),
                origin: format!("agent {agent_name}"),
            });
        }
    }

    // Walk `api <name>` blocks for file-local collision check.
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            let name = trimmed
                .strip_prefix("api ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let api_line = i;
            let mut method: Option<String> = None;
            let mut path: Option<String> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if leading_spaces(inner) == 4 {
                    if let Some(rest) = inner_trim.strip_prefix("method ") {
                        method = Some(rest.trim().to_ascii_uppercase());
                    } else if let Some(rest) = inner_trim.strip_prefix("path ") {
                        let unquoted = rest
                            .trim()
                            .strip_prefix('"')
                            .and_then(|s| s.strip_suffix('"'))
                            .unwrap_or(rest.trim());
                        path = Some(unquoted.to_owned());
                    }
                }
                j += 1;
            }
            if let (Some(method), Some(path)) = (method, path) {
                local_paths.push(LocalExpose {
                    line: api_line,
                    method,
                    path_normalised: lsp_normalise_path(&path),
                    origin: format!("api {name}"),
                });
            }
            i = j;
            continue;
        }
        i += 1;
    }

    // Local collision: any two LocalExpose entries with same
    // (method, normalised_path) but different `origin` collide
    // *within the same file*.
    for (idx_a, a) in local_paths.iter().enumerate() {
        for b in local_paths.iter().skip(idx_a + 1) {
            if a.method == b.method
                && a.path_normalised == b.path_normalised
                && a.origin != b.origin
            {
                diagnostics.push(simple_canonical_diagnostic(
                    a.line,
                    lines[a.line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_path_conflict_local_diagnostics",
                    &format!(
                        "{} declares an HTTP route that collides with {} (same method + normalised path) inside this file.",
                        a.origin, b.origin,
                    ),
                ));
            }
        }
    }

    diagnostics
}

#[derive(Debug, Clone)]
struct LocalExpose {
    line: usize,
    method: String,
    path_normalised: String,
    origin: String,
}

fn extract_path_slots(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| segment.strip_prefix(':').map(str::to_owned))
        .collect()
}

fn lsp_normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for (i, segment) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        if segment.starts_with(':') {
            out.push_str(":_");
        } else {
            out.push_str(segment);
        }
    }
    out
}

/// Cut A.11 — file-local shape checks on the `cors` block in
/// `app.lzi`. Validates `allow_origins` has an env + at least one
/// quoted origin, and `allow_credentials` is `true`/`false`. The
/// cross-feature checks (origin documented in `urls`, environment
/// declared, credentials/wildcard conflict) live in doctor.
fn cors_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut in_cors = false;
    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if leading == 2 {
            in_cors = trimmed == "cors";
            continue;
        }
        if !in_cors || leading != 4 {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("allow_origins ") {
            // `<env> "<origin>"[, "<origin>"]+`
            let (env, body) = match rest.split_once(char::is_whitespace) {
                Some(pair) => pair,
                None => {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "cors_contract_diagnostics",
                        "`cors allow_origins` needs an environment and at least one quoted origin: `allow_origins <env> \"<origin>\"`.",
                    ));
                    continue;
                }
            };
            if env.trim().is_empty() {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors allow_origins` is missing the environment name.",
                ));
                continue;
            }
            let mut saw_origin = false;
            for raw in body.split(',') {
                let token = raw.trim();
                if token.is_empty() {
                    continue;
                }
                saw_origin = true;
                let is_wildcard = token == "\"*\"" || token == "*";
                let quoted = token.starts_with('"') && token.ends_with('"') && token.len() >= 2;
                if !quoted && !is_wildcard {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "cors_contract_diagnostics",
                        &format!("`cors allow_origins` origin {token} must be a quoted string."),
                    ));
                }
            }
            if !saw_origin {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors allow_origins <env>` needs at least one origin after the environment.",
                ));
            }
        } else if let Some(rest) = trimmed.strip_prefix("allow_credentials ") {
            let value = rest.trim();
            if !matches!(value, "true" | "false") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    &format!(
                        "`cors allow_credentials {value}` is invalid — closed catalog is `true` or `false`."
                    ),
                ));
            }
        } else if trimmed.starts_with("max_age ") {
            // max_age shape is adapter-parseable; LSP just confirms
            // the token is present + quoted.
            let rest = trimmed.strip_prefix("max_age ").unwrap().trim();
            if !rest.starts_with('"') {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors max_age` requires a quoted duration string (e.g. `\"1h\"`).",
                ));
            }
        } else {
            // Unknown child of `cors`.
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "cors_contract_diagnostics",
                "`cors` children are `allow_origins`, `allow_credentials`, or `max_age`.",
            ));
        }
    }

    diagnostics
}

/// Cut A.9 — file-local checks on `approval` blocks declared inside
/// commands. Required children present (`by`, `timeout`, `then`),
/// `then` value in the closed catalog, `by` non-empty. Cross-feature
/// role resolution lives in doctor.
fn approval_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
fn reserved_trace_event_diagnostics(source: &str) -> Vec<Diagnostic> {
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
fn is_lower_ident(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn emits_derived_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn notification_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading != 2 || !trimmed.starts_with("notification ") {
            index += 1;
            continue;
        }

        let header_index = index;
        let mut has_channel = false;
        let mut has_recipient = false;
        let mut has_trigger = false;
        let mut has_template = false;
        let mut has_policy = false;
        let mut bad_channel: Option<(usize, String)> = None;
        let allowed_channels = ["email", "push", "sms", "in_app"];

        index += 1;
        while index < lines.len() {
            let inner = lines[index];
            let inner_trimmed = inner.trim_start();
            let inner_leading = leading_spaces(inner);

            if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                index += 1;
                continue;
            }
            if inner_leading <= 2 {
                break;
            }
            if inner_leading == 4 {
                if let Some(rest) = inner_trimmed.strip_prefix("channel ") {
                    has_channel = true;
                    for ch in rest.split(',').map(|c| c.trim()) {
                        if !allowed_channels.contains(&ch) {
                            bad_channel = Some((index, inner.to_owned()));
                        }
                    }
                } else if inner_trimmed.starts_with("recipient ") {
                    has_recipient = true;
                } else if inner_trimmed.starts_with("trigger ") {
                    has_trigger = true;
                } else if inner_trimmed.starts_with("template ") {
                    has_template = true;
                } else if inner_trimmed.starts_with("policy ") {
                    has_policy = true;
                }
                // Notifications expanded bucket cycle — `digest` /
                // `throttle` sub-blocks are recognised here only so
                // the file-local diagnostic does not flag the
                // headers as malformed children. The structural
                // checks live in `tier3_notification_diagnostics`
                // (six `NOTIF-DIGEST-*` / `NOTIF-THROTTLE-*` codes)
                // and run against the typed IR.
            }
            index += 1;
        }

        for (label, present) in [
            ("`channel <email|push|sms|in_app>[, ...]`", has_channel),
            ("`recipient <expression>`", has_recipient),
            ("`trigger event <pattern>`", has_trigger),
            ("`template \"./path\"`", has_template),
            ("`policy @policy.<name>`", has_policy),
        ] {
            if !present {
                diagnostics.push(simple_canonical_diagnostic(
                    header_index,
                    lines[header_index],
                    DiagnosticSeverity::ERROR,
                    "notification-contract",
                    &format!("`notification` declarations must declare {label}."),
                ));
            }
        }

        if let Some((idx, owned_line)) = bad_channel {
            diagnostics.push(simple_canonical_diagnostic(
                idx,
                &owned_line,
                DiagnosticSeverity::ERROR,
                "notification-contract",
                "`channel` accepts a comma-separated subset of `email`, `push`, `sms`, `in_app`.",
            ));
        }
    }

    diagnostics
}

fn has_many_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn split_derived_from(rhs: &str) -> Option<(&str, &str)> {
    if let Some(pos) = rhs.find(" derived from ") {
        return Some((&rhs[..pos], &rhs[pos + " derived from ".len()..]));
    }
    if let Some(stripped) = rhs.strip_suffix(" derived from") {
        return Some((stripped, ""));
    }
    None
}

fn contains_top_level_eq(expr: &str) -> bool {
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

fn field_typed_rhs(trimmed: &str) -> Option<&str> {
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
            // Accepted forms (per docs/invariants.md):
            //   <feature>.query.<name>              — fully qualified
            //   <feature>.query.<name>(<args>)      — fully qualified with args
            //   <feature>.query.*                   — feature-local wildcard
            //   query.<name>                        — same-feature short form
            //   query.*                             — same-feature wildcard
            let entry = trimmed.split_whitespace().next().unwrap_or("");
            let valid = entry.contains(".query.") || entry.starts_with("query.");
            if !valid {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-invalidation-contract",
                    "cache invalidation entries should target queries: `<feature>.query.<name>`, `<feature>.query.*`, `query.<name>` (same feature), or `query.*` (same feature).",
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
    let mut in_contract = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            in_contract = trimmed.starts_with("contract ");
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

        // Inside a top-level `contract <name>` block, `error` cases on
        // operations expose schema-defined fields, not the
        // command-level `message|code|data` envelope. The
        // contract-operation validator handles the shape; skip the
        // command-level rules here.
        if in_contract {
            continue;
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

fn stack_kind(trimmed_line: &str) -> Option<&'static str> {
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

fn workspace_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn validate_workspace_app_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn validate_workspace_boundary_line(
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

fn validate_workspace_communication_line(
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

fn validate_workspace_gateway_route_line(
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

fn validate_workspace_gateway_route_child(
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

fn quoted_prefix(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}

fn external_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn validate_contract_import_line(
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

fn validate_contract_operation_line(
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

fn is_contract_operation_retry(parts: &[&str]) -> bool {
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

fn is_contract_operation_idempotency(parts: &[&str]) -> bool {
    parts.len() >= 3
        && parts[0] == "idempotency"
        && parts[1] == "by"
        && parts.iter().skip(2).all(|t| !t.is_empty())
}

fn is_contract_operation_error(parts: &[&str]) -> bool {
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

fn validate_contract_field_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn is_contract_name(value: &str) -> bool {
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

fn is_contract_type_token(value: &str) -> bool {
    value.starts_with("@semantic.")
        || value.starts_with("@cap.")
        || is_type_name(value)
        || matches!(
            value,
            "ID" | "Text" | "Integer" | "Decimal" | "Float" | "Boolean" | "DateTime" | "Date"
        )
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

fn app_child_block(trimmed: &str) -> Option<&'static str> {
    let first = trimmed.split_whitespace().next()?;
    match first {
        "uses" => Some("uses"),
        "packs" => Some("packs"),
        "bindings" => Some("bindings"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "cors" => Some("cors"),
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
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

fn registry_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                current_child = match trimmed {
                    "env" => Some("env"),
                    "capabilities" => Some("capabilities"),
                    "integrations" => Some("integrations"),
                    "packs" => Some("packs"),
                    "tools" => Some("tools"),
                    // Webhooks expanded cycle — registry-side catalog
                    // of expected inbound envelope shapes. Indent-4
                    // entries open new envelopes; indent-6 children
                    // declare typed fields. Validation lives in the
                    // doctor path (`WEBHOOK-PAYLOAD-001` etc.); the
                    // LSP contract diagnostic only suppresses the
                    // unknown-block warning.
                    "webhook_events" => Some("webhook_events"),
                    _ => {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "registry blocks use `env`, `capabilities`, `integrations`, `packs`, `tools`, and `webhook_events`.",
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

fn profile_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn profile_child_kind(trimmed: &str) -> Option<&'static str> {
    match trimmed {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

fn validate_profile_url_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn is_profile_binding_line(trimmed: &str) -> bool {
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

fn validate_profile_integration_line(
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
        "profile integration overrides use `<integration> environment sandbox|production` or `<integration> adapter <source>`, where adapter sources are `@runtime/...`, `@plugin/publisher/name`, `@adapter.local`, or a local path.",
    ));
}

fn validate_profile_deploy_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn feature_requirements_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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

fn validate_feature_requirement_line(
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

fn parse_feature_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
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

fn external_call_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
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
struct ExternalCallBlockFacts {
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
struct ExternalCallLine {
    line_index: usize,
    line: String,
}

fn external_call_block_diagnostics(block: ExternalCallBlockFacts) -> Vec<Diagnostic> {
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

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
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

fn command_name_if(trimmed: &str) -> Option<String> {
    named_block_name(trimmed, "command").map(str::to_owned)
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
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

fn validate_app_binding_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn validate_app_pack_use_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
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

fn validate_registry_pack_header(
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

fn validate_registry_pack_child(
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

fn parse_app_binding_line(trimmed: &str) -> Option<(&str, &str, &str)> {
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
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration children use `adapter @runtime/...`, `adapter @plugin/publisher/name`, `adapter @adapter.<local>`, local adapter paths, `environments ...`, `credentials platform|tenant|actor`, or `data_classification @pii.<class>`.",
        )),
    }
}

fn adapter_source_provenance(source: &str) -> Option<&'static str> {
    if source
        .strip_prefix("@runtime/")
        .is_some_and(valid_pathish_tail)
    {
        Some("runtime")
    } else if source
        .strip_prefix("@plugin/")
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

fn valid_plugin_tail(value: &str) -> bool {
    value.split('/').filter(|part| !part.is_empty()).count() >= 2
        && value.split('/').all(valid_path_segment)
}

fn valid_pathish_tail(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_path_segment)
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
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
            "app capabilities declare intent such as `database postgres`, `queue background_jobs`, `object_storage files`, or `integration crm`; providers stay in adapters under `@runtime/<name>`.",
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

fn env_top_level_legacy_diagnostics(source: &str) -> Vec<Diagnostic> {
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
                        "environment reference `env.{reference}` should be declared in `registry.env` with scope, type, and requiredness. Doctor cross-checks against the package registry; this LSP rule only sees the current file.",
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
            | "auth_password_algorithm_hash_mismatch"
            | "auth_sessions_resource_unknown"
            | "auth_identity_field_unknown"
            | "auth_oauth_adapter_unbound"
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

fn is_design_lzi_uri(uri: &Url) -> bool {
    uri.path().ends_with("design.lzi")
}

fn is_lzx_uri(uri: &Url) -> bool {
    uri.path().ends_with(".lzx")
}

fn design_keyword_completion_items() -> Vec<CompletionItem> {
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

/// Row 30 — context-aware closed-catalog completions for the four
/// `@cap.File(...)` argument values. Returns `None` outside of
/// `@cap.File(...)`; when inside, looks at the most recent keyword
/// before the cursor to pick the right catalog:
///
/// - `visibility:` → `public`, `private`, `signed`
/// - `max_size:<int>` → `kb`, `mb`, `gb`
/// - `signed_ttl:<int>` → `s`, `m`, `h`, `d`
/// - `accept:` → the seven IANA-top families (`text`, `image`, …, `*`)
fn cap_file_value_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
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

fn design_keyword_description(keyword: &str) -> Option<&'static str> {
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

pub fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "workspace" => Some(
            "Declares an optional distributed-system contract across local apps and external services.",
        ),
        "app" => Some("Declares the `.lzi` application entrypoint and operational contract."),
        "registry" => Some(
            "Declares the package-level catalog for env, capabilities, integrations, and packs.",
        ),
        "profile" => Some(
            "Declares environment-specific app overrides such as public URLs, sandbox integrations, binding overrides, and deploy topology.",
        ),
        "apps" => Some("Groups apps that participate in a workspace graph."),
        "shared_registry" => {
            Some("Declares the package registry shared by apps in a workspace contract.")
        }
        "boundaries" => Some("Groups workspace event publication and consumption edges."),
        "gateway" => {
            Some("Declares provider-neutral public ingress routes for a distributed workspace.")
        }
        "contract" => {
            Some("References a versioned external service contract, not an implementation.")
        }
        "compatibility" => Some("Declares the external contract compatibility policy."),
        "import" => Some(
            "Imports an external contract schema such as OpenAPI, AsyncAPI, Proto, JSON Schema, or Avro.",
        ),
        "operation" => Some("Declares one provider-neutral external service operation."),
        "environments" => {
            Some("Declares deployment/runtime environments such as local, staging, and production.")
        }
        "urls" => {
            Some("Declares public app URLs used by clients, CORS, emails, callbacks, and webhooks.")
        }
        "bindings" => Some(
            "Binds abstract feature requirements to concrete app or registry integration entries.",
        ),
        "packs" => Some(
            "Declares Lazuli pack catalog entries in `registry.lzi` or enabled pack references in `app.lzi`.",
        ),
        "provides" => {
            Some("Declares what a registry pack provides, such as `provides feature payments`.")
        }
        "from" => Some(
            "Declares a source relationship, such as pack enablement or create-from-input sugar.",
        ),
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
        "topology" => Some("Declares an environment deploy topology override in a profile."),
        "environment" => Some("Selects a provider environment such as sandbox or production."),
        "env" => Some("Declares typed environment variables and client/server exposure."),
        "aggregate" | "entity" => Some("Declares a domain resource with fields and behavior."),
        "record" => Some("Declares a non-persisted typed result/DTO shape."),
        "agent" => Some(
            "Declares an LLM-powered capability with typed input, output, prompt template, model reference, policy, and rate limits. the runtime wires the LLM transport; Lazuli owns the contract.",
        ),
        "notification" => Some(
            "Declares a multi-channel outbound notification with `channel`, `recipient`, `trigger`, `template`, and `policy`. the runtime generates dispatch wiring; adapters (Sendgrid/SES/Twilio/APNs/FCM) handle transport.",
        ),
        "channel" => Some(
            "Two distinct uses, disambiguated by indent level:\n\n\
             • Feature-level kind: `channel <name>` declares a typed, tenant-scoped, \
               policy-gated push stream (realtime bucket cycle MVP). Required children: \
               `tenant_from <axis>`, `policy @policy.<name>`, `payload <RecordType>`. \
               Transport (WebSocket / SSE) is adapter-resolved at runtime; the language \
               declares the contract. Doctor: `CHANNEL-PAYLOAD-001`.\n\n\
             • On a `notification`, declares one or more delivery channels: \
               `email`, `push`, `sms`, `in_app`.",
        ),
        "recipient" => Some(
            "On a `notification`, declares the recipient expression (e.g., `target.email`, `payload.user_id`).",
        ),
        "template" => {
            Some("On a `notification`, points to the template file at `./path` (mjml/mdx/text).")
        }
        "digest" => Some(
            "On a `notification`, declares window-based aggregation. Children: `every \"<duration>\"` (required), `group_by <payload-path>`, `max_size <N>` (1..=10000), `template_strategy merge|append`. Distinct from scalar `rate_limit`. Doctor: `NOTIF-DIGEST-001/002/003`.",
        ),
        "throttle" => Some(
            "On a `notification`, declares structured per-recipient / per-channel rate-limit with optional burst. Children: `max_per \"<duration>\"` (required), `per_recipient`, `per_channel`, `burst <N>`. Distinct from scalar `rate_limit` (which is per-call). Doctor: `NOTIF-THROTTLE-001/002/003`.",
        ),
        "every" => Some(
            "On `notification.digest`, sets the aggregation window. Closed shape: `<N> (seconds|minutes|hours|days)`. Example: `every \"15 minutes\"`.",
        ),
        "group_by" => Some(
            "On `notification.digest`, keys the aggregation bucket on a payload path.",
        ),
        "max_size" => Some(
            "On `notification.digest`, caps items per digest window. Range: 1..=10000. Above the ceiling buffers unbounded payloads.",
        ),
        "template_strategy" => Some(
            "On `notification.digest`, declares how the adapter combines per-trigger payloads when rendering the digest template. Closed catalog: `merge` (last-write-wins per key), `append` (emits a list).",
        ),
        "max_per" => Some(
            "On `notification.throttle`, sets the refill window for the rate-limit bucket. Closed shape: `<N> (seconds|minutes|hours|days)`.",
        ),
        "per_recipient" => Some(
            "On `notification.throttle`, keys the throttle bucket on the notification's `recipient <path>`. At least one of `per_recipient` or `per_channel` is required.",
        ),
        "per_channel" => Some(
            "On `notification.throttle`, gives each channel of a multi-channel notification its own bucket (e.g., email and `in_app` throttled independently).",
        ),
        "burst" => Some(
            "On `notification.throttle`, number of immediate dispatches the bucket allows before throttling starts. Useful for OTP / login flows.",
        ),
        "model" => {
            Some("On an `agent`, references the LLM model under the `@llm.<name>` namespace.")
        }
        "prompt" => Some("On an `agent`, points to the prompt template file at `./path`."),
        "tools" => Some(
            "On an `agent`, declares the closed list of `@tool.<name>` references the agent may invoke.",
        ),
        "safety" => Some(
            "On an `agent`, declares safety classifiers or policy checks applied to inputs/outputs.",
        ),
        "stream" => Some(
            "On an `agent` `output`, marks the response as a streamed value: `output stream <Type>`.",
        ),
        "command" => Some("Declares a write operation for an aggregate."),
        "query.list" => Some("Declares a generated collection query."),
        "query.lookup" => Some("Declares a generated single-record lookup query."),
        "query.sql" => Some("Declares a query backed by an external SQL file."),
        "defaults" => Some("Declares repeated feature defaults such as tenancy and timestamps."),
        "domain" => Some("Groups resources, records, queries, rules, and events."),
        "policies" => Some("Declares feature-local policy categories and field policies."),
        "auth" => Some(
            "Authentication block: groups identity, password, sessions, MFA, and OAuth subcontracts for a feature.",
        ),
        "identity" => Some(
            "`identity <Resource>.<field>` — names the resource field used as the canonical login identifier.",
        ),
        "oauth" => Some(
            "OAuth subcontract: `oauth <provider>` with `adapter @adapter.<x>`. v0 providers: `google`, `github`, `microsoft`, `apple`.",
        ),
        "mfa" => {
            Some("MFA subcontract: `mfa <method>` with `enroll` + `verify`. v0 method: `totp`.")
        }
        "sessions" => Some("Sessions subcontract: backing resource + ttl + refresh policy."),
        "refresh" => Some("Whether the session adapter issues refresh tokens. Default `false`."),
        "enroll" => {
            Some("Enrolment function reference (`@fn.*`) returning method-specific enrolment data.")
        }
        "verify" => {
            Some("Verification reference (`@fn.*` or `@validator.*`) returning success/failure.")
        }
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
        "previously" => Some(
            "Declares identity continuity with an explicit `migrated` or `alias` mode. Doctor: `PREVIOUSLY-FWD-001` rejects stale rename targets; `PREVIOUSLY-CYCLE-001` rejects A→B→A cycles; `PREVIOUSLY-DUP-001` rejects two current names claiming the same previous identity.",
        ),
        "migrated" => Some(
            "Marks a previous name as migration-only history, not a generated compatibility alias.",
        ),
        "alias" => Some(
            "Marks a previous name as a temporary compatibility alias still accepted by generated surfaces.",
        ),
        "path" => Some("Declares a concrete URL path for app routes, APIs, or webhooks."),
        "params" => Some("Declares typed query or API parameters."),
        "to" => Some("Binds a top-level `.lzx` route to an abstract experience view."),
        "let" => Some("Binds a derived value for later command, job, or event expressions."),
        "derived" => Some(
            "Marks a resource field as computed at read time: `<name>: <Type> derived from <expression>`. Not persisted; cannot have `default`, `required`, or `optional`.",
        ),
        "audit" => Some(
            "Declares an operation as audited. Use `audit` for default fields, `audit <field>, <field>` for explicit entries, or `audit none` to opt out.",
        ),
        // Wave B — `effect` closed-catalog verbs on commands. Cursor
        // hover on the verb shows the one-liner; richer Markdown
        // template lives in `rich_keyword_hover("effect")`.
        "creates" => Some(
            "Command write effect — creates a new row of `<Resource>`. Body assigns input/derived values to fields. One mutating effect per command.",
        ),
        "updates" => Some(
            "Command write effect — mutates the loaded `target` of `<Resource>`. Body assigns the changed fields. One mutating effect per command.",
        ),
        "deletes" => Some(
            "Command write effect — removes the loaded `target` of `<Resource>`. Soft-delete is automatic when the resource declares `soft_delete`.",
        ),
        "returns" => Some(
            "Command non-mutating effect — returns a `<Record>` shape without writing to a resource. Also valid on `query.sql` as `returns <Type>`.",
        ),
        // PG.B — Plan & Gate vocabulary hovers.
        "plan" => Some(
            "Subscription tier declaration. Declares a feature set and a limit set, optionally with a `trial` revert policy. The catalog is package-wide; the union of every plan's `features`/`limits` forms the closed set for `gate` directives.",
        ),
        "features" => Some(
            "Comma-separated identifier list of features in this plan. Cross-plan reuse: `features <other_plan>.features`. References at call sites: `gate behind plan.feature: <name>`.",
        ),
        "limits" => Some(
            "Comma-separated `<name> <value>` pairs. Value is a positive integer or the literal `unlimited`. Cross-plan reuse: `limits <other_plan>.limits`. References at call sites: `gate quota plan.limit: <name>`.",
        ),
        "trial" => Some(
            "Trial revert policy on a plan: `trial duration <integer><s|m|h|d>, then <plan>`. Runtime watches the subscription's expires_at and reverts to `<plan>` after the duration.",
        ),
        "unlimited" => Some(
            "Limit value meaning the runtime emits no quota check at this tier. Use to opt a plan out of a quota that other plans declare.",
        ),
        "subscription" => Some(
            "App-level directive: `subscription resource <feature>.<field>` names the resource that holds the active subscription. Required when any callable uses `gate behind plan.*` or `gate quota plan.*`. Exactly one per app.",
        ),
        "gate" => Some(
            "Subscription gate on a callable. Two forms: `gate behind plan.feature: <name>` (boolean, 402 plan.feature_forbidden on refusal) or `gate quota plan.limit: <name>` (counter, 402 plan.quota_exceeded; increments after success).",
        ),
        "behind" => Some(
            "Boolean gate: `gate behind plan.feature: <name>`. Refuses dispatch when the caller's active plan does not list `<name>` in its `features` set. Evaluates before `policy`.",
        ),
        "quota" => Some(
            "Counter gate: `gate quota plan.limit: <name>`. Refuses dispatch when period usage has reached the plan's value for `<name>`; increments after successful dispatch.",
        ),
        "has_many" => Some(
            "Declares a collection on a resource: `has_many <name>: <Type> [inverse <field>]`. the runtime generates the inverse lookup query and foreign-key contract.",
        ),
        "inverse" => Some(
            "Declares the field on the target resource that owns the inverse foreign key for a `has_many` collection.",
        ),
        "policy" => Some("Associates a command with an authorization policy capability."),
        "policy_for" => Some("Declares a feature default policy for specific construct families."),
        "rate_limit" => Some("Declares a generated throttle policy for a command or auth flow."),
        "calls" => Some(
            "Declares that a command or job calls an abstract integration/service operation; the runtime wires this to Go transport bindings.",
        ),
        "method" => Some("Declares the HTTP method for a custom API endpoint."),
        "output" => Some("Declares the response shape for a custom API endpoint."),
        "locale" => Some(
            "App locale contract: `default <tag>`, `supported <tags>` (comma-separated), optional `fallback <src> -> <dst>` edges. Supersedes the bare `default_locale` scalar when present. BCP-47 tags (e.g. `pt-BR`, `en-US`).",
        ),
        "supported" => Some(
            "List of BCP-47 tags the app accepts. The locale-negotiation middleware matches `Accept-Language` against this list.",
        ),
        "fallback" => Some(
            "Locale fallback edge: `fallback <src> -> <dst>`. When a translation is missing in the source tag, the runtime walks fallbacks before defaulting to `app.locale.default`.",
        ),
        "cache" => Some(
            "Query cache contract: `key <expr>` + `ttl <duration>` (+ optional `tags <label>...` for fan-out invalidation, `namespace <label>` for cross-feature scoping). Requires a `cache <name>` capability in `registry.lzi`.",
        ),
        "key" => Some("Declares a cache key, lookup key, or dedupe key depending on context."),
        "ttl" => Some(
            "Cache time-to-live. Closed unit catalog: `s`, `m`, `h`, `d` (e.g. `5m`, `7d`). Quoted prose (`\"5 minutes\"`) also accepted; adapters parse it.",
        ),
        "tags" => Some(
            "Cache tags: comma-separated labels used by `invalidates tag:<label>` for fan-out invalidation across queries. Labels are author-defined lowercase identifiers.",
        ),
        "namespace" => Some(
            "Cache namespace label. Scopes the cache key beyond the default `<feature>.query.<name>` to avoid collisions in workspace / pack deployments. One namespace per query.",
        ),
        "invalidates" => Some("Declares queries that become stale after a command succeeds."),
        "error" => Some("Declares a named public error case with status and exposure fields."),
        "expose" => Some("Declares which error fields are visible to generated clients."),
        "write_window" => Some("Declares the temporal write window checked before a command runs."),
        "idempotency" => Some(
            "Declares a dedupe key for jobs, webhooks, and notifications. `idempotency by <path>` — re-fires sharing the same key are no-ops. Common paths: `envelope.id`, `payload.batch_id`, `tenant.org_id, schedule.day`.",
        ),
        "job" => Some(
            "Declares a unit of asynchronous or scheduled work. `trigger event ...` runs as a reactor; `trigger schedule \"<cron>\"` runs as scheduled. Add `queue <lane>` to enqueue rather than run inline. Body is either `handler \"./...\"` or a declarative target / let / updates / emits chain.",
        ),
        "webhook" => Some(
            "Declares a verified inbound HTTP integration boundary. Requires `path \"...\"` and `verify hmac <alg>` with nested `secret env.X` + `header \"X-...\"`. Multi-tenant apps must declare `tenant_from payload.<axis>_id`.",
        ),
        // Migrations bucket cycle Route C — `tenant_migration` kind +
        // deploy block expansion. See `docs/proposals/bucket-migrations-cycle.md`.
        "tenant_migration" => Some(
            "Per-tenant idempotent schema migration. Closed body: `target tenants <axis>` (required), `idempotency by <path>` (required), `retry`, `timeout`, `handler \"./...\"`. No `emits` or business effects.",
        ),
        "strategy" => Some(
            "Migration deployment strategy. Closed catalog: `rolling` (zero-downtime), `blue_green` (parallel cutover), `canary` (incremental traffic shift). Doctor: `DEPLOY-STRATEGY-001`.",
        ),
        "lock_timeout" => Some(
            "Max time to wait for the migration advisory lock before aborting. Adapter-parsed duration literal (`\"30s\"`, `\"5m\"`).",
        ),
        "pre_migration_hook" => Some(
            "Shell script the runtime executes before applying migrations. Path is relative to `app.lzi`.",
        ),
        "post_migration_hook" => Some(
            "Shell script the runtime executes after applying migrations. Path is relative to `app.lzi`.",
        ),
        "checkpoint" => Some(
            "Pinned IR snapshot for migration planning. `checkpoint <name> \"<path>\"` records a baseline; `lazuli plan --check <name>` validates the snapshot's integrity. Doctor: `DEPLOY-CHECKPOINT-001` (path missing) + `DEPLOY-CHECKPOINT-002` (version drift).",
        ),
        // OpenAPI bucket cycle — `deprecated` decorator + sub-fields.
        "deprecated" => Some(
            "Marks the command (or api, post-Tier-4) as deprecated. Inline form: `deprecated [since \"<version>\"] [replacement <ref>] [sunset \"<YYYY-MM-DD>\"]`. Generates OpenAPI `deprecated: true` + `Sunset` HTTP header. Doctor: `deprecated_replacement_unknown`, `deprecated_sunset_date_invalid`, `deprecated_sunset_in_past`.",
        ),
        "since" => Some(
            "Version string when the deprecation was declared. Free-form (semver, calendar, git-sha); emitted verbatim as OpenAPI `x-lazuli-deprecated-since`.",
        ),
        "replacement" => Some(
            "Replacement reference for a deprecated command. Resolves to a same-feature command name, a `<feature>.command.<name>` qualified ref, or an `https://` URL.",
        ),
        "sunset" => Some(
            "ISO-8601 date (`YYYY-MM-DD`) when consumers must stop using this endpoint. Emitted as OpenAPI `x-lazuli-sunset` and HTTP `Sunset` header.",
        ),
        // i18n bucket cycle — locale / translation / locale_negotiate.
        "translation" => Some(
            "Feature-scoped translation block. Declares a catalog path (`./i18n/<feature>.<locale>.json`) and typed keys. Each key declares one variant per `app.locale.supported` tag, plus optional CLDR plural arms (`zero/one/two/few/many/other`).",
        ),
        "catalog" => Some(
            "Translation catalog path. Carries `<locale>` placeholder; the runtime resolves it per request, e.g. `./i18n/customer.pt-BR.json`. Format (JSON/YAML/ICU MessageFormat) is an adapter contract on the Lazuli runtime side.",
        ),
        "locale_negotiate" => Some(
            "Per-runtime-unit (or per-api) middleware that resolves the request locale into `ctx.locale`. Closed catalog: `source` ∈ {accept_language|query_param|cookie|user_profile|subdomain}, `strategy` ∈ {best_match|prefix_match|exact_match}, optional `fallback <tag>`.",
        ),
        "source" => Some(
            "Inside `locale_negotiate`: the request axis the runtime reads to determine the locale. Closed catalog: `accept_language`, `query_param`, `cookie`, `user_profile`, `subdomain`.",
        ),
        "supported" => Some(
            "List of BCP-47 tags `app` accepts. The negotiation middleware matches `Accept-Language` against this list; `app.locale.default` must appear here.",
        ),
        "plural" => Some(
            "CLDR plural arm. Closed catalog: `zero`, `one`, `two`, `few`, `many`, `other`. The actual rule for which arm fires is locale data from CLDR, not language-declared.",
        ),
        "trigger" => Some(
            "Declares the event or schedule that starts a job or notification. `trigger event <feature>.<event>` for reactors; `trigger schedule \"<cron>\"` for scheduled work.",
        ),
        // L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md).
        "poller" => Some(
            "Declares an async resolution loop over a persistent cursor table. `poller <name>` is a feature-level kind parallel to `job` / `webhook` / `notification`. Closed children: `source <Resource>`, `cursor`, `retry`, `states`, `resolve via @fn.<name>`, `terminal_status_field`, `terminal_result_field`, `tick every <duration> batch <int>`, `tenant_from row.<axis>_id`, `idempotency by row.<field>, ...`, `audit`, `emits`, `retry_quirk` (closed catalog).",
        ),
        "cursor" => Some(
            "Inside `poller`: names the three closed cursor fields on `source`. Body is exactly `eligible_when <next_at>, <resolved_at>` + `attempts <field>`. The runtime selects rows where `next_at <= NOW() AND resolved_at IS NULL`.",
        ),
        "eligible_when" => Some(
            "Inside `poller cursor`: the two field names that gate row eligibility. Positional pair: `eligible_when <next_check_at>, <resolved_at>` — first is `DateTime required`, second is nullable `DateTime`.",
        ),
        "tick" => Some(
            "Inside `poller`: wall-clock cadence. `tick every <duration> [batch <int>]`. Defaults: `every 30s`, `batch 100`. Duration unit catalog is closed (`s`/`m`/`h`/`d`); doctor warns when `every < 5s` (POLLER-TICK-TOO-FAST-001).",
        ),
        "retry_quirk" => Some(
            "Inside `poller`: closed-catalog retry transformation. v0.1 catalog: `gender_flip_once`. Body: `when <predicate>`, `counter <field>`, `mutate <field> = <transform>`. No predicate sublanguage; for arbitrary mutation, drop to a `command` chained off `emits`.",
        ),
        "backoff" => Some(
            "On `retry`: closed-catalog backoff strategy. Catalog: `fixed`, `linear`, `exponential`. `linear` and `exponential` require `base <duration>`; `exponential` strongly recommends `cap <duration>` (POLLER-EXPONENTIAL-NO-CAP-001).",
        ),
        "resolve" => Some(
            "Inside `poller`: names the per-row handler. `resolve via @fn.<name>` — handler signature is derived from the poller's row + state + terminal types (`poller.ResolveResult[State, Terminal, Result]`). Doctor enforces the `@fn.<name>` is declared in feature `extensions` (POLLER-HANDLER-ORPHAN-001).",
        ),
        "retry" => Some(
            "Declares retry attempts and backoff strategy. For jobs: `retry <count> backoff <fixed|exponential>`. For pollers: `retry` block with `max_attempts <int>` + `backoff <strategy> [base <d>] [cap <d>]`. v0 catalog is closed.",
        ),
        "queue" => Some(
            "Declares an async queue lane for event-triggered jobs. Without `queue`, event jobs run inline as reactors; with `queue <lane>`, the runtime adapter dispatches via the queue (River, Asynq).",
        ),
        "tenant_from" => Some(
            "Pins an event/job/webhook/notification's tenant context from a payload path. `tenant_from payload.<axis>_id` — doctor cross-checks the axis against the feature's tenancy.",
        ),
        "fanout" => Some(
            "Declares per-tenant expansion for scheduled jobs. `fanout tenants <axis>` runs one execution per tenant per fire. Requires `idempotency by ...` to avoid double-execution on re-fires (warning `JOB-FANOUT-002`).",
        ),
        "external_calls" => Some(
            "Inspect projection of every `calls <slot>.<op>` inside a job body. Doctor uses it to enforce timeout, retry, and idempotency on each external call (`INT-CALL-*`, `JOB-TIMEOUT-001`).",
        ),
        "payload_group" => Some(
            "On a notification template binding, references a shared `event_group` payload schema. The runtime hydrates the template with the named group's payload shape.",
        ),
        "payload" => Some(
            "On an `event_group`, declares the shared event payload schema for every concrete event under the group. Field-binding lines (`customer_id = id`) compile into the group's typed payload.",
        ),
        "encryption" => Some(
            "App-level encryption key binding catalog. One `key @key.<scope>` child per `@cap.Encrypted` / `@cap.E2ee` scope used in the capsule. Closed catalog: `@key.app`, `@key.tenant`, `@key.user`, `@key.record`. Per `docs/proposals/encryption-vocab.md`.",
        ),
        "rotation" => Some(
            "Key rotation strategy on `encryption.key @key.<scope>`. v0 catalog: `manual` (rewrite env, re-encrypt rows via a job). `kms_managed` is deferred to a future cut.",
        ),
        "reason" => Some("Documents why a dangerous declarative override is intentional."),
        "requires" => Some(
            "Declares a feature requirement or an additional authority requirement for a workflow transition.",
        ),
        "integration" => {
            Some("Declares an abstract external integration requirement or registry capability.")
        }
        "password" => Some("Password subcontract: hash + verify + algorithm (+ rate_limit)."),
        "hash" => Some(
            "Hashing function reference (`@fn.*`) returning a `@cap.Hashed(algorithm:<X>)` value.",
        ),
        "algorithm" => Some(
            "Password hash algorithm. v0: `argon2id` (recommended) | `bcrypt` (legacy migration).",
        ),
        "secret" => Some("Declares the secret source for declarative webhook verification."),
        "header" => Some("Declares the signature header for declarative webhook verification."),
        "modifier" => Some("Attaches a query modifier extension to a generated query."),
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
        "adapter" => Some(
            "Adapter slot: `@runtime/...`, `@plugin/publisher/name`, `@adapter.<local>`, or a local path. Inside `auth`, resolved against `extensions adapter <name>` or `registry.integrations`.",
        ),
        "query_modifier" => Some("Declares a reusable query modifier extension contract."),
        "escape_route" => Some("Declares a custom route outside generated UI ownership."),
        "group" => Some("Groups related app env declarations without creating a namespace."),
        "required" => Some("Marks a field as required."),
        "unique" => Some("Marks a field as unique."),
        "default" => Some("Declares a default field value."),
        // Row 30 — `@cap.File(...)` argument keywords. The wider
        // `@cap.File` decorator itself surfaces via the word-at-position
        // hover lookup; the parser swallows the `@` so the word is
        // typically `cap.File` — both are listed here for symmetry.
        "@cap.File" | "cap.File" => Some(
            "File capability: `max_size:<size>` + `accept:<mime>` (required) + `visibility:<mode>` (required on api outputs) + `signed_ttl:<duration>` (required when `visibility:signed`). Authored on resource fields and api outputs. Requires the package to declare an `object_storage` or `storage` capability.",
        ),
        "max_size" => Some(
            "Maximum upload size for a `@cap.File`. Closed unit catalog: `kb`, `mb`, `gb` (binary prefixes — `25mb` = 25 * 1024 * 1024 bytes).",
        ),
        "accept" => Some(
            "Accepted MIME types for a `@cap.File`; pipe-separated for alternatives, e.g. `text/csv|application/vnd.ms-excel`. Known families: `text`, `image`, `application`, `audio`, `video`, `font`, `*`. Subtype `*` is also valid.",
        ),
        "visibility" => Some(
            "Visibility of the file URL produced by `@cap.File`. Closed catalog: `public` (unguessable but un-gated, suits CDN-served static assets), `private` (policy-gated download handler), `signed` (time-limited signed URL — requires `signed_ttl`).",
        ),
        "signed_ttl" => Some(
            "Signed-URL TTL for `@cap.File(visibility:signed)`. Closed unit catalog: `s`, `m`, `h`, `d`. Forbidden when `visibility` is `public` or `private`.",
        ),
        // Report vocab — `report <name>` kind keywords. See
        // `docs/proposals/report-vocab.md` v0.2.
        "report" => Some(
            "Declares a tabular export contract (CSV / XLSX) on a feature. Replaces the `api + opaque handler` pattern for static-column exports. Body: `source <query_ref>`, `columns`, `formats csv|xlsx`, optional `storage`, `visibility`, `signed_ttl`, `filename`, `policy`, `rate_limit`, `audit`.",
        ),
        "columns" => Some(
            "On a `report`, declares the column list at compile time. Each row: `<name> from row.<field> | @fn.<name>(args) [label \"...\"] [format \"...\"]`. Doctor cross-checks `row.<field>` against the source query's projection via `REPORT-COLUMN-MISMATCH-001`.",
        ),
        "formats" => Some(
            "On a `report`, declares the export formats. Closed catalog: `csv`, `xlsx`. Each entry auto-mounts `GET /api/reports/<name>.<format>`. Unknown formats raise `REPORT-FORMAT-UNKNOWN-001`.",
        ),
        "filename" => Some(
            "On a `report`, declares the download filename template. Closed token catalog: `{format}`, `{ctx.now:<strftime>}` (strftime tokens `yyyy`, `mm`, `dd`, `HH`, `MM`, `ss`), `{ctx.user.id}`, `{ctx.tenant.id}`. Unknown tokens raise `REPORT-FILENAME-TOKEN-UNKNOWN-001`.",
        ),
        // Observability bucket cycle row 36 — `app.logging` /
        // `app.tracing` keywords. Each closed catalog matches the
        // doctor diagnostic.
        "logging" => Some(
            "App logging contract (`app.logging`). Closed catalogs: `level ∈ {debug, info, warn, error}`, `format ∈ {json, text}`, `redact ∈ {pii, none}`. Optional `sample_rate ∈ [0.0, 1.0]`. Profile-aware overrides.",
        ),
        "tracing" => Some(
            "App tracing contract (`app.tracing`). `propagate <bool>` toggles trace-context propagation. `sample_rate ∈ [0.0, 1.0]` for head sampling. `exporter <name>` resolves to a `registry.capabilities <name>: tracing` entry; runtime picks default when absent.",
        ),
        "level" => Some(
            "Severity level. Closed catalog: `debug`, `info`, `warn`, `error`. Shared by `app.logging.level` and `event.trace <name> level`.",
        ),
        "format" => Some(
            "Log encoding. Closed catalog: `json` (machine-parseable, production-friendly) or `text` (human-readable, dev-friendly).",
        ),
        "redact" => Some(
            "PII redaction policy. Closed catalog: `pii` (auto-strip fields tagged `@pii.*`) or `none` (no auto-redaction; adapter may still redact).",
        ),
        "sample_rate" => Some(
            "Sampling rate, float in `[0.0, 1.0]`. `1.0` captures everything; `0.0` disables capture (tracing still propagates context). Out-of-range values are rejected by doctor.",
        ),
        "propagate" => Some(
            "Trace-context propagation toggle. `true` (default) threads `trace_id` / `request_id` through downstream calls; `false` disables propagation but keeps span capture.",
        ),
        "exporter" => Some(
            "Tracing exporter slot. Must resolve to a `registry.capabilities <name>: tracing` entry. `None` lets the runtime pick a default (no-op or stdout).",
        ),
        // Observability bucket cycle row 37.
        "emit_to" => Some(
            "Audit destination. Resolves to one of the reserved streams (`audit_log`, `audit_stream`) or to an `event_group <name>` declared in the same feature. Without `emit_to`, the runtime falls back to `audit_log`.",
        ),
        // Webhooks expanded cycle — payload/replay/dlq hover catalog.
        "webhook_events" => Some(
            "Registry-side catalog of expected inbound envelope shapes. Each entry under `registry.webhook_events.<name>` is a typed external envelope referenced by `webhook ... payload from webhook_events.<name>`. Treated as external-origin: Lazuli does not assume the source is trustworthy, only that the contract matches what the provider documents.",
        ),
        "payload_from" => Some(
            "Typed reference to a `registry.webhook_events.<name>` envelope. Surface form: `payload from webhook_events.<name>`. Doctor cross-checks the envelope name and validates `tenant_from payload.<axis>` / `idempotency by payload.<axis>` against the declared fields.",
        ),
        "replay" => Some(
            "Declarative replay contract on an inbound webhook. Short form: `replay allow within \"<duration>\"`. Long form: `replay` header + nested `allow|deny` + optional `within \"...\"` + optional `dedupe by <path>`. `dedupe_by` defaults to the webhook's `idempotency by ...` path.",
        ),
        "allow" => Some(
            "On `replay`: re-deliveries within the window are accepted; the runtime returns 200 without re-running the handler. Requires `within \"<duration>\"`.",
        ),
        "within" => Some(
            "Replay window for `replay allow`. Quoted duration verbatim (e.g. `\"24h\"`, `\"7d\"`). The adapter parses; the language keeps the literal.",
        ),
        "dedupe" => Some(
            "On `replay`: `dedupe by <path>` overrides the dedupe key used to detect re-deliveries. Without `dedupe by`, replay reuses the webhook's `idempotency by ...` path.",
        ),
        "dlq" => Some(
            "Dead-letter routing after retry exhaustion. Three closed variants (mutually exclusive): `dlq emit <event>` publishes a tombstone event; `dlq handler \"./...\"` runs an adapter-side handler; `dlq drop` + `reason \"...\"` is an explicit waiver.",
        ),
        "emit" => Some(
            "On `dlq`: `dlq emit <event>` publishes a tombstone event onto the bus after retry exhaustion. The event must be declared in the same feature (via `emits`, `event_group`, or `event.trace`).",
        ),
        "drop" => Some(
            "On `dlq`: `dlq drop` discards re-delivery attempts after retry exhaustion. Must carry an explicit `reason \"...\"` waiver — silent drops on dead-letter are rejected by `WEBHOOK-DLQ-002`.",
        ),
        "from" => Some(
            "Catalog hop. In `payload from webhook_events.<name>`, points at the registry-side envelope shape. The `webhook_events.` prefix is mandatory at the surface so the catalog is obvious to a cold-reading author.",
        ),
        // RBAC catalog vocab — `permission` / `role` top-level kinds +
        // `inherits` / `grants` / `grants_all` children and the
        // `has_role` / `has_permission` policy predicates. See
        // `docs/proposals/rbac-catalog-vocab.md`.
        "permission" => Some(
            "Declares one closed-catalog permission at top level. Identifier is colon-separated, 2-4 segments (`<resource>:<action>` ... `<resource>:<action>:<scope>:<qualifier>`). Catalog is package-scoped; placement convention is `features/auth/auth.lzi`.",
        ),
        "role" => Some(
            "Declares one closed-catalog role at top level. Body accepts optional `inherits <role>` (single-parent) and exactly one of `grants` (indented list of permission refs) or `grants_all` (shorthand for every declared permission), or neither (inherits-only).",
        ),
        "inherits" => Some(
            "On `role`: single-parent inheritance (`inherits <role>`). Multi-parent (`inherits A, B`) is rejected in v0.1 — declare a chain instead. Closure is computed at compile time.",
        ),
        "grants" => Some(
            "On `role`: block listing the permissions granted by this role (one per line, indent 4). Each entry is a bare permission identifier resolved against the catalog. Mutually exclusive with `grants_all`.",
        ),
        "grants_all" => Some(
            "On `role`: shorthand granting every declared permission in the catalog. Mutually exclusive with `grants`. Newly added permissions are automatically included (useful for `admin`-style roles; LSP hover surfaces the resolved closure).",
        ),
        "has_role" => Some(
            "Closed predicate inside a `policy` expression: `has_role <name>` evaluates to true when the actor's current role is `<name>` or transitively inherits from it. Use `@role.<name>` inside `policies` dictionary entries instead.",
        ),
        "has_permission" => Some(
            "Closed predicate inside a `policy` expression: `has_permission <resource>:<action>` evaluates to true when the actor's current role grants the permission via the catalog closure. Reference must resolve against a declared `permission`.",
        ),
        _ => None,
    }
}

/// Rich Markdown hover for the closed-catalog DSL kinds the LSP knows
/// best. Each entry renders a one-line summary, required-children
/// bullets, optional-children bullets, a worked example, and a doc
/// anchor link. Markdown intentionally uses only the conservative
/// subset (headings via `**bold**`, bullet lists, fenced code blocks,
/// inline `[label](path)` links) so VS Code and Helix both render it
/// the same way; we don't use VS Code-only renderer features.
///
/// Falls back to `keyword_description` (one-liner) when no rich
/// template exists, so adding a kind here is strictly additive and
/// cannot regress unrelated hover output.
///
/// The seven canonical kinds covered today: `command`, `query.list`,
/// `query.lookup`, `query.sql`, `api`, `policy`, `effect`, `audit`,
/// `rate_limit`. `agent` keeps its existing one-line description plus
/// the enriched markdown here so the canonical hover pattern from the
/// agent cycle remains the reference shape.
pub fn rich_keyword_hover(keyword: &str) -> Option<String> {
    match keyword {
        "command" => Some(
            [
                "**`command`** — write operation on an aggregate. Lazuli owns the contract; the runtime emits a typed handler that runs effects, emits events, and invalidates queries.",
                "",
                "**Required children**",
                "- `policy @policy.<name>` — feature-local authorization category.",
                "- An effect line — exactly one of `creates`/`updates`/`deletes`, or a non-mutating `returns <Record>` shape.",
                "",
                "**Optional children**",
                "- `input` / short-form `input name, email` — submitted fields.",
                "- `route <name>: <Type>` — URL/context slots.",
                "- `rate_limit \"<N> per <window> per <axis>\"` — required when the policy includes `@scope.public` or the command mutates state.",
                "- `audit` / `audit <field>+` / `audit none` — audit-log contract.",
                "- `emits <event> [from creates|updates|deletes]` — domain event publication.",
                "- `invalidates query.<name>` — cache fan-out.",
                "- `approval` — conditional human sign-off block.",
                "- `validate @validator.<name>` — blocking validator.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  input",
                "    name: Text required",
                "  policy @policy.create",
                "  rate_limit \"30 per hour per ip\"",
                "  creates Customer",
                "    name = input.name",
                "  emits customer_created from creates",
                "```",
                "",
                "See [quickref.md §Minimal Feature](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "query.list" => Some(
            [
                "**`query.list`** — generated collection query. Defaults to `order created_at desc`; simple equality filters derive language-managed indexes.",
                "",
                "**Required children**",
                "- None at the syntax level — a bare `query.list <name>` is valid.",
                "",
                "**Optional children**",
                "- `params` — typed read arguments.",
                "- `filters` — equality / `when params.*` filter rows; derives indexes.",
                "- `search params.<name> over <field>...` with `mode contains|prefix|exact` — text matching (does not derive indexes).",
                "- `order <field> asc|desc` — override the `created_at desc` default.",
                "- `paginate <positive-int>` — generated default page size, not a hard maximum.",
                "- `cache key <expr> ttl <duration>` (+ optional `tags`, `namespace`).",
                "- `scope override` (+ `reason`) — cross-tenant / admin queries.",
                "- `policy @policy.<name>` — explicit category (required under `scope override`).",
                "- `modifier @query_modifier.<name>` — query-modifier extension.",
                "",
                "**Example**",
                "```lazuli",
                "query.list list",
                "  params",
                "    status: CustomerStatus optional",
                "  filters",
                "    status when params.status",
                "  paginate 50",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "query.lookup" => Some(
            [
                "**`query.lookup`** — generated single-record query. Single-key form sugars to `query.lookup <name> by <field>: <Type>`; composite/reshaped lookups use a `params`/`key` body.",
                "",
                "**Required slots**",
                "- A key spec — either `by <field>: <Type>` (single-key sugar) or a `params`/`key` body for composite lookups.",
                "",
                "**Optional children**",
                "- `params` — composite-key arguments (when `by` shorthand is not used).",
                "- `key` — explicit key composition for composite lookups.",
                "- `policy @policy.<name>` — explicit category.",
                "- `cache key <expr> ttl <duration>`.",
                "- `scope override` (+ `reason`) — cross-tenant lookups.",
                "",
                "**Example**",
                "```lazuli",
                "query.lookup by_id by id: ID",
                "",
                "query.lookup by_email by email: @semantic.Email",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "query.sql" => Some(
            [
                "**`query.sql`** — SQL-backed query wrapper. The result type must resolve to a `record`, resource, or registered contract before codegen; Lazuli does not infer result shape from SQL text.",
                "",
                "**Required children**",
                "- `returns <Type>` or `returns <Type>[]` — must resolve to a `record`, resource, or contract.",
                "- `sql \"./queries/<name>.sql\"` — relative path to the SQL file.",
                "",
                "**Optional children**",
                "- `params` — typed query arguments referenced inside the SQL file.",
                "- `scope` — tenancy or filter scope applied at codegen.",
                "- `policy @policy.<name>` — explicit category.",
                "- `cache key <expr> ttl <duration>`.",
                "",
                "**Example**",
                "```lazuli",
                "query.sql lifetime_value",
                "  returns CustomerLtv[]",
                "  scope",
                "    org = ctx.user.org",
                "  sql \"./queries/customer_lifetime_value.sql\"",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "api" => Some(
            [
                "**`api`** — custom typed HTTP endpoint outside `command`/`query`/`webhook` semantics. Use it when the handler does meaningful work beyond translating HTTP to a single dispatch; otherwise prefer `expose http` on an `agent` or a generated command/query.",
                "",
                "**Required children**",
                "- `method <GET|POST|PUT|PATCH|DELETE>` — HTTP verb.",
                "- `path \"<url>\"` — concrete URL path; `:slot` placeholders bind via `route`.",
                "- `output <Type>` — response shape (record, resource, or `@cap.File(...)`).",
                "- `policy @policy.<name>` — authorization category.",
                "- `handler @fn.<name>` or `handler \"./path.go\"` — handler reference.",
                "",
                "**Optional children**",
                "- `input <Type>` — request body shape.",
                "- `route <name>: <Type>` — one per `:slot` placeholder.",
                "- `rate_limit \"<N> per <window> per <axis>\"` — per-call throttle (required when policy includes `@scope.public`).",
                "- `audit` / `audit <field>+` / `audit none`.",
                "",
                "**Example**",
                "```lazuli",
                "api me",
                "  method GET",
                "  path \"/me\"",
                "  output User",
                "  policy @policy.authenticated",
                "  handler @fn.me",
                "```",
                "",
                "See [quickref.md §Security Checklist](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "policy" => Some(
            [
                "**`policy`** — feature-local authorization category reference on a `command`/`query`/`api`/`webhook`/`job`. The category resolves against the same feature's `policies` block unless feature-qualified.",
                "",
                "**Forms**",
                "- `policy @policy.<name>` — single category from the feature `policies` dictionary.",
                "- `policy @policy.<feature>.<name>` — cross-feature category (rarely needed).",
                "- On `policies` entry lines (atom decomposition): `<category>: <atom>[, <atom>]+` where each atom is `@role.*`, `@scope.*`, or `@actor.*`.",
                "- Predicate combinators inside categories: comma = OR, `and` = AND, parentheses for grouping (canonical closed predicate language).",
                "",
                "**Rules**",
                "- Commands declare `policy` explicitly — there is no implicit `creates -> @policy.create`.",
                "- Direct atoms (`@role.*`, `@scope.*`, `@actor.*`) belong in `policies` entries, not on individual command lines. Jobs / webhooks / escape routes may use atoms directly where appropriate.",
                "",
                "**Example**",
                "```lazuli",
                "policies",
                "  create: @role.admin, @role.sales",
                "  read: @scope.same_org",
                "",
                "command reassign",
                "  policy @policy.update",
                "```",
                "",
                "See [quickref.md §Policy Vocabulary](docs/quickref.md) and [invariants.md §Policies](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "effect" => Some(
            [
                "**`effect`** — write effect on a `command`. The closed catalog is `creates` / `updates` / `deletes` / `returns`. Exactly one mutating effect per command; `returns` is non-mutating.",
                "",
                "**Closed catalog**",
                "- `creates <Resource>` — new row; body assigns input/derived values to fields.",
                "- `updates <Resource>` — mutates the loaded `target`; body assigns changed fields.",
                "- `deletes <Resource>` — removes the loaded `target`. Soft-delete is automatic when the resource declares `soft_delete`.",
                "- `returns <Record>` — non-mutating command (no row write); the handler returns a typed record.",
                "",
                "**Rules**",
                "- One mutating effect per command. Multi-effect commands are rejected.",
                "- `target` is loaded before `updates`/`deletes` (explicit `target query.by_id(...)` or sugar when route/lookup match).",
                "- Event derivation works with effects: `emits <event> from creates|updates|deletes` maps the effect's bindings into the event payload by name.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  policy @policy.create",
                "  creates Customer",
                "    name = input.name",
                "    email = input.email",
                "  emits customer_created from creates",
                "```",
                "",
                "See [quickref.md §Canonical Sugar Table](docs/quickref.md) and [invariants.md §Source And Derived Views](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "audit" => Some(
            [
                "**`audit`** — declares an operation as audited so generated audit-log codegen has a typed contract instead of relying on event-name conventions. Surfaces in `lazuli inspect --expand=security`.",
                "",
                "**Forms**",
                "- `audit` — emit the default audit fields (`actor`, `tenant`, `target.id`, `ctx.now`).",
                "- `audit <field>, <field>, ...` — explicit field list. Each entry resolves against the command's binding namespaces (`input.*`, `route.*`, `target.*`, `ctx.*`, `payload.*`, etc.).",
                "- `audit none` — opt out of audit-log generation. Doctor records the opt-out so security review can see it.",
                "",
                "**Optional child**",
                "- `emit_to <stream>` — direct audit emission to a specific stream (e.g. `audit_log`).",
                "",
                "**Rules**",
                "- Valid on `command`, `query.*`, `job`, `webhook`, and `report` (and `api` via `policy` linkage).",
                "- Audit declarations do not replace `emits`; events and audits are different contracts.",
                "",
                "**Example**",
                "```lazuli",
                "command reassign",
                "  policy @policy.update",
                "  audit actor, target.id, input.owner_id",
                "    emit_to audit_log",
                "  updates Customer",
                "    owner = resolved_owner",
                "```",
                "",
                "See [invariants.md §Source And Derived Views](docs/invariants.md) (audit fields paragraph) and [quickref.md §Security Checklist](docs/quickref.md).",
            ]
            .join("\n"),
        ),
        "rate_limit" => Some(
            [
                "**`rate_limit`** — per-call throttle on a `command`, `api`, `agent.expose http`, or `auth password`. Distinct from `notification.throttle` (which keys on recipient/channel axes).",
                "",
                "**Grammar**",
                "- `rate_limit \"<N> per <window> per <axis>\"`",
                "- `<N>` — positive integer.",
                "- `<window>` — duration string (`second`, `minute`, `hour`, `day`, or `<N> <unit>` like `\"5 10 minutes\"` for explicit count).",
                "- `<axis>` — closed catalog: `ip`, `user`, `org`, `tenant`.",
                "- `rate_limit none` (with `reason \"...\"`) — explicit opt-out; required when the strict security profile demands a decision.",
                "",
                "**When required**",
                "- Commands that mutate state.",
                "- Commands / APIs whose effective policy includes `@scope.public`.",
                "- `auth password` flows.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  policy @policy.create",
                "  rate_limit \"30 per hour per ip\"",
                "  creates Customer",
                "```",
                "",
                "See [quickref.md §Security Checklist](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
            ]
            .join("\n"),
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
const KIND_CHILD_COMPLETIONS: &[(&str, &[&str])] = &[
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
        &["returns", "sql", "params", "scope", "policy", "cache", "audit"],
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
        ],
    ),
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
];

/// Closed catalog of effect verbs offered as completion when the
/// cursor is positioned where an effect would go inside a `command`.
/// `returns` is the non-mutating sibling shipped on commands like the
/// `smoke-hello` fixture's `greet` command.
const EFFECT_VERBS: &[&str] = &["creates", "updates", "deletes", "returns"];

/// Closed catalog of `rate_limit` axis tokens for the
/// `"<N> per <window> per <axis>"` grammar. Surfaced inside double
/// quotes after `per` so authors / LLMs see the closed set instead of
/// guessing tenant-style words.
const RATE_LIMIT_AXES: &[&str] = &["ip", "user", "org", "tenant"];

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
fn block_kind_at(source: &str, position: Position) -> Option<&'static str> {
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
            let mut kinds: Vec<&str> =
                KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
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
fn context_aware_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];

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

    // 2. Indent-aware kind child.
    // Only fire when the prefix is whitespace (cursor on a blank
    // indented line) or a partial child keyword. We don't want to
    // shadow `@cap.File(...)` value completion (handled earlier in
    // the dispatch chain) or general keyword completion mid-token in
    // an unrelated context.
    let trimmed_before = before.trim_start();
    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word =
        !trimmed_before.is_empty() && trimmed_before.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
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
fn namespace_prefix_completions(source: &str, before_cursor: &str) -> Option<Vec<CompletionItem>> {
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
fn collect_namespace_names(source: &str, ns: &str) -> Vec<String> {
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

/// Inside a `rate_limit "<N> per <window> per "` value, offer the
/// closed axis catalog. Returns `None` outside that context.
fn rate_limit_axis_completions(before_cursor: &str) -> Option<Vec<CompletionItem>> {
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
        || string_so_far.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_').ends_with(" per ");
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

const DESIGN_KEYWORDS: &[&str] = &[
    "design",
    "extends",
    "color",
    "typography",
    "space",
    "radius",
    "shadow",
    "motion",
    "breakpoint",
    "z",
    "family",
    "scale",
    "weight",
    "tracking",
    "duration",
    "easing",
    "size",
    "line_height",
    "base",
    "hover",
    "active",
    "foreground",
    "dark",
];

const KEYWORDS: &[&str] = &[
    "workspace",
    "app",
    "registry",
    "profile",
    "apps",
    "shared_registry",
    "boundaries",
    "gateway",
    "contract",
    "compatibility",
    "import",
    "operation",
    "env",
    "aggregate",
    "entity",
    "record",
    "command",
    "query.list",
    "query.lookup",
    "query.sql",
    "agent",
    "model",
    "prompt",
    "tools",
    "safety",
    "stream",
    "temperature",
    "max_tokens",
    "top_p",
    "seed",
    "notification",
    "channel",
    "recipient",
    "template",
    // Notifications expanded bucket cycle — `digest` / `throttle`
    // sub-blocks + their child keywords. Closed-catalog completion
    // for `template_strategy` lives below in
    // `NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES`.
    "digest",
    "throttle",
    "every",
    "group_by",
    "template_strategy",
    "max_per",
    "per_recipient",
    "per_channel",
    "burst",
    // Row 30 — storage bucket cycle `@cap.File(...)` argument keywords.
    "max_size",
    "accept",
    "visibility",
    "signed_ttl",
    // Observability bucket cycle row 36 — `app.logging` /
    // `app.tracing` slot keywords. Closed catalogs surface through
    // `keyword_hover` above and the closed-catalog completion below.
    "logging",
    "tracing",
    "level",
    "format",
    "redact",
    "sample_rate",
    "propagate",
    "exporter",
    // Observability bucket cycle row 37 — `audit emit_to` slot.
    "emit_to",
    // Migrations bucket cycle Route C — `tenant_migration` kind +
    // `deploy.{strategy, lock_timeout, pre_migration_hook,
    // post_migration_hook, checkpoint}` keywords. See
    // `docs/proposals/bucket-migrations-cycle.md`.
    "tenant_migration",
    "strategy",
    "lock_timeout",
    "pre_migration_hook",
    "post_migration_hook",
    "checkpoint",
    "defaults",
    "domain",
    "policies",
    "errors",
    "auth",
    "identity",
    "password",
    "oauth",
    "mfa",
    "sessions",
    "refresh",
    "enroll",
    "verify",
    "hash",
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
    "bindings",
    "targets",
    "environments",
    "urls",
    "group",
    "in",
    "integrations",
    "integration",
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
    "topology",
    "environment",
    "view",
    "audience",
    "extends",
    "input",
    "route",
    "previously",
    "migrated",
    "alias",
    "path",
    "params",
    "to",
    "let",
    "derived",
    "audit",
    "has_many",
    "inverse",
    "policy",
    "policy_for",
    "rate_limit",
    "calls",
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
    "external_calls",
    "payload",
    "payload_group",
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
    // Webhooks expanded cycle — payload/replay/dlq vocabulary.
    "webhook_events",
    "payload_from",
    "replay",
    "allow",
    "within",
    "dedupe",
    "dlq",
    "emit",
    "drop",
];

/// Phase L — closed-catalog tokens offered as completion values for
/// `auth` subblock slots. The slot context is not inferred today; the
/// LSP simply offers these as `VALUE` completions so authors and LLMs
/// see the canonical set alongside keyword completions.
///
/// - `algorithm` → `argon2id`, `bcrypt`
/// - `oauth <provider>` → `google`, `github`, `microsoft`, `apple`
/// - `mfa <method>` → `totp`
/// - `refresh` → `true`, `false`
pub const AUTH_CATALOG_VALUES: &[&str] = &[
    "argon2id",
    "bcrypt",
    "google",
    "github",
    "microsoft",
    "apple",
    "totp",
    "true",
    "false",
];

/// Hover/completion description for a closed-catalog value.
pub fn auth_catalog_detail(value: &str) -> Option<&'static str> {
    match value {
        "argon2id" => Some("Password hash algorithm — recommended for v0."),
        "bcrypt" => Some("Password hash algorithm — legacy migration only."),
        "google" => Some("OAuth provider — `google`."),
        "github" => Some("OAuth provider — `github`."),
        "microsoft" => Some("OAuth provider — `microsoft`."),
        "apple" => Some("OAuth provider — `apple`."),
        "totp" => Some("MFA method — Time-based One-Time Password."),
        "true" => Some("Boolean — `true`."),
        "false" => Some("Boolean — `false`."),
        _ => None,
    }
}

/// Observability bucket cycle row 36 — closed-catalog values offered
/// as `VALUE` completions for the new `app.logging` / `app.tracing`
/// slots. Same shape and dispatch as `AUTH_CATALOG_VALUES`.
///
/// - `level` → `debug`, `info`, `warn`, `error`
/// - `format` → `json`, `text`
/// - `redact` → `pii`, `none`
/// - `propagate` (tracing) → `true`, `false`
pub const OBSERVABILITY_CATALOG_VALUES: &[&str] = &[
    "debug", "info", "warn", "error", "json", "text", "pii", "none",
];

/// Hover/completion description for the observability closed-catalog
/// values. Mirrors `auth_catalog_detail` shape.
pub fn observability_catalog_detail(value: &str) -> Option<&'static str> {
    match value {
        "debug" => Some("Log level — verbose tracing for local development."),
        "info" => Some("Log level — production default."),
        "warn" => Some("Log level — recoverable errors and degraded conditions."),
        "error" => Some("Log level — request failures and exceptions."),
        "json" => Some("Log format — machine-parseable single-line JSON."),
        "text" => Some("Log format — human-readable for local development."),
        "pii" => Some("Redaction — auto-strip fields tagged with `@pii.*`."),
        "none" => Some("Redaction — disabled; adapter may still redact."),
        _ => None,
    }
}

/// Migrations bucket cycle Route C — closed `deploy.strategy` catalog.
/// Three rollout patterns the language fixes so doctor can pin a
/// finite ruleset (`DEPLOY-STRATEGY-001`).
pub const DEPLOY_STRATEGY_VALUES: &[&str] = &["rolling", "blue_green", "canary"];

/// i18n bucket cycle — closed `locale_negotiate.source` catalog.
/// Five request axes the runtime can read to populate `ctx.locale`.
/// Doctor `locale_negotiate_source_invalid` enforces this set.
pub const LOCALE_NEGOTIATE_SOURCE_VALUES: &[&str] = &[
    "accept_language",
    "query_param",
    "cookie",
    "user_profile",
    "subdomain",
];

/// i18n bucket cycle — closed `locale_negotiate.strategy` catalog.
/// Three matching algorithms. Doctor `locale_negotiate_strategy_invalid`
/// enforces this set.
pub const LOCALE_NEGOTIATE_STRATEGY_VALUES: &[&str] =
    &["best_match", "prefix_match", "exact_match"];

/// i18n bucket cycle — closed CLDR plural-arm catalog. Doctor
/// `cldr_plural_arm_invalid` enforces this set.
pub const CLDR_PLURAL_ARM_VALUES: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Notifications expanded bucket cycle — closed catalog for
/// `notification.digest.template_strategy`. Two strategies: `merge`
/// (last-write-wins per payload key) and `append` (emits a list the
/// digest template iterates over). Doctor surfaces unknown values
/// silently as `None` in IR; LSP completion narrows authoring.
pub const NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES: &[&str] = &["merge", "append"];

/// i18n bucket cycle — popular BCP-47 tags surfaced as soft completions.
/// The set is **not** closed (BCP-47 tags are open); these are
/// authoring hints only. Doctor never validates against this list.
pub const BCP47_POPULAR_TAGS: &[&str] = &[
    "en-US", "en-GB", "pt-BR", "pt-PT", "es-ES", "es-AR", "es-MX", "fr-FR", "de-DE", "it-IT",
    "ja-JP", "zh-CN", "zh-TW", "ko-KR",
];

/// Notifications expanded bucket cycle — hover/completion description
/// for `notification.digest.template_strategy` closed-catalog values.
pub fn notification_digest_template_strategy_detail(value: &str) -> Option<&'static str> {
    match value {
        "merge" => Some(
            "Merge — collapse per-trigger payloads into a single object (last-write-wins per key). Default when omitted.",
        ),
        "append" => {
            Some("Append — emit a list of per-trigger payloads the digest template iterates over.")
        }
        _ => None,
    }
}

/// Hover/completion description for the deploy.strategy closed-catalog
/// values. Mirrors `observability_catalog_detail` shape.
pub fn deploy_strategy_detail(value: &str) -> Option<&'static str> {
    match value {
        "rolling" => Some(
            "Rolling rollout — replace instances one window at a time. Lowest risk; longest cutover.",
        ),
        "blue_green" => Some(
            "Blue/green rollout — provision parallel stack, flip traffic atomically. Fast rollback; doubles infra during cutover.",
        ),
        "canary" => Some(
            "Canary rollout — shift traffic incrementally to a fresh cohort. Best for unproven changes; requires per-version observability.",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SecurityProfile, diagnostics_for, diagnostics_for_uri, diagnostics_for_with_profile,
        format_canonical_source,
    };
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

    #[test]
    fn canonical_order_accepts_feature_blocks_in_order() {
        let source = r#"
registry
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

        // The full-capsule feature file references env vars declared in the
        // sibling `registry.lzi`. The per-file LSP can't see registry, so it
        // emits an informational `env-schema-reference` warning that doctor
        // resolves cross-package. Filter it out for ordering tests.
        let filtered: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code.as_ref().and_then(|c| match c {
                    tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                    _ => None,
                }) != Some("env-schema-reference")
            })
            .cloned()
            .collect();

        assert!(
            filtered.is_empty(),
            "expected no canonical ordering diagnostics, got: {filtered:#?}"
        );
    }

    #[test]
    fn canonical_accepts_feature_integration_requirements() {
        let source = r#"
feature payments
  purpose "Payments"

  requires integration gateway: PaymentGateway

feature credit_check
  purpose "Credit checks"

  requires
    integration bureau: CreditBureau
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_feature_requirements() {
        let source = r#"
feature payments
  purpose "Payments"

  requires
    provider mercadopago: PaymentGateway
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "feature requirements currently use `integration <name>: <CapabilityType>`",
            )
        }));
    }

    #[test]
    fn canonical_accepts_external_calls_through_required_integration_slot() {
        let source = r#"
feature imports
  requires integration crm: CRMProvider

  job process_import
    trigger event import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected no external call diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_external_calls_without_contract_guards() {
        let source = r#"
feature imports
  job process_import
    trigger event import_uploaded
    calls crm.normalize_import_batch
      payload.batch_id
    handler "./jobs/process_import.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("requires integration"))
        );
        assert!(messages.iter().any(|message| message.contains("timeout")));
        assert!(messages.iter().any(|message| message.contains("retry")));
        assert!(
            messages
                .iter()
                .any(|message| message.contains("idempotency"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("argument bindings"))
        );
    }

    #[test]
    fn canonical_warns_for_invalid_app_bindings() {
        let source = r#"
app AcmeCRM
  uses
    payments

  bindings
    payments.gateway -> mercadopago
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("app bindings use `<feature>.<slot> = integrations.<name>`")
        }));
    }

    #[test]
    fn canonical_accepts_app_profiles() {
        let source = r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    customer_import.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
    migrations before_deploy
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected profile contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_app_profiles() {
        let source = r#"
profile 123
  urls
    web http://localhost:3000
  bindings
    customer_import.crm -> integrations.crm
  integrations
    crm sandbox
  deploy
    topology "split"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile headers"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("URL overrides"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile bindings"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("integration overrides"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile deploy"))
        );
    }

    #[test]
    fn canonical_accepts_app_and_registry_pack_contracts() {
        let source = r#"
app AcmeCRM
  uses
    payments
  packs
    payments from registry.packs.payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck

registry
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
    serasa: CreditBureau
      adapter @plugin/acme/serasa
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected app/registry pack contracts to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_pack_contracts() {
        let source = r#"
app AcmeCRM
  packs
    payments -> registry.packs.payments

registry
  packs
    payments @runtime/payments
      provides
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("app pack entries use `<alias> from registry.packs.<name>`")
        }));
        assert!(
            messages
                .iter()
                .any(|message| message.contains("registry packs use `<name> from"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("pack children use"))
        );
    }

    #[test]
    fn canonical_warns_for_unknown_adapter_provenance() {
        let source = r#"
registry
  integrations
    crm: CRMProvider
      adapter @unknown.crm

profile local
  integrations
    crm adapter @unknown.fake
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("adapter @runtime/") || message.contains("adapter <source>")
        }));
    }

    #[test]
    fn canonical_accepts_workspace_contract() {
        let source = r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
  shared_registry "./registry.lzi"
  boundaries
    crm publishes customer.*
    ai consumes customer.*
  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus
  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
      timeout "5s"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected workspace contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_workspace_contract() {
        let source = r#"
workspace 123
  apps
    crm "./apps/crm/app.lzi"
  shared_registry ./registry.lzi
  boundaries
    crm listens customer.*
  communication
    default sync grpc
  gateway 123
    route "/api/customers/*" to app crm
      auth inherit
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace contracts use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace apps use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("shared registries use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace boundaries use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace communication uses"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace gateways use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("gateway route children use"))
        );
    }

    #[test]
    fn canonical_accepts_external_contract() {
        let source = r#"
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  record CustomerSummaryResult
    summary: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected external contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_external_contract() {
        let source = r#"
contract 123
  compatibility future
  import swagger ./ai.yaml
  record request
    customer_id ID required
  operation summarize
    transport grpc
    method FETCH
    path /v1/summary
  event summary_ready
    topic ai.summary_ready
    payload
      customer_id: ID
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("external contracts use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract compatibility"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract imports use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract records use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract fields use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("operation children use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract event topics"))
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
                "full-capsule-registry.lzi",
                include_str!("../../../examples/full-capsule/registry.lzi"),
            ),
            (
                "full-capsule-profiles.lzi",
                include_str!("../../../examples/full-capsule/profiles.lzi"),
            ),
            (
                "full-capsule-workspace.lzi",
                include_str!("../../../examples/full-capsule/workspace.lzi"),
            ),
            (
                "full-capsule-contract-ai.lzi",
                include_str!("../../../examples/full-capsule/contracts/ai.lzi"),
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
            // `env-schema-reference` is a per-file warning that doctor
            // cross-checks across the package registry. The LSP can't see
            // sibling files, so feature sources that reference env vars
            // declared in `registry.lzi` legitimately surface this warning.
            // Filter it out for the per-file canonical contract.
            let filtered: Vec<_> = diagnostics
                .iter()
                .filter(|d| {
                    d.code.as_ref().and_then(|c| match c {
                        tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                        _ => None,
                    }) != Some("env-schema-reference")
                })
                .cloned()
                .collect();
            assert!(
                filtered.is_empty(),
                "expected {name} to satisfy canonical LSP diagnostics, got: {filtered:#?}"
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

  bindings
    customer.gateway = integrations.crm

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
  to customer.view.detail(id: route.id)
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
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn derived_field_accepts_expression() {
        let source = r#"
feature customer
  domain
    resource Customer
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
"#;
        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn derived_field_rejects_default_or_requiredness() {
        let bad_default = r#"
feature customer
  domain
    resource Customer
      tier: Text derived from compute_tier(score) = "bronze"
"#;
        let diagnostics = diagnostics_for(bad_default);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must not declare `default`"))
        );

        let bad_required = r#"
feature customer
  domain
    resource Customer
      tier: Text required derived from compute_tier(score)
"#;
        let diagnostics = diagnostics_for(bad_required);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must not declare `required`"))
        );
    }

    #[test]
    fn derived_field_requires_expression() {
        let source = r#"
feature customer
  domain
    resource Customer
      tier: Text derived from
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("requires an expression"))
        );
    }

    #[test]
    fn has_many_accepts_canonical_collections() {
        let source = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote inverse customer
      has_many tags: CustomerTag
"#;
        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn has_many_rejects_unexpected_tail_or_missing_inverse_field() {
        let bad_tail = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote required
"#;
        assert!(
            diagnostics_for(bad_tail)
                .iter()
                .any(|d| d.message.contains("Only `inverse <field>` is allowed"))
        );

        let bad_inverse = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote inverse
"#;
        assert!(
            diagnostics_for(bad_inverse)
                .iter()
                .any(|d| d.message.contains("`inverse` requires a field name"))
        );
    }

    #[test]
    fn agent_accepts_canonical_declaration() {
        let source = r#"
feature customer
  policies
    read: @scope.same_org

  agent summarize_customer
    input
      customer_id: ID required
    context customer.query.by_id(id: input.customer_id)
    policy @policy.read
    rate_limit "20 per hour per user"
    output stream Text
    model @llm.default
    prompt "./prompts/summarize_customer.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got: {:#?}",
            diagnostics
                .iter()
                .map(|d| (d.message.clone(), d.code.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn agent_rejects_missing_required_children() {
        let source = r#"
feature customer
  agent summarize_customer
    input
      prompt: Text required
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`policy @policy.<name>`"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`output [stream] <Type>`"))
        );
        assert!(messages.iter().any(|m| m.contains("`model @llm.<name>`")));
        assert!(messages.iter().any(|m| m.contains("prompt")));
    }

    #[test]
    fn agent_rejects_non_llm_model_reference() {
        let source = r#"
feature customer
  agent summarize_customer
    input
      prompt: Text required
    policy @policy.read
    output stream Text
    model gpt-4
    prompt "./prompts/summarize_customer.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must be a `@llm.<name>` reference"))
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — file-local diagnostics (§6.2 snapshot tests)
    // -------------------------------------------------------------------------

    fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics
            .iter()
            .filter_map(|d| match d.code.as_ref()? {
                tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.clone()),
                tower_lsp::lsp_types::NumberOrString::Number(n) => Some(n.to_string()),
            })
            .collect()
    }

    #[test]
    fn agent_tools_accepts_canonical_block() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.query.lookup.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_tools_diagnostics"),
            "canonical tool block should not produce agent_tools_diagnostics; got: {codes:?}"
        );
    }

    #[test]
    fn agent_tools_rejects_unknown_kind_segment() {
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.script.run_unsafe
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            codes.iter().any(|c| c == "agent_tools_diagnostics"),
            "expected agent_tools_diagnostics for unknown kind; got: {codes:?}"
        );
    }

    #[test]
    fn agent_tools_rejects_empty_segment() {
        // `customer..by_id` has an empty segment — must be rejected.
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer..by_id
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_tools_diagnostics"),
            "expected agent_tools_diagnostics for empty segment"
        );
    }

    #[test]
    fn agent_evals_accepts_case_with_requires_forbids() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case redacts_email
        requires customer.email = "ada@example.com"
        forbids output contains @semantic.Email
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_evals_diagnostics"),
            "canonical evals block should not produce agent_evals_diagnostics; got: {codes:?}"
        );
        assert!(
            !codes.iter().any(|c| c == "eval_nondeterministic_warning"),
            "agent pinned at temperature 0 + seed 1 must not warn nondeterministic; got: {codes:?}"
        );
    }

    #[test]
    fn agent_evals_rejects_given_expect_legacy_vocabulary() {
        let source = r#"
feature customer
  agent legacy
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      given a_case
        expect output contains "ok"
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("`given` is legacy")),
            "expected `given` legacy diagnostic; got: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("`expect` is legacy")),
            "expected `expect` legacy diagnostic; got: {messages:?}"
        );
    }

    #[test]
    fn agent_discriminator_rejects_when_marker_outside_record() {
        // Field `tag: Status discriminator` declared inside `agent
        // input` instead of a record — must be rejected.
        let source = r#"
feature customer
  agent classify
    input
      message: Text required
      tag: Status discriminator
    policy @policy.read
    output discriminator Intent
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_discriminator_diagnostics"),
            "expected agent_discriminator_diagnostics when marker appears outside record"
        );
    }

    #[test]
    fn agent_evals_warns_without_temperature_zero_seed() {
        // Agent has an evals block but `temperature 0.7` (non-zero) and
        // no `seed` — must emit `eval_nondeterministic_warning`.
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "eval_nondeterministic_warning"),
            "expected eval_nondeterministic_warning"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.7 — `expose http` file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn agent_expose_local_path_conflict_caught() {
        // Two agents in the same file declare the same (method, path).
        let source = r#"
feature customer
  agent first
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:id"
      route id: ID

  agent second
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./q.md"
    expose http
      method POST
      path "/api/x/:other"
      route other: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_path_conflict_local_diagnostics"),
            "expected local path conflict; got: {:?}",
            diagnostic_codes(&diagnostics)
        );
    }

    #[test]
    fn agent_expose_slot_unbound_caught() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_slot_unbound_diagnostics"),
            "expected slot_unbound"
        );
    }

    #[test]
    fn agent_expose_slot_must_use_route_caught_with_input_slot_collision() {
        let source = r#"
feature customer
  agent broken
    input
      customer_id: ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            codes
                .iter()
                .any(|c| c == "agent_expose_slot_must_use_route_diagnostics"),
            "expected slot_must_use_route; got: {codes:?}"
        );
    }

    #[test]
    fn agent_expose_method_get_streaming_warns() {
        let source = r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method GET
      path "/api/customers/:id/summary"
      route id: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_method_streaming_mismatch_warning"),
            "expected method/streaming warning"
        );
    }

    #[test]
    fn agent_expose_well_formed_emits_nothing() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        for code in [
            "agent_expose_path_conflict_local_diagnostics",
            "agent_expose_slot_unbound_diagnostics",
            "agent_expose_slot_must_use_route_diagnostics",
            "agent_expose_method_streaming_mismatch_warning",
        ] {
            assert!(
                !codes.iter().any(|c| c == code),
                "well-formed expose should not produce {code}; got: {codes:?}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Cut A.11 — `cors` block file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn cors_rejects_unknown_child() {
        let source = r#"
app MyApp
  cors
    allow_methods GET, POST
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "expected cors_contract_diagnostics for unknown child `allow_methods`"
        );
    }

    #[test]
    fn cors_rejects_allow_origins_without_origins() {
        let source = r#"
app MyApp
  cors
    allow_origins production
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "expected cors_contract_diagnostics for missing origins"
        );
    }

    #[test]
    fn cors_rejects_invalid_allow_credentials() {
        let source = r#"
app MyApp
  cors
    allow_credentials yes
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("allow_credentials yes")),
            "expected diagnostic about invalid allow_credentials value; got {messages:?}"
        );
    }

    #[test]
    fn cors_well_formed_emits_nothing() {
        let source = r#"
app MyApp
  cors
    allow_origins production "https://app.example.com", "https://*.example.com"
    allow_origins local "*"
    allow_credentials true
    max_age "1h"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "well-formed cors must not produce cors_contract_diagnostics"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.9 — `approval` file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn approval_rejects_missing_required_children() {
        let source = r#"
feature customer
  command archive
    approval
      by @role.admin
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "approval_contract_diagnostics"),
            "expected approval_contract_diagnostics for missing timeout/then"
        );
    }

    #[test]
    fn approval_rejects_unknown_then_action() {
        let source = r#"
feature customer
  command archive
    approval
      by @role.admin
      timeout "24h"
      then escalate
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`approval then escalate`")),
            "expected diagnostic about invalid then value; got: {messages:?}"
        );
    }

    #[test]
    fn approval_well_formed_emits_nothing() {
        let source = r#"
feature customer
  command archive
    approval
      required_when target.tier = enterprise
      by @role.admin
      timeout "24h"
      then deny
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "approval_contract_diagnostics"),
            "well-formed approval must not produce approval_contract_diagnostics"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.8 — reserved trace event name (LSP file-local)
    // -------------------------------------------------------------------------

    #[test]
    fn event_trace_agent_run_authored_is_rejected() {
        let source = r#"
feature customer
  domain
    event.trace agent_run
      payload
        agent_id: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "event_trace_reserved_name_diagnostics"),
            "expected reserved-name diagnostic"
        );
    }

    #[test]
    fn event_trace_custom_name_is_allowed() {
        let source = r#"
feature customer
  domain
    event.trace custom_metric
      payload
        value: Integer
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "event_trace_reserved_name_diagnostics"),
            "non-reserved trace events must be allowed"
        );
    }

    #[test]
    fn agent_discriminator_allows_marker_inside_record() {
        // Sanity gate: `discriminator` on a record field is the
        // canonical use; must not fire the file-local diagnostic.
        let source = r#"
feature customer
  domain
    record Action
      kind: ActionKind discriminator
      customer_id: Customer.ID optional
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_discriminator_diagnostics"),
            "canonical record-field marker must not produce agent_discriminator_diagnostics; got: {codes:?}"
        );
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
            message.contains("validators are referenced through `validates @validator.<name>`")
        }));
    }

    #[test]
    fn canonical_warns_for_redundant_validates_scope_keyword() {
        let scoped_field = r#"
feature customer
  domain
    resource Customer
      tier: Text required
      validates field tier @validator.tier_check
"#;
        assert!(
            diagnostics_for(scoped_field)
                .iter()
                .any(|d| d.message.contains("drop the `field <name>` prefix"))
        );

        let scoped_resource = r#"
feature customer
  domain
    resource Customer
      tier: Text required
      validates resource @validator.row_check
"#;
        assert!(
            diagnostics_for(scoped_resource)
                .iter()
                .any(|d| d.message.contains("drop the `resource` prefix"))
        );
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
    fn canonical_warns_for_unknown_query_mode() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required

  query.fancy something
"#;

        assert!(
            diagnostics_for(source)
                .iter()
                .any(|d| d.message.contains("unknown query mode"))
        );
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
registry
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
registry
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

    // ----------------------------------------------------------------
    // Row 30 — Storage bucket cycle: hovers + closed-catalog completions
    // for `@cap.File(...)` argument keywords.
    // ----------------------------------------------------------------

    use super::{
        DESIGN_KEYWORDS, KEYWORDS, cap_file_value_completions, completion_items_for_uri,
        design_keyword_description, keyword_description,
    };
    use tower_lsp::lsp_types::Position;

    #[test]
    fn design_lzi_completion_surfaces_token_groups() {
        let uri = Url::parse("file:///workspace/design.lzi").unwrap();
        let items = completion_items_for_uri(&uri);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

        for group in [
            "color",
            "typography",
            "space",
            "radius",
            "shadow",
            "motion",
            "breakpoint",
            "z",
        ] {
            assert!(
                labels.contains(&group),
                "`design.lzi` completions should include `{group}`"
            );
        }
    }

    #[test]
    fn feature_lzi_does_not_surface_design_keywords() {
        let uri = Url::parse("file:///workspace/features/customer/customer.lzi").unwrap();
        let items = completion_items_for_uri(&uri);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

        for design_only in [
            "color",
            "typography",
            "space",
            "radius",
            "shadow",
            "motion",
            "breakpoint",
            "z",
        ] {
            assert!(
                !labels.contains(&design_only),
                "feature `.lzi` completions should not include design keyword `{design_only}`"
            );
        }
    }

    #[test]
    fn design_keyword_hovers_link_to_proposal() {
        for kw in DESIGN_KEYWORDS {
            let description = design_keyword_description(kw)
                .unwrap_or_else(|| panic!("hover for `{kw}` missing"));
            assert!(description.contains("docs/proposals/design-tokens.md"));
        }
    }

    #[test]
    fn keyword_hover_describes_cap_file_arguments() {
        for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
            let description =
                keyword_description(kw).unwrap_or_else(|| panic!("hover for `{kw}` missing"));
            assert!(
                !description.is_empty(),
                "hover for `{kw}` must be non-empty"
            );
        }
    }

    // Encryption bucket cycle — hover catalog for the `encryption`
    // block. `key`, `source`, `algorithm` are already in the catalog
    // (claimed by sibling bucket cycles); only `encryption` and
    // `rotation` are new tokens. See
    // `docs/proposals/encryption-vocab.md` §LSP hovers.
    #[test]
    fn keyword_hover_describes_encryption_block() {
        let description =
            keyword_description("encryption").expect("encryption hover present");
        assert!(description.contains("@key."));
        assert!(description.contains("@cap.Encrypted"));
    }

    #[test]
    fn keyword_hover_describes_rotation_strategy() {
        let description =
            keyword_description("rotation").expect("rotation hover present");
        assert!(description.contains("manual"));
    }

    #[test]
    fn keyword_hover_describes_cap_file_decorator() {
        for kw in ["@cap.File", "cap.File"] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_visibility_lists_closed_catalog() {
        let description = keyword_description("visibility").unwrap();
        assert!(description.contains("public"));
        assert!(description.contains("private"));
        assert!(description.contains("signed"));
    }

    #[test]
    fn keywords_list_contains_storage_arguments() {
        for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
            assert!(
                KEYWORDS.contains(&kw),
                "`KEYWORDS` should list `{kw}` so completions surface it"
            );
        }
    }

    #[test]
    fn cap_file_value_completion_for_visibility_offers_closed_catalog() {
        let source = "    output @cap.File(max_size:10mb,accept:text/csv,visibility:";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("visibility offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["public", "private", "signed"]);
    }

    #[test]
    fn cap_file_value_completion_for_max_size_offers_units() {
        let source = "    file: @cap.File(max_size:25";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("max_size offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["kb", "mb", "gb"]);
    }

    #[test]
    fn cap_file_value_completion_for_signed_ttl_offers_units() {
        let source =
            "    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("signed_ttl offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["s", "m", "h", "d"]);
    }

    #[test]
    fn cap_file_value_completion_for_accept_offers_mime_families() {
        let source = "    output @cap.File(max_size:10mb,accept:";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("accept offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "text",
                "image",
                "application",
                "audio",
                "video",
                "font",
                "*"
            ]
        );
    }

    #[test]
    fn cap_file_value_completion_returns_none_outside_capability() {
        let source = "    file: Text";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        assert!(cap_file_value_completions(source, position).is_none());
    }

    // ----------------------------------------------------------------
    // Notifications expanded bucket cycle — hovers + closed-catalog
    // completions for `notification.digest` / `notification.throttle`.
    // ----------------------------------------------------------------

    #[test]
    fn keyword_hover_describes_notification_digest_children() {
        for kw in [
            "digest",
            "every",
            "group_by",
            "max_size",
            "template_strategy",
        ] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_describes_notification_throttle_children() {
        for kw in [
            "throttle",
            "max_per",
            "per_recipient",
            "per_channel",
            "burst",
        ] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_throttle_distinguishes_from_rate_limit() {
        let throttle = keyword_description("throttle").unwrap();
        assert!(
            throttle.contains("per-recipient") || throttle.contains("Distinct from"),
            "throttle hover must call out the distinction from scalar rate_limit; got `{throttle}`"
        );
    }

    #[test]
    fn keywords_list_contains_notification_subblocks() {
        for kw in [
            "digest",
            "throttle",
            "every",
            "group_by",
            "max_size",
            "template_strategy",
            "max_per",
            "per_recipient",
            "per_channel",
            "burst",
        ] {
            assert!(
                KEYWORDS.contains(&kw),
                "`KEYWORDS` should list `{kw}` so completions surface it"
            );
        }
    }

    #[test]
    fn notification_digest_template_strategy_catalog_has_two_entries() {
        use super::NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES;
        assert_eq!(NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES.len(), 2);
        for value in NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES {
            assert!(
                super::notification_digest_template_strategy_detail(value).is_some(),
                "detail for `{value}` must be available"
            );
        }
    }

    // ----------------------------------------------------------------
    // Wave B — LSP hover + completion coverage for
    // `command`/`query.list`/`query.lookup`/`query.sql`/`api`/`policy`/
    // `effect`/`audit`/`rate_limit`. Each kind gets one hover
    // assertion and one completion assertion so the closed catalogs
    // surface to editors instead of being shape-only strings.
    // ----------------------------------------------------------------

    use super::{
        EFFECT_VERBS, KIND_CHILD_COMPLETIONS, RATE_LIMIT_AXES, block_kind_at,
        context_aware_completions, rich_keyword_hover,
    };
    use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

    /// Helper: assert the rich Markdown hover for `keyword` exists and
    /// contains every snippet in `expected_fragments`. Fragments
    /// double as a smoke test that required-children / optional-
    /// children / example / doc anchor all land in the output.
    fn assert_rich_hover_contains(keyword: &str, expected_fragments: &[&str]) {
        let rendered = rich_keyword_hover(keyword)
            .unwrap_or_else(|| panic!("rich hover for `{keyword}` must be present"));
        for fragment in expected_fragments {
            assert!(
                rendered.contains(fragment),
                "rich hover for `{keyword}` must contain `{fragment}`; got:\n{rendered}"
            );
        }
    }

    #[test]
    fn rich_hover_for_command_describes_required_and_optional_children() {
        assert_rich_hover_contains(
            "command",
            &[
                "**`command`**",
                "**Required children**",
                "policy @policy.",
                "creates",
                "**Optional children**",
                "rate_limit",
                "audit",
                "emits",
                "invalidates",
                "**Example**",
                "```lazuli",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_list_calls_out_default_order_and_paginate() {
        assert_rich_hover_contains(
            "query.list",
            &[
                "**`query.list`**",
                "order created_at desc",
                "paginate",
                "search",
                "cache",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_lookup_documents_single_key_and_composite_forms() {
        assert_rich_hover_contains(
            "query.lookup",
            &[
                "**`query.lookup`**",
                "by <field>: <Type>",
                "params",
                "key",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_sql_requires_returns_and_sql_path() {
        assert_rich_hover_contains(
            "query.sql",
            &[
                "**`query.sql`**",
                "**Required children**",
                "returns",
                "sql \"./queries",
                "record",
                "**Example**",
                "docs/invariants.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_api_lists_method_path_output_policy_handler() {
        assert_rich_hover_contains(
            "api",
            &[
                "**`api`**",
                "method <GET|POST|PUT|PATCH|DELETE>",
                "path \"<url>\"",
                "output",
                "policy @policy.",
                "handler",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_policy_documents_forms_and_predicate_combinators() {
        assert_rich_hover_contains(
            "policy",
            &[
                "**`policy`**",
                "@policy.<name>",
                "@role.",
                "@scope.",
                "@actor.",
                "policies",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_effect_lists_closed_catalog_of_four_verbs() {
        assert_rich_hover_contains(
            "effect",
            &[
                "**`effect`**",
                "creates",
                "updates",
                "deletes",
                "returns",
                "One mutating effect per command",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_audit_lists_three_forms() {
        assert_rich_hover_contains(
            "audit",
            &[
                "**`audit`**",
                "`audit`",
                "audit <field>",
                "audit none",
                "emit_to",
                "**Example**",
                "docs/invariants.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_rate_limit_documents_grammar_and_axes() {
        assert_rich_hover_contains(
            "rate_limit",
            &[
                "**`rate_limit`**",
                "<N> per <window> per <axis>",
                "ip",
                "user",
                "org",
                "tenant",
                "rate_limit none",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_returns_none_for_unrelated_keywords() {
        // `domain` is a plain keyword that keeps its brief one-line
        // description; rich hover should not invent Markdown for it.
        assert!(
            rich_keyword_hover("domain").is_none(),
            "rich hover must stay scoped to LSP-extended kinds; `domain` should fall back to keyword_description"
        );
    }

    /// Helper: drive `context_aware_completions` and unwrap the
    /// returned items. Panics with a helpful message when the
    /// completion context isn't recognised so test failures point at
    /// the unrecognised path immediately.
    fn completions_at(source: &str, line: u32, character: u32) -> Vec<CompletionItem> {
        context_aware_completions(
            source,
            Position {
                line,
                character,
            },
        )
        .unwrap_or_else(|| panic!("expected context-aware completion at line {line}:{character}"))
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn completion_inside_command_offers_effect_verbs_and_children() {
        let source = "feature customer\n  command create\n    policy @policy.create\n    \n";
        // Line 3 (0-indexed) is the indented blank line; cursor at
        // character 4 sits inside the indent.
        let items = completions_at(source, 3, 4);
        let labels = labels(&items);
        for child in [
            "creates",
            "updates",
            "deletes",
            "returns",
            "policy",
            "rate_limit",
            "audit",
            "emits",
            "invalidates",
            "input",
        ] {
            assert!(
                labels.contains(&child),
                "command completion must offer `{child}`; got {labels:?}"
            );
        }
        // Effect verbs lead the list inside `command`.
        assert_eq!(labels[..4], ["creates", "deletes", "returns", "updates"]);
    }

    #[test]
    fn completion_inside_query_list_offers_closed_catalog_children() {
        let source = "feature customer\n  query.list list\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in [
            "params",
            "filters",
            "search",
            "order",
            "paginate",
            "cache",
            "policy",
            "modifier",
            "scope",
        ] {
            assert!(
                labels.contains(&child),
                "query.list completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_query_lookup_offers_params_and_key() {
        let source = "feature customer\n  query.lookup by_id\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in ["params", "key", "policy", "cache", "scope"] {
            assert!(
                labels.contains(&child),
                "query.lookup completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_query_sql_offers_returns_sql_params() {
        let source = "feature customer\n  query.sql lifetime_value\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in ["returns", "sql", "params", "scope", "policy"] {
            assert!(
                labels.contains(&child),
                "query.sql completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_api_offers_method_path_output_policy_handler() {
        let source = "feature hello\n  api greet\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in [
            "method",
            "path",
            "output",
            "policy",
            "handler",
            "rate_limit",
            "input",
            "audit",
            "route",
        ] {
            assert!(
                labels.contains(&child),
                "api completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_after_policy_namespace_offers_declared_categories() {
        let source = "feature customer\n  policies\n    create: @role.admin\n    read: @scope.same_org\n    update: @role.admin\n\n  command create\n    policy @policy.\n";
        // Cursor sits immediately after `@policy.` on line 7
        // (0-indexed). Compute the character position.
        let line = "    policy @policy.";
        let items = completions_at(source, 7, line.len() as u32);
        let mut labels = labels(&items);
        labels.sort();
        assert_eq!(labels, vec!["create", "read", "update"]);
    }

    #[test]
    fn completion_after_validator_namespace_offers_declared_extensions() {
        let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    validate @validator.\n";
        let line = "    validate @validator.";
        let items = completions_at(source, 7, line.len() as u32);
        let labels = labels(&items);
        assert_eq!(labels, vec!["verify_totp"]);
    }

    #[test]
    fn completion_after_fn_namespace_offers_declared_fns() {
        let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    let v = @fn.\n";
        let line = "    let v = @fn.";
        let items = completions_at(source, 7, line.len() as u32);
        let labels = labels(&items);
        assert_eq!(labels, vec!["lifetime_value"]);
    }

    #[test]
    fn completion_for_rate_limit_axis_offers_closed_catalog() {
        let source = "feature customer\n  command create\n    rate_limit \"30 per hour per ";
        // Cursor sits inside the open string after `per `.
        let line_text = "    rate_limit \"30 per hour per ";
        let items = completions_at(source, 2, line_text.len() as u32);
        let mut labels = labels(&items);
        labels.sort();
        let mut expected: Vec<&str> = RATE_LIMIT_AXES.to_vec();
        expected.sort();
        assert_eq!(labels, expected);
        // Each item carries an `ENUM_MEMBER` kind so VS Code and
        // Helix render the closed set as values, not keywords.
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
        }
    }

    #[test]
    fn completion_falls_back_outside_known_blocks() {
        // Top-level cursor — not inside command/query/api/agent —
        // returns None so the global keyword list still surfaces.
        let source = "feature customer\n  \n";
        let result = context_aware_completions(
            source,
            Position {
                line: 1,
                character: 2,
            },
        );
        assert!(
            result.is_none(),
            "top-level / unknown context must fall back; got {result:?}"
        );
    }

    #[test]
    fn block_kind_detection_handles_nested_indent() {
        // A `command` block at indent 2 with a child line at indent
        // 4 — block_kind_at must walk back to the header.
        let source = "feature customer\n  command create\n    policy @policy.create\n    ";
        let kind = block_kind_at(
            source,
            Position {
                line: 3,
                character: 4,
            },
        );
        assert_eq!(kind, Some("command"));
    }

    #[test]
    fn block_kind_detection_distinguishes_query_kinds() {
        for (block_header, expected) in [
            ("query.list list", "query.list"),
            ("query.lookup by_id by id: ID", "query.lookup"),
            ("query.sql lifetime_value", "query.sql"),
            ("api greet", "api"),
            ("agent summarize", "agent"),
            ("command create", "command"),
        ] {
            let source = format!("feature x\n  {block_header}\n    ");
            let kind = block_kind_at(
                &source,
                Position {
                    line: 2,
                    character: 4,
                },
            );
            assert_eq!(
                kind,
                Some(expected),
                "header `{block_header}` should resolve to `{expected}` kind"
            );
        }
    }

    #[test]
    fn kind_child_completions_cover_seven_target_kinds() {
        let kinds: Vec<&str> = KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
        for required in ["command", "query.list", "query.lookup", "query.sql", "api"] {
            assert!(
                kinds.contains(&required),
                "kind catalog must include `{required}`; got {kinds:?}"
            );
        }
    }

    #[test]
    fn effect_verbs_catalog_is_the_canonical_four() {
        let mut verbs = EFFECT_VERBS.to_vec();
        verbs.sort();
        assert_eq!(verbs, vec!["creates", "deletes", "returns", "updates"]);
    }
}
