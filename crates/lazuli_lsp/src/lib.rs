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
        diagnostics.extend(event_payload_reference_diagnostics(source));
        diagnostics.extend(command_contract_diagnostics(source));
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
    name: String,
    line_index: usize,
    line: String,
    route_slots: HashSet<String>,
    route_references: Vec<CommandRouteReference>,
    has_policy: bool,
    has_target: bool,
    has_write_effect: bool,
    needs_default_route_target: bool,
}

impl CanonicalCommandFacts {
    fn new(name: String, line_index: usize, line: &str) -> Self {
        Self {
            name,
            line_index,
            line: line.to_owned(),
            route_slots: HashSet::new(),
            route_references: Vec::new(),
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

fn command_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_command: Option<CanonicalCommandFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command));
            }

            current_command = Some(CanonicalCommandFacts::new(
                command_name(trimmed),
                line_index,
                line,
            ));
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command));
            }
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
            } else if trimmed.starts_with("target ") {
                command.has_target = true;
            } else if let Some(effect) = command_write_effect(trimmed) {
                command.has_write_effect = true;
                command.needs_default_route_target = matches!(effect, "updates" | "deletes");
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
        diagnostics.extend(command_diagnostics(command));
    }

    diagnostics
}

fn command_diagnostics(command: CanonicalCommandFacts) -> Vec<Diagnostic> {
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

fn command_write_effect(trimmed_line: &str) -> Option<&str> {
    let effect = trimmed_line.split_whitespace().next()?;
    if matches!(effect, "creates" | "updates" | "deletes") {
        Some(effect)
    } else {
        None
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
        formatted.extend(segment.lines);
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
        "surface" => Some("Declares UI projections for list, form, and detail views."),
        "input" => Some("Lists fields accepted by a command."),
        "route" => Some("Declares route or context values accepted by a command."),
        "let" => Some("Binds a derived value for later command, job, or event expressions."),
        "policy" => Some("Associates a command with an authorization policy capability."),
        "emits" => Some("Declares a domain event emitted by a command."),
        "search" => Some("Lists fields used by a query search index."),
        "filter" => Some("Lists fields available as query filters."),
        "list" => Some("Declares table/list fields for a surface."),
        "form" => Some("Declares editable form fields for a surface."),
        "detail" => Some("Declares read-only detail fields for a surface."),
        "columns" => Some("Introduces list columns."),
        "fields" => Some("Introduces form or detail fields."),
        "validate" => Some("Attaches a whole-resource validator implementation."),
        "validates" => Some("Attaches a field-scoped validator implementation."),
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
    "surface",
    "input",
    "route",
    "let",
    "policy",
    "emits",
    "search",
    "filter",
    "list",
    "form",
    "detail",
    "columns",
    "fields",
    "validate",
    "validates",
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
    read: same_org

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
    server before_create: Hook[CreateCustomer]

  escape_route "/admin/customer-debug"
    at "./pages/customer_debug.tsx"
    policy role_admin
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
    fn canonical_formatter_preserves_full_capsule_fixture() {
        let source = include_str!("../../../examples/full-capsule.lzi");
        let formatted = format_canonical_source(source).expect("canonical source");

        assert_eq!(formatted, source);
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
