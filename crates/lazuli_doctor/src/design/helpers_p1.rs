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
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::design::helpers::Allowlist;
    ///
    /// let mut a = Allowlist::default();
    /// a.buckets.insert("bg".into(), vec!["primary".into()]);
    /// assert!(a.contains("bg", "primary"));
    /// assert!(!a.contains("bg", "ghost"));
    /// ```
    pub fn contains(&self, prefix: &str, suffix: &str) -> bool {
        self.buckets
            .get(prefix)
            .is_some_and(|vs| vs.iter().any(|v| v == suffix))
    }

    /// `true` when the `font` bucket lists `name`. Used by
    /// `design-token-fontfamily-leak` to validate inline font strings.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::design::helpers::Allowlist;
    ///
    /// let mut a = Allowlist::default();
    /// a.buckets.insert("font".into(), vec!["sans".into(), "mono".into()]);
    /// assert!(a.is_known_font_token("sans"));
    /// assert!(!a.is_known_font_token("Helvetica"));
    /// ```
    pub fn is_known_font_token(&self, name: &str) -> bool {
        self.buckets
            .get("font")
            .is_some_and(|vs| vs.iter().any(|v| v == name))
    }

    /// `true` when the prefix is one Doctor recognises. Unknown prefixes
    /// (e.g. `flex-`, `items-`, `justify-`) are not checked — they are not
    /// design-token bound. `design-token-undefined` only fires for prefixes
    /// that the allowlist actually owns.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::design::helpers::Allowlist;
    ///
    /// let mut a = Allowlist::default();
    /// a.buckets.insert("bg".into(), vec!["primary".into()]);
    /// assert!(a.knows_prefix("bg"));
    /// assert!(!a.knows_prefix("flex"));
    /// ```
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
///
/// Closes WAR-DOCTOR-DESIGN-02: also merges `dist/ts-web/design/allowlist.extension.json`
/// when present. The extension file is hand-authored by the capsule
/// owner to declare tokens that come from EXTERNAL workspace packages
/// (e.g. `@example/design-tokens`) which Lazuli's design.lzi
/// emitter can't see. Tokens listed in the extension append to the
/// per-prefix allowlist buckets. Same JSON shape as the canonical file.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::helpers::read_allowlist;
///
/// let allowlist = read_allowlist(Path::new("/proj")).unwrap_or_default();
/// // Every design-token rule short-circuits to empty when this returns None.
/// ```
pub fn read_allowlist(root: &Path) -> Option<Allowlist> {
    let canonical = root
        .join("dist")
        .join("ts-web")
        .join("design")
        .join("allowlist.json");
    let raw = fs::read_to_string(&canonical).ok()?;
    let mut allowlist: Allowlist = serde_json::from_str(&raw).ok()?;

    // Optional extension file for externally-defined design tokens.
    let extension_path = root
        .join("dist")
        .join("ts-web")
        .join("design")
        .join("allowlist.extension.json");
    if let Ok(ext_raw) = fs::read_to_string(&extension_path)
        && let Ok(ext) = serde_json::from_str::<Allowlist>(&ext_raw) {
            for (prefix, mut suffixes) in ext.buckets {
                let bucket = allowlist.buckets.entry(prefix).or_default();
                bucket.append(&mut suffixes);
            }
        }

    Some(allowlist)
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::helpers::walk_tsx_files;
///
/// for path in walk_tsx_files(Path::new("src")) {
///     println!("scanning {}", path.display());
/// }
/// ```
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
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::design::helpers::scan_lines;
///
/// let body = "alpha\nbeta\n";
/// let collected: Vec<_> = scan_lines(body).collect();
/// assert_eq!(collected, vec![(1, "alpha"), (2, "beta")]);
/// ```
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
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::design::helpers::is_allowed_by_escape_comment;
///
/// let lines = [
///     "// lazuli-allow: design-token-hex-leak — vendor color",
///     r##"<div style={{ color: "#7c3aed" }} />"##,
/// ];
/// assert!(is_allowed_by_escape_comment(&lines, 1, "design-token-hex-leak"));
/// ```
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
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::design::helpers::iter_class_strings;
///
/// let line = r#"<div className="bg-primary p-3" />"#;
/// let collected: Vec<_> = iter_class_strings(line).collect();
/// assert_eq!(collected, vec!["bg-primary p-3"]);
/// ```
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
            if ok && best.is_none_or(|(b, _)| idx < b) {
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
