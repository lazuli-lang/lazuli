pub(crate) struct Backend {
    pub(crate) client: Client,
    pub(crate) documents: Arc<RwLock<HashMap<Url, String>>>,
    /// Workspace root captured from the `initialize` handshake
    /// (`root_uri` / first workspace folder). Used as the starting point
    /// for `Lazurite.toml` discovery when a document's own path can't be
    /// resolved.
    pub(crate) workspace_root: Arc<RwLock<Option<PathBuf>>>,
    /// D3 — monotonic generation counter for the debounced package-engine
    /// run. Each `publish_diagnostics` bumps it; the background task
    /// captures its generation and only publishes if it is still the
    /// latest when the engine finishes — so a burst of keystrokes collapses
    /// to one engine run, and stale results never overwrite fresh ones.
    pub(crate) doctor_run_generation: Arc<AtomicU64>,
}

/// Debounce window before the background package-engine run fires. Short
/// enough to feel live, long enough that a burst of keystrokes collapses
/// to a single full-workspace run.
const DOCTOR_RUN_DEBOUNCE: Duration = Duration::from_millis(400);

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
/// v2 single-source residual — the `Lazurite.toml` body is read from the
/// **open-document store first** (`open_docs`): if the workspace
/// `Lazurite.toml` is open in the editor, its UNSAVED buffer drives the
/// resolved config, so an unsaved `[doctor]` profile / preset /
/// `severity_override` edit changes in-editor severity live (for BOTH the
/// file-local layer and the package-engine layer, which now share this
/// config). Falls back to the on-disk file when that document is not open.
///
/// Falls back to a profile-only `Strict` config when no manifest is
/// reachable (single-file edits, scratch dirs) — matching the CLI's
/// behavior on manifest-less invocations.
fn resolve_workspace_config(
    doc_uri: &Url,
    workspace_root: Option<&Path>,
    open_docs: &HashMap<Url, String>,
) -> ResolvedDoctorConfig {
    let manifest_path = doc_uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .and_then(|dir| find_lazurite_manifest(&dir))
        .or_else(|| workspace_root.and_then(find_lazurite_manifest));

    let Some(manifest_path) = manifest_path else {
        return ResolvedDoctorConfig::default();
    };

    // Prefer the UNSAVED editor buffer when the workspace `Lazurite.toml`
    // is open, so unsaved `[doctor]` edits take effect immediately; else
    // read the on-disk file.
    let buffered = Url::from_file_path(&manifest_path)
        .ok()
        .and_then(|manifest_uri| open_docs.get(&manifest_uri).cloned());
    let body = match buffered {
        Some(buf) => buf,
        None => match std::fs::read_to_string(&manifest_path) {
            Ok(disk) => disk,
            Err(_) => return ResolvedDoctorConfig::default(),
        },
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
                // Doctor-fix bridge (audit gap #7) — the `lazuli.applyFix`
                // command the doctor-fix code-actions dispatch to. Runs the
                // shared `lazuli_fix` kernel (same one `lazuli fix --apply`
                // uses) so CLI + LSP fixes stay byte-identical.
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        crate::code_actions::doctor_fix::APPLY_FIX_COMMAND.to_owned(),
                    ],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                // H4 — parser-driven semantic-token highlighting. Legend
                // is the `lazuli_keywords::SemanticToken` projection so it
                // tracks the registry. `full` only for now; clients fall
                // back to the static tmLanguage grammar for spans the
                // classifier under-classifies (by design).
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: crate::semantic_tokens::legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
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
        // The client echoes the document's published diagnostics (filtered
        // to the request range) in `context.diagnostics` — that's where the
        // Layer-2 doctor findings carrying a `lazuli_fix` envelope come
        // back, so the doctor-fix bridge can turn them into actions.
        let context_diagnostics = params.context.diagnostics;
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&uri) else {
            return Ok(None);
        };
        Ok(handlers::code_actions_for_position(
            source,
            &uri,
            position,
            &context_diagnostics,
        ))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command != crate::code_actions::doctor_fix::APPLY_FIX_COMMAND {
            return Ok(None);
        }
        let Some(argument) = params.arguments.into_iter().next() else {
            return Ok(None);
        };
        let Ok(data) =
            serde_json::from_value::<crate::code_actions::doctor_fix::DoctorFixData>(argument)
        else {
            self.client
                .log_message(
                    MessageType::ERROR,
                    "lazuli.applyFix: malformed fix argument",
                )
                .await;
            return Ok(None);
        };

        // Apply the fix through the SHARED kernel — byte-identical to
        // `lazuli fix --apply`. The action reads + writes the file on disk.
        let request = lazuli_fix::FixRequest {
            rule: data.rule.clone(),
            path: std::path::PathBuf::from(&data.path),
            line: data.line,
            column: data.column,
            apply: true,
        };
        let result = match lazuli_fix::execute(&request) {
            Ok(result) => result,
            Err(err) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("lazuli.applyFix failed for {}: {err}", data.rule),
                    )
                    .await;
                return Ok(None);
            }
        };

        match result.outcome {
            lazuli_fix::FixOutcome::Applied => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("lazuli.applyFix applied {} to {}", data.rule, data.path),
                    )
                    .await;
                // The fix wrote to disk; refresh the in-memory buffer (if
                // the file is open) and re-publish so squiggles reflect the
                // applied change without waiting for the editor's own
                // did-change round-trip.
                if let Ok(uri) = Url::from_file_path(&data.path)
                    && let Ok(updated) = std::fs::read_to_string(&data.path)
                {
                    self.documents
                        .write()
                        .await
                        .insert(uri.clone(), updated.clone());
                    self.publish_diagnostics(uri, updated).await;
                }
            }
            lazuli_fix::FixOutcome::NoChange => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("lazuli.applyFix: no change needed for {}", data.rule),
                    )
                    .await;
            }
            lazuli_fix::FixOutcome::Skipped | lazuli_fix::FixOutcome::Preview => {
                let note = result.note.unwrap_or_else(|| "fix not applied".to_owned());
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("lazuli.applyFix: {} — {note}", data.rule),
                    )
                    .await;
            }
        }

        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let documents = self.documents.read().await;
        let Some(source) = documents.get(&uri) else {
            return Ok(None);
        };
        let tokens = crate::semantic_tokens::semantic_tokens_full(source);
        Ok(Some(SemanticTokensResult::Tokens(tokens)))
    }
}

impl Backend {
    pub(crate) async fn publish_diagnostics(&self, uri: Url, source: String) {
        // W2 — resolve the workspace `[doctor]` config (profile + presets
        // + overrides) per publish so manifest edits take effect without a
        // server restart, and so single-file opens still discover the
        // nearest `Lazurite.toml`. Cheap: one small TOML parse.
        //
        // v2 — resolve from the open-document store FIRST so an unsaved
        // `Lazurite.toml` buffer drives both the file-local layer (below)
        // and the package-engine layer (the config is now threaded into
        // `run_package`). Snapshot the docs map once; the workspace
        // manifest is usually small, so the clone is cheap.
        let workspace_root = self.workspace_root.read().await.clone();
        let config = {
            let docs = self.documents.read().await;
            resolve_workspace_config(&uri, workspace_root.as_deref(), &docs)
        };

        // Layer 1 — synchronous file-local pass for typing responsiveness.
        // These are the LSP-owned shape / contract / security diagnostics;
        // published immediately so squiggles track keystrokes.
        let file_local = diagnostics_for_uri_with_config(&uri, &source, &config);
        self.client
            .publish_diagnostics(uri.clone(), file_local.clone(), None)
            .await;

        // Layer 2 — debounced background package-engine run (D3). Bump the
        // generation; a later keystroke that bumps again will make THIS
        // task's result stale, so it won't publish. Only fires when a
        // workspace root + the document's file path are resolvable (the
        // engine needs a real project to load).
        let generation = self.doctor_run_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (Some(workspace_root), Ok(_doc_path)) = (workspace_root, uri.to_file_path()) else {
            return;
        };

        let generation_handle = Arc::clone(&self.doctor_run_generation);
        let client = self.client.clone();
        // v2 — thread the FULL resolved config (profile + presets +
        // overrides, buffer-preferring) into the package engine, not just
        // the profile, so the package layer's published severities react to
        // unsaved `[doctor]` edits exactly like the file-local layer.
        let config_for_engine = config.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DOCTOR_RUN_DEBOUNCE).await;
            // Debounce: a newer publish superseded us — bail before the
            // (synchronous, potentially heavy) engine run.
            if generation_handle.load(Ordering::SeqCst) != generation {
                return;
            }

            // The package engine run is synchronous + filesystem-bound;
            // keep it off the async reactor.
            let doc_for_engine = uri.clone();
            let source_for_engine = source.clone();
            let doctor_owned = match tokio::task::spawn_blocking(move || {
                crate::doctor_engine::doctor_owned_for_document(
                    &workspace_root,
                    &doc_for_engine,
                    &source_for_engine,
                    &config_for_engine,
                )
            })
            .await
            {
                Ok(doctor_owned) => doctor_owned,
                // A panic in the package engine must NOT silently erase the
                // squiggles: surface the panic to the client log and keep
                // the already-computed Layer-1 (file-local) diagnostics so
                // shape/contract findings survive an engine crash.
                Err(join_err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("doctor engine panicked: {join_err}"),
                        )
                        .await;
                    Vec::new()
                }
            };

            // Generation re-check: a keystroke could have landed during the
            // engine run. If so, the newer publish already owns the squiggle
            // set — don't clobber it with stale results.
            if generation_handle.load(Ordering::SeqCst) != generation {
                return;
            }

            // Republish the MERGED stream (LSP-owned file-local + the
            // doctor-owned package findings) since an LSP publish replaces
            // the document's diagnostics wholesale.
            let mut merged = file_local;
            merged.extend(doctor_owned);
            client.publish_diagnostics(uri, merged, None).await;
        });
    }
}
