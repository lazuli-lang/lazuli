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
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_vocab_resolved_text;
///
/// // No feature-level override — falls back to the built-in English string.
/// let text = error_vocab_resolved_text("", "billing", "not_found").unwrap();
/// assert!(!text.is_empty());
/// ```
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
pub(crate) fn lookup_feature_error_key(
    source: &str,
    feature_name: &str,
    code: &str,
) -> Option<String> {
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
pub(crate) fn lookup_translation_first_variant(
    source: &str,
    feature_name: &str,
    key: &str,
) -> Option<String> {
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
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_vocab_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Outside any error-vocab trigger context — no completions.
/// let result = error_vocab_completions("feature billing\n", Position { line: 0, character: 0 });
/// assert!(result.is_none());
/// ```
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

/// Resolved-text hover for the 8 closed-catalog error codes, fired when
/// the cursor sits on one of them inside a feature's `errors` block. Shows
/// the locally-resolved translation (from the same feature's `translation`
/// block, first locale variant) or, if no feature-level override exists,
/// the built-in English fallback shipped by the runtime.
///
/// Returns `None` when the cursor is outside an `errors` block or when
/// `word` is not one of the 12 codes. The rich-markdown one-liner for the
/// codes ships through `keyword_description` instead.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_vocab_code_resolved_hover;
/// use tower_lsp::lsp_types::Position;
///
/// // Word is not in the catalog — None.
/// let hover = error_vocab_code_resolved_hover("", Position { line: 0, character: 0 }, "nonsense");
/// assert!(hover.is_none());
/// ```
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
