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
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn resolved_text_falls_back_to_builtin() {
        let text = error_vocab_resolved_text("", "billing", "not_found")
            .expect("builtin fallback should resolve");
        assert!(!text.is_empty());
    }

    #[test]
    fn completions_return_none_outside_trigger_context() {
        assert!(error_vocab_completions("feature x\n", Position { line: 0, character: 0 }).is_none());
    }

    #[test]
    fn resolved_hover_rejects_unknown_word() {
        let hover = error_vocab_code_resolved_hover(
            "",
            Position { line: 0, character: 0 },
            "definitely_not_a_code",
        );
        assert!(hover.is_none());
    }
}
