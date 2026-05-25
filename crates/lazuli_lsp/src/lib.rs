use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lazuli_syntax::Span;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, InsertTextFormat, MarkupContent, MarkupKind, MessageType, OneOf, Position,
    Range, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
    WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

mod catalogs;
mod code_actions;
mod completion;
mod completion_items;
mod conventions;
mod diagnostics;
mod dispatch;
mod format;
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
pub(crate) use diagnostics::app::*;
pub(crate) use diagnostics::auth::*;
pub(crate) use diagnostics::cache::*;
pub(crate) use diagnostics::canonical_kinds::*;
pub(crate) use diagnostics::capability::*;
pub(crate) use diagnostics::command::*;
pub(crate) use diagnostics::crypto::*;
pub(crate) use diagnostics::doctor_local::*;
pub(crate) use diagnostics::env::*;
pub(crate) use diagnostics::error::*;
pub(crate) use diagnostics::event::*;
pub(crate) use diagnostics::external::*;
pub(crate) use diagnostics::field::*;
pub(crate) use diagnostics::http_headers::*;
pub(crate) use diagnostics::lifecycle::*;
pub use diagnostics::lifecycle::{lifecycle_gate_completions, lifecycle_gate_hover};
pub(crate) use diagnostics::lzx::*;
pub(crate) use diagnostics::notification::*;
pub(crate) use diagnostics::policy::*;
pub(crate) use diagnostics::profile::*;
pub(crate) use diagnostics::query::*;
pub(crate) use diagnostics::registry::*;
pub(crate) use diagnostics::route_guard::*;
pub use diagnostics::route_guard::{route_guard_completions, route_guard_hover};
pub(crate) use diagnostics::security::*;
pub(crate) use diagnostics::vocab::*;
pub(crate) use diagnostics::webhook::*;
pub(crate) use diagnostics::workspace::*;

pub use catalogs::*;
pub use code_actions::auth_refresh::auth_refresh_code_actions;
pub use code_actions::error_vocab::error_vocab_code_actions;
pub use code_actions::lifecycle_gate::lifecycle_gate_code_actions;
pub use code_actions::route_guard::route_guard_code_actions;
pub use completion::auth_refresh::auth_refresh_completions;
pub(crate) use completion::auth_refresh::{
    AuthRotationBlock, AuthSessionsBlock, after_keyword_value_prefix,
    auth_refresh_rotation_clause_completion_items, auth_refresh_theft_action_completion_items,
    auth_rotation_has_children, auth_sessions_has_child, block_end_line,
    duration_literal_completion_items, enclosing_auth_rotation_block,
    enclosing_auth_sessions_block, has_auth_parent, is_rotation_line, is_sessions_line,
    rotation_block_snippet_completion,
};
pub(crate) use completion::cap_file::cap_file_value_completions;
pub(crate) use completion::context::{
    EFFECT_VERBS, KIND_CHILD_COMPLETIONS, RATE_LIMIT_AXES, block_kind_at,
    context_aware_completions, convention_bundle_hover, is_inside_conventions_list,
    rate_limit_axis_completions,
};
pub(crate) use completion::error_page::error_page_value_completions;
pub use completion::error_vocab::{
    error_vocab_code_resolved_hover, error_vocab_completions, error_vocab_resolved_text,
};
pub(crate) use completion::error_vocab::{
    in_feature_errors_block, lookup_feature_error_key, lookup_translation_first_variant,
};
pub use completion::input_field::input_field_completions;
pub(crate) use completion::input_field::{
    collect_command_input_and_route_params, input_dot_trigger,
};
pub(crate) use completion::namespace::{collect_namespace_names, namespace_prefix_completions};
pub(crate) use completion::owner_axis::owner_axis_through_completions;
pub(crate) use completion_items::{completion_items_for_uri, make_symbol, merge_completion_items};
pub use conventions::conventions_list_completions;
pub(crate) use format::canonical::*;
pub use hover::*;
pub(crate) use keywords::{DESIGN_KEYWORDS, KEYWORDS, design_keyword_description};
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

// Catalog detail-lookup functions (`resource_lock_strategy_detail`,
// `error_page_status_detail`, `auth_catalog_detail`,
// `auth_refresh_theft_action_detail`, `observability_catalog_detail`,
// `notification_digest_template_strategy_detail`,
// `deploy_strategy_detail`) now live in `catalogs.rs` and are
// re-exported via `pub use catalogs::*;`.

/// Position at the start of `line_idx` (character 0). Used as both the
/// start and end of an inserting `TextEdit` (zero-width range).
pub(crate) fn position_at_line_start(line_idx: usize) -> Position {
    Position {
        line: line_idx as u32,
        character: 0,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
