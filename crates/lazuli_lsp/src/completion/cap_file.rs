//! `@cap.File(...)` value-side completions.
//!
//! Offer suggestions when the cursor sits inside `@cap.File(` after the
//! `:` of a known argument. Four argument keywords are recognised:
//!
//! - `visibility:` → `public`, `private`, `signed`
//! - `max_size:` → `kb`, `mb`, `gb` (binary size units)
//! - `signed_ttl:<int>` → `s`, `m`, `h`, `d`
//! - `accept:` → the seven IANA-top families (`text`, `image`, …, `*`)

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

pub(crate) fn cap_file_value_completions(
    source: &str,
    position: Position,
) -> Option<Vec<CompletionItem>> {
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
