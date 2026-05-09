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
        let diagnostics = diagnostics_for(&source);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    if is_canonical_source(source) {
        let mut diagnostics = canonical_order_diagnostics(source);
        diagnostics.extend(query_mode_diagnostics(source));
        diagnostics.extend(lookup_shorthand_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(policy_namespace_diagnostics(source));
        diagnostics.extend(type_namespace_diagnostics(source));
        diagnostics.extend(extension_declaration_diagnostics(source));
        diagnostics.extend(event_payload_reference_diagnostics(source));
        diagnostics.extend(event_kind_diagnostics(source));
        diagnostics.extend(event_locator_diagnostics(source));
        diagnostics.extend(rule_self_diagnostics(source));
        diagnostics.extend(anchor_whitelist_diagnostics(source));
        diagnostics.extend(test_block_diagnostics(source));
        diagnostics.extend(command_contract_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        diagnostics.extend(idempotency_key_diagnostics(source));
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
    source
        .lines()
        .any(|line| line.trim_start().starts_with("feature "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalBlockKind {
    Meta,
    Defaults,
    Uses,
    Domain,
    Policies,
    Auth,
    Command,
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
            Self::Domain => 3,
            Self::Policies => 4,
            Self::Auth => 5,
            Self::Command => 6,
            Self::Workflow => 7,
            Self::Job => 8,
            Self::Webhook => 9,
            Self::Surface => 10,
            Self::Extensions => 11,
            Self::EscapeRoute => 12,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Defaults => "defaults",
            Self::Uses => "uses",
            Self::Domain => "domain",
            Self::Policies => "policies",
            Self::Auth => "auth",
            Self::Command => "command",
            Self::Workflow => "workflow",
            Self::Job => "job",
            Self::Webhook => "webhook",
            Self::Surface => "surface",
            Self::Extensions => "extensions",
            Self::EscapeRoute => "escape_route",
        }
    }
}

const CANONICAL_FEATURE_ORDER: &str = "meta -> defaults -> uses -> domain -> policies -> auth -> command -> workflow -> job -> webhook -> surface -> extensions -> escape_route";

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
        "domain" => Some(CanonicalBlockKind::Domain),
        "policies" => Some(CanonicalBlockKind::Policies),
        "auth" => Some(CanonicalBlockKind::Auth),
        "command" => Some(CanonicalBlockKind::Command),
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

        if first == "query" {
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
                    "unknown `@...` namespace. Allowed namespaces are `@role`, `@scope`, `@actor`, `@semantic`, `@cap`, `@fn`, `@hook`, `@validator`, `@adapter`, `@client`, `@query_modifier`, and `@anchor`.",
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
            | "semantic"
            | "cap"
            | "fn"
            | "hook"
            | "validator"
            | "adapter"
            | "client"
            | "query_modifier"
            | "anchor"
    )
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

        if is_namespaced_atom(policy_ref) || policy_ref.contains('.') {
            continue;
        }

        let is_local_category = current_feature
            .as_ref()
            .and_then(|feature| policy_categories.get(feature))
            .is_some_and(|categories| categories.contains(policy_ref));

        if !is_local_category {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "direct policy atoms should be namespaced, e.g. `policy @actor.system` or `policy @role.admin`. Keep bare names for feature-local policy categories such as `policy create`.",
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

fn type_namespace_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
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
                "capability types should use the `@cap.*` namespace, e.g. `@cap.File` or `@cap.Secret`.",
            ));
        }
    }

    diagnostics
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

fn legacy_rule_subject_alias(predicate: &str) -> Option<&str> {
    let first = predicate.split_whitespace().next()?;
    let (head, _) = first.split_once('.')?;

    if matches!(
        head,
        "self" | "ctx" | "params" | "payload" | "envelope" | "route" | "input"
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
            trimmed_line.starts_with("allow @")
                || trimmed_line.starts_with("deny @")
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
    if parts.next()? != "events" {
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
    let (name, _) = trimmed_line.split_once(':')?;
    let name = name.trim();

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
            "event payload references `{field_name}`, but resource `{resource_name}` has no field named `{field_name}`. Shared event payload expressions resolve against the `events ... on {resource_name}` resource."
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
        "app" => Some("Names the generated application."),
        "aggregate" | "entity" => Some("Declares a domain resource with fields and behavior."),
        "command" => Some("Declares a write operation for an aggregate."),
        "query" => Some("Declares a read operation for an aggregate."),
        "query.list" => Some("Declares a generated collection query."),
        "query.lookup" => Some("Declares a generated single-record lookup query."),
        "query.sql" => Some("Declares a query backed by an external SQL file."),
        "event.trace" => {
            Some("Declares an observability-only event that is outside the feature reaction graph.")
        }
        "surface" => Some("Declares UI projections for list, form, and detail views."),
        "input" => Some("Lists fields accepted by a command."),
        "route" => Some("Declares route or context values accepted by a command."),
        "let" => Some("Binds a derived value for later command, job, or event expressions."),
        "policy" => Some("Associates a command with an authorization policy capability."),
        "requires" => {
            Some("Declares an additional authority requirement for a workflow transition.")
        }
        "modifier" => Some("Attaches a query modifier extension to a generated query."),
        "from" => Some("Copies matching input fields into a create assignment."),
        "emits" => Some("Declares a domain event emitted by a command."),
        "extensible_by" => Some("Whitelists features allowed to extend a view anchor."),
        "tests" => Some(
            "Declares inline IR assertions for a command, transition, rule, or view extension.",
        ),
        "allow" | "allows" => Some("Declares a positive inline test assertion."),
        "deny" | "denies" => Some("Declares a negative inline test assertion."),
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
        "validate" => Some("Attaches a whole-resource validator implementation."),
        "validates" => Some("Attaches a field-scoped validator implementation."),
        "client" => Some("Declares a reusable client-side extension contract."),
        "fn" => Some("Declares a reusable server-side pure function extension contract."),
        "hook" => Some("Declares a reusable lifecycle hook extension contract."),
        "validator" => Some("Declares a reusable validator extension contract."),
        "adapter" => Some("Declares a reusable integration adapter extension contract."),
        "query_modifier" => Some("Declares a reusable query modifier extension contract."),
        "required" => Some("Marks a field as required."),
        "unique" => Some("Marks a field as unique."),
        "default" => Some("Declares a default field value."),
        _ => None,
    }
}

const KEYWORDS: &[&str] = &[
    "app",
    "aggregate",
    "entity",
    "command",
    "query",
    "query.list",
    "query.lookup",
    "query.sql",
    "event.trace",
    "surface",
    "input",
    "route",
    "let",
    "policy",
    "requires",
    "modifier",
    "from",
    "emits",
    "extensible_by",
    "tests",
    "allow",
    "allows",
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
    "required",
    "unique",
    "default",
];

#[cfg(test)]
mod tests {
    use super::{diagnostics_for, format_canonical_source};
    use tower_lsp::lsp_types::DiagnosticSeverity;

    #[test]
    fn canonical_order_accepts_feature_blocks_in_order() {
        let source = r#"
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

  command create
    policy create
    creates Customer

  workflow lifecycle on Customer.status
    policy update

  job sync
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"

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
        let diagnostics = diagnostics_for(include_str!("../../../examples/full-capsule.lzi"));

        assert!(
            diagnostics.is_empty(),
            "expected no canonical ordering diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_examples_satisfy_lsp_contracts() {
        let examples = [
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
                include_str!("../../../examples/full-capsule.lzi"),
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
    fn canonical_events_payload_warns_for_unknown_resource_field() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    events customer_* on Customer
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
    policy update
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
    policy create
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
    policy create
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
    policy login
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
    policy update
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
    policy update
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
    policy update
    deletes CustomerTagAssignment
"#;

        assert!(diagnostics_for(source).is_empty());
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
    policy create
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
        let source = include_str!("../../../examples/full-capsule.lzi");
        let formatted = format_canonical_source(source).expect("canonical source");

        assert_eq!(formatted, source);
    }

    #[test]
    fn canonical_formatter_removes_blank_before_transition_children() {
        let source = r#"
feature customer
  purpose "Customers"

  workflow lifecycle on Customer.status
    policy update

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
feature billing
  purpose "Billing"

  domain
    resource Invoice

  surface web admin
    view list Table

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
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
feature customer
  purpose "Customers"

  surface web admin
    view list Table

  uses org

  domain
    resource Customer

  webhook inbound
    path "/webhooks/inbound"
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
