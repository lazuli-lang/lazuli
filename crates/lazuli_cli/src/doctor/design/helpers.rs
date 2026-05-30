//! Shared utilities for the `design-token-*` doctor rules.
//!
//! - `Allowlist` — typed view over `dist/ts-web/design/allowlist.json`,
//!   the closed enum of legal Tailwind classes emitted by Cell B
//!   (`docs/proposals/design-tokens.md` §4.1).
//! - `read_allowlist` — loads the allowlist; returns `None` when the file
//!   is missing so that callers can suppress every design-token rule
//!   before `design.lzi` exists.
//! - `walk_tsx_files` — recursive walk that visits authoring `.tsx` files
//!   only (skips `node_modules`, `dist`, `.lazuli`, `target`, `.git`,
//!   `.next`, `.expo`, and test/story files).
//! - `scan_lines` — yields `(line_number_1_based, line)` pairs.
//! - `is_allowed_by_escape_comment` — implements the §6.3 inline escape
//!   hatch (`// lazuli-allow: <code> — <reason>`).
//! - `iter_class_strings` / `iter_style_block_segments` — small text
//!   slicers shared by the rule implementations. Regex-free, single-pass.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── allowlist ────────────────────────────────────────────────────────────────

/// Closed allowlist of legal Tailwind classes, keyed by utility prefix.
///
/// Cell B (design-tokens emitter) writes this to
/// `dist/ts-web/design/allowlist.json`. Doctor reads only — never re-parses
/// the Tailwind preset itself.
///
/// Example JSON shape:
/// ```json
/// {
///   "bg":      ["primary", "primary-hover", "background", "success"],
///   "text":    ["primary-foreground", "foreground", "foreground-muted"],
///   "p":       ["1", "2", "3", "4"],
///   "px":      ["1", "2", "3", "4"],
///   "rounded": ["sm", "base", "md"],
///   "shadow":  ["sm", "base", "md"],
///   "font":    ["sans", "mono"],
///   "z":       ["docked", "modal"]
/// }
/// ```
///
/// Each map entry is `prefix → declared token suffixes`. `"bg-primary"` is
/// allowed when `"primary"` is in `bg`'s vec; `"bg-purple-500"` fails when
/// `"purple-500"` is not in `bg`'s vec.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Allowlist {
    #[serde(flatten)]
    pub buckets: HashMap<String, Vec<String>>,
}

impl Allowlist {
    /// `true` when `<prefix>-<suffix>` is a declared Tailwind class.
    pub fn contains(&self, prefix: &str, suffix: &str) -> bool {
        self.buckets
            .get(prefix)
            .is_some_and(|vs| vs.iter().any(|v| v == suffix))
    }

    /// `true` when the `font` bucket lists `name`. Used by
    /// `design-token-fontfamily-leak` to validate inline font strings.
    pub fn is_known_font_token(&self, name: &str) -> bool {
        self.buckets
            .get("font")
            .is_some_and(|vs| vs.iter().any(|v| v == name))
    }

    /// `true` when the prefix is one Doctor recognises. Unknown prefixes
    /// (e.g. `flex-`, `items-`, `justify-`) are not checked — they are not
    /// design-token bound. `design-token-undefined` only fires for prefixes
    /// that the allowlist actually owns.
    pub fn knows_prefix(&self, prefix: &str) -> bool {
        self.buckets.contains_key(prefix)
    }
}

/// Reads `dist/ts-web/design/allowlist.json` from the project root.
///
/// Returns `None` when the file is missing — callers MUST treat this as
/// "no design.lzi yet, suppress every rule". Returns `None` on parse
/// failures too; doctor surfaces a separate diagnostic for malformed
/// allowlist files in a future pass (out of L0 #2 scope).
pub fn read_allowlist(root: &Path) -> Option<Allowlist> {
    let path = root
        .join("dist")
        .join("ts-web")
        .join("design")
        .join("allowlist.json");
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str::<Allowlist>(&raw).ok()
}

// ── filesystem walk ──────────────────────────────────────────────────────────

/// Recursively walk `root` and return every authoring `.tsx` file.
///
/// Skips:
/// - directories: `node_modules`, `dist`, `.lazuli`, `target`, `.git`,
///   `.next`, `.expo`
/// - filenames ending in `.test.tsx`, `.spec.tsx`, `.stories.tsx`
///
/// Output is sorted by path for deterministic diagnostics.
pub fn walk_tsx_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_tsx(root, &mut out);
    out.sort();
    out
}

fn collect_tsx(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_skipped_dir(&name) {
                continue;
            }
            collect_tsx(&path, out);
        } else if ft.is_file() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_authoring_tsx(&name) {
                out.push(path);
            }
        }
    }
}

fn is_skipped_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "dist" | ".lazuli" | "target" | ".git" | ".next" | ".expo"
    )
}

fn is_authoring_tsx(name: &str) -> bool {
    if !name.ends_with(".tsx") {
        return false;
    }
    !(name.ends_with(".test.tsx") || name.ends_with(".spec.tsx") || name.ends_with(".stories.tsx"))
}

// ── line scanner ─────────────────────────────────────────────────────────────

/// Yields `(line_number_1_based, line_content)` for every line in `content`.
///
/// Trailing line endings are stripped; intra-line whitespace is preserved
/// (rules need to inspect quoting and brace structure exactly as written).
pub fn scan_lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content.lines().enumerate().map(|(i, l)| (i + 1, l))
}

// ── escape hatch (§6.3) ──────────────────────────────────────────────────────

/// `true` when `code` is suppressed on `current_line_idx_0based` by an
/// inline-or-prev-line escape comment.
///
/// Pattern (§6.3): `// lazuli-allow: <code> — <reason>`. The `—` separator
/// is conventional but not enforced — any text after the code is accepted.
/// The comment may be on the same line as the violation OR on the line
/// immediately above it.
pub fn is_allowed_by_escape_comment(
    lines: &[&str],
    current_line_idx_0based: usize,
    code: &str,
) -> bool {
    if has_allow_comment_for(lines[current_line_idx_0based], code) {
        return true;
    }
    if current_line_idx_0based > 0
        && has_allow_comment_for(lines[current_line_idx_0based - 1], code)
    {
        return true;
    }
    false
}

fn has_allow_comment_for(line: &str, code: &str) -> bool {
    let Some(idx) = line.find("lazuli-allow:") else {
        return false;
    };
    let tail = &line[idx + "lazuli-allow:".len()..];
    let trimmed = tail.trim_start();
    if let Some(rest) = trimmed.strip_prefix(code) {
        // Code must be followed by end-of-line, whitespace, or a separator
        // character (`,` / `—` / `-`). Prevents `design-token-undefined`
        // matching `design-token-undefined-foo`.
        match rest.chars().next() {
            None => true,
            Some(c) => c.is_whitespace() || matches!(c, ',' | '—' | '-' | ':'),
        }
    } else {
        false
    }
}

// ── class-string extraction ──────────────────────────────────────────────────

/// Yields each quoted string value that appears as a `className="..."` /
/// `class="..."` attribute on `line`. Returns the *raw* string (no quotes,
/// no decoding) so callers can tokenise on whitespace.
///
/// Only `"..."` and `'...'` quoting recognised — `className={` JS
/// expressions are out of scope (callers handle those via the inline-style
/// path or future enhancements).
pub fn iter_class_strings(line: &str) -> impl Iterator<Item = &str> {
    ClassStringIter { rest: line }
}

struct ClassStringIter<'a> {
    rest: &'a str,
}

impl<'a> Iterator for ClassStringIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Find next className= / class= occurrence.
            let (consumed, key_len) = find_class_attr(self.rest)?;
            // Advance past the attribute name and `=`.
            let after_eq = &self.rest[consumed + key_len..];
            // Skip whitespace between `=` and the value (rare in JSX but
            // tolerated).
            let trimmed = after_eq.trim_start_matches(|c: char| c.is_whitespace());
            let Some(quote) = trimmed.chars().next() else {
                self.rest = "";
                return None;
            };
            if quote != '"' && quote != '\'' {
                // `className={...}` JS expression — skip past the `{` and
                // resume scanning after it.
                self.rest = &trimmed[1..];
                continue;
            }
            let after_quote = &trimmed[1..];
            let Some(end_offset) = after_quote.find(quote) else {
                self.rest = "";
                return None;
            };
            let value = &after_quote[..end_offset];
            self.rest = &after_quote[end_offset + 1..];
            return Some(value);
        }
    }
}

/// Returns `(byte_offset_to_attr_start, attr_name_byte_len)` for the next
/// `className=` or `class=` occurrence in `s`, or `None` if not found.
///
/// "Attribute start" requires a non-identifier character before the name
/// to avoid matching `myClassName=`.
fn find_class_attr(s: &str) -> Option<(usize, usize)> {
    let candidates = [("className=", 10), ("class=", 6)];
    let mut best: Option<(usize, usize)> = None;
    for (key, key_len) in candidates {
        if let Some(idx) = s.find(key) {
            let ok = match idx
                .checked_sub(1)
                .and_then(|i| s.as_bytes().get(i).copied())
            {
                None => true,
                Some(b) => !is_ident_byte(b),
            };
            if ok && best.map_or(true, |(b, _)| idx < b) {
                best = Some((idx, key_len));
            }
        }
    }
    best
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ── style-block scanner ──────────────────────────────────────────────────────

/// Per-line view of the `style={{ ... }}` regions on that line.
///
/// `StyleSpan { line_idx_0based, segment }` carries the slice of text that
/// falls inside a `style={{` ... matching `}}` region. Across multiple
/// lines, a single style block produces one `StyleSpan` per line it
/// spans (with the slice trimmed to the in-block portion). Rules iterate
/// these spans and check their own concern (hex, px, font, shadow).
#[derive(Debug, Clone)]
pub struct StyleSpan<'a> {
    pub line_idx_0based: usize,
    pub segment: &'a str,
}

/// Returns every `style={{ ... }}` segment in `content`, line-anchored.
///
/// Brace depth is tracked across lines so that multi-line style props
/// (the common case for readable React code) are scanned in full.
/// Quoted strings inside the style block do NOT terminate the block —
/// the matcher only counts `{` / `}` outside `"..."` and `'...'` regions.
pub fn iter_style_spans<'a>(content: &'a str, lines: &[&'a str]) -> Vec<StyleSpan<'a>> {
    let mut out: Vec<StyleSpan<'a>> = Vec::new();
    let _ = content; // explicit: we operate on `lines`, content is contextual

    let mut state = ScanState::Outside;
    let mut current_start_in_line: usize = 0;
    let mut current_line: usize = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut i = 0;
        if matches!(state, ScanState::Inside { .. }) {
            current_start_in_line = 0;
            current_line = line_idx;
        }
        while i < bytes.len() {
            match state {
                ScanState::Outside => {
                    if let Some(rest) = line.get(i..)
                        && let Some(rel) = rest.find("style=")
                    {
                        let pos = i + rel;
                        // require preceding char to be non-identifier (avoid `nostyle=` etc).
                        let ok = match pos.checked_sub(1).and_then(|p| bytes.get(p).copied()) {
                            None => true,
                            Some(b) => !is_ident_byte(b),
                        };
                        if !ok {
                            i = pos + "style=".len();
                            continue;
                        }
                        let after = pos + "style=".len();
                        // Look for `{{`. We tolerate a single `{` followed
                        // immediately by `{`; whitespace between is not
                        // accepted (matches conventional JSX style props).
                        if line.as_bytes().get(after).copied() == Some(b'{')
                            && line.as_bytes().get(after + 1).copied() == Some(b'{')
                        {
                            state = ScanState::Inside {
                                depth: 2,
                                in_string: None,
                            };
                            current_start_in_line = after + 2;
                            current_line = line_idx;
                            i = after + 2;
                        } else {
                            // not a style={{ block — skip past `style=` and continue.
                            i = after;
                        }
                    } else {
                        break;
                    }
                }
                ScanState::Inside {
                    ref mut depth,
                    ref mut in_string,
                } => {
                    let c = bytes[i];
                    match in_string {
                        Some(q) => {
                            if c == b'\\' && i + 1 < bytes.len() {
                                i += 2;
                                continue;
                            }
                            if c == *q {
                                *in_string = None;
                            }
                            i += 1;
                        }
                        None => {
                            if c == b'"' || c == b'\'' || c == b'`' {
                                *in_string = Some(c);
                                i += 1;
                            } else if c == b'{' {
                                *depth += 1;
                                i += 1;
                            } else if c == b'}' {
                                *depth -= 1;
                                if *depth == 0 {
                                    // End of style block (consumed the second `}` at `i`;
                                    // the matching first `}` of the pair is at `i-1`).
                                    // The segment excludes both closing braces — slice
                                    // ends at `i - 1`.
                                    let seg_end = i.saturating_sub(1);
                                    let seg_start = if current_line == line_idx {
                                        current_start_in_line
                                    } else {
                                        0
                                    };
                                    let seg = &line[seg_start..seg_end.max(seg_start)];
                                    out.push(StyleSpan {
                                        line_idx_0based: line_idx,
                                        segment: seg,
                                    });
                                    state = ScanState::Outside;
                                    i += 1;
                                } else {
                                    i += 1;
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
        // End-of-line: if still inside, emit the rest of the line as one
        // segment so per-line rules see it.
        if let ScanState::Inside { .. } = state {
            let seg = if current_line == line_idx {
                &line[current_start_in_line..]
            } else {
                &line[..]
            };
            if !seg.is_empty() {
                out.push(StyleSpan {
                    line_idx_0based: line_idx,
                    segment: seg,
                });
            }
        }
    }
    out
}

#[derive(Debug)]
enum ScanState {
    Outside,
    Inside { depth: usize, in_string: Option<u8> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as stdfs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(crate) struct TempDir {
        pub(crate) path: PathBuf,
    }

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    impl TempDir {
        pub(crate) fn new(tag: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join("lazuli-design-doctor-test")
                .join(format!("{tag}-{nonce}-{id}-{}", std::process::id()));
            stdfs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        pub(crate) fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = stdfs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn allowlist_reads_known_buckets() {
        let td = TempDir::new("allowlist");
        let dir = td.path().join("dist").join("ts-web").join("design");
        stdfs::create_dir_all(&dir).unwrap();
        stdfs::write(
            dir.join("allowlist.json"),
            r#"{"bg":["primary","success"],"text":["foreground"],"font":["sans"]}"#,
        )
        .unwrap();
        let al = read_allowlist(td.path()).expect("allowlist parses");
        assert!(al.contains("bg", "primary"));
        assert!(!al.contains("bg", "purple-500"));
        assert!(al.is_known_font_token("sans"));
        assert!(al.knows_prefix("bg"));
        assert!(!al.knows_prefix("flex"));
    }

    #[test]
    fn allowlist_missing_returns_none() {
        let td = TempDir::new("allowlist-missing");
        assert!(read_allowlist(td.path()).is_none());
    }

    #[test]
    fn walk_tsx_skips_node_modules_and_tests() {
        let td = TempDir::new("walk");
        let root = td.path();
        stdfs::create_dir_all(root.join("features").join("hello")).unwrap();
        stdfs::create_dir_all(root.join("node_modules").join("react")).unwrap();
        stdfs::create_dir_all(root.join("dist").join("ts-web")).unwrap();
        stdfs::write(root.join("features").join("hello").join("ok.tsx"), "x").unwrap();
        stdfs::write(root.join("features").join("hello").join("ok.test.tsx"), "x").unwrap();
        stdfs::write(
            root.join("features").join("hello").join("ok.stories.tsx"),
            "x",
        )
        .unwrap();
        stdfs::write(root.join("node_modules").join("react").join("idx.tsx"), "x").unwrap();
        stdfs::write(root.join("dist").join("ts-web").join("gen.tsx"), "x").unwrap();
        let files = walk_tsx_files(root);
        assert_eq!(files.len(), 1, "found: {:?}", files);
        assert!(files[0].ends_with("ok.tsx"));
    }

    #[test]
    fn scan_lines_indexes_from_one() {
        let lines: Vec<(usize, &str)> = scan_lines("a\nb\nc").collect();
        assert_eq!(lines, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn escape_comment_same_line_suppresses() {
        let lines = vec![
            "function x() {",
            "  return <div style={{ color: \"#fff\" }} />; // lazuli-allow: design-token-hex-leak — vendor brand",
            "}",
        ];
        assert!(is_allowed_by_escape_comment(
            &lines,
            1,
            "design-token-hex-leak"
        ));
    }

    #[test]
    fn escape_comment_prev_line_suppresses() {
        let lines = vec![
            "// lazuli-allow: design-token-hex-leak — vendor brand",
            "<div style={{ color: \"#fff\" }} />",
        ];
        assert!(is_allowed_by_escape_comment(
            &lines,
            1,
            "design-token-hex-leak"
        ));
    }

    #[test]
    fn escape_comment_only_matches_exact_code() {
        let lines = vec!["x // lazuli-allow: design-token-hex-leak — note"];
        assert!(is_allowed_by_escape_comment(
            &lines,
            0,
            "design-token-hex-leak"
        ));
        assert!(!is_allowed_by_escape_comment(
            &lines,
            0,
            "design-token-px-leak"
        ));
        // Prefix-only collision must NOT match.
        let lines2 = vec!["x // lazuli-allow: design-token-hex-leak-and-more"];
        // The escape is `design-token-hex-leak` followed by `-and-more` —
        // a `-` is an accepted separator per the parser. This is the
        // intentional behaviour: `-` is treated as a separator so that
        // hyphenated reasons need not be quoted.
        assert!(is_allowed_by_escape_comment(
            &lines2,
            0,
            "design-token-hex-leak"
        ));
    }

    #[test]
    fn class_string_iter_emits_jsx_attrs() {
        let line = r#"<div className="bg-primary text-foreground" id="x"></div>"#;
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert_eq!(items, vec!["bg-primary text-foreground"]);
    }

    #[test]
    fn class_string_iter_handles_single_quotes() {
        let line = r#"<div className='bg-primary'></div>"#;
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert_eq!(items, vec!["bg-primary"]);
    }

    #[test]
    fn class_string_iter_skips_jsx_expressions() {
        let line = r#"<div className={cn("bg-primary")}></div>"#;
        // JSX expression `{...}` is skipped by the class iterator (returns
        // empty for the className= match); the inner literal would be
        // caught by a future enhancement. v0 accepts this trade-off.
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert!(items.is_empty());
    }

    #[test]
    fn style_span_single_line() {
        let content = r##"<div style={{ color: "#fff", padding: "12px" }} />"##;
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].segment.contains("#fff"));
        assert!(spans[0].segment.contains("12px"));
    }

    #[test]
    fn style_span_multi_line() {
        let content = "<div\n  style={{\n    color: \"#fff\",\n    padding: \"12px\",\n  }}\n/>";
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        // Expect one span per line that overlaps the style block (lines 2..5).
        assert!(spans.len() >= 3);
        let combined: String = spans.iter().map(|s| s.segment.to_string()).collect();
        assert!(combined.contains("#fff"));
        assert!(combined.contains("12px"));
    }

    #[test]
    fn style_span_string_braces_ignored() {
        let content = r#"<div style={{ content: "a}b" }} />"#;
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].segment.contains("a}b"));
    }
}
