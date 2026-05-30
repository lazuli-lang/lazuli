//! `design-token-undefined` — Tailwind class not in `allowlist.json`.
//!
//! Triggers when a `.tsx` file uses a class like `bg-purple-500` where
//! `purple-500` is not declared in `design.lzi`'s color group. The rule
//! consults the allowlist emitted by Cell B at
//! `dist/ts-web/design/allowlist.json` — Doctor never re-parses the
//! Tailwind preset itself.
//!
//! Severity: warning in strict, error in production
//! (`docs/proposals/design-tokens.md` §6.1).
//!
//! Arbitrary values (`bg-[#fff]`) are NOT flagged here — they belong to
//! `design-token-hex-leak`.

use std::path::{Path, PathBuf};

use super::helpers::{
    Allowlist, is_allowed_by_escape_comment, iter_class_strings, scan_lines, walk_tsx_files,
};

/// Tailwind utility prefixes Doctor recognises. Each MUST also appear as a
/// bucket key in `allowlist.json` (Cell B emits the buckets that match
/// these prefixes). Order matters: longer prefixes MUST appear before
/// shorter ones with the same head (`shadow-` before `shadow`) so the
/// matcher picks the most specific bucket.
const KNOWN_PREFIXES: &[&str] = &[
    "bg-",
    "text-",
    "border-",
    "outline-",
    "ring-",
    "fill-",
    "stroke-",
    "from-",
    "to-",
    "via-",
    "px-",
    "py-",
    "pt-",
    "pr-",
    "pb-",
    "pl-",
    "p-",
    "mx-",
    "my-",
    "mt-",
    "mr-",
    "mb-",
    "ml-",
    "m-",
    "gap-x-",
    "gap-y-",
    "gap-",
    "rounded-",
    "rounded",
    "shadow-",
    "shadow",
    "z-",
    "font-",
    "leading-",
    "tracking-",
    "duration-",
    "ease-",
];

/// Bare class names (no trailing `-`) that map to a `DEFAULT` token of
/// the same bucket. E.g. `rounded` → check `rounded` bucket for `DEFAULT`;
/// `shadow` → check `shadow` bucket for `DEFAULT`. The Tailwind v3 preset
/// shape uses `DEFAULT` for the unscoped utility.
const BARE_CLASS_DEFAULTS: &[(&str, &str)] = &[("rounded", "rounded"), ("shadow", "shadow")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub class_token: String,
    pub prefix: String,
    pub suffix: String,
}

impl Finding {
    pub const CODE: &'static str = "design-token-undefined";

    pub fn message(&self) -> String {
        format!(
            "Tailwind class `{}` uses prefix `{}` with suffix `{}` not declared in design.lzi. \
             Either add the token to `design.lzi` or use a declared token.",
            self.class_token, self.prefix, self.suffix,
        )
    }
}

/// Run `design-token-undefined` across every authoring `.tsx` file under
/// `root`. Findings are sorted by `(path, line)` for determinism.
pub fn check(root: &Path, allowlist: &Allowlist) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in walk_tsx_files(root) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        findings.extend(check_file(&path, &lines, allowlist));
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    findings
}

/// Same as `check` but for a single in-memory file. Exposed so the
/// integration test can drive the rule without temp-files-for-every-case.
pub fn check_file(path: &Path, lines: &[&str], allowlist: &Allowlist) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (line_num, line) in scan_lines(&lines.join("\n")) {
        let idx0 = line_num - 1;
        for class_str in iter_class_strings(line) {
            for raw in class_str.split_ascii_whitespace() {
                // Strip Tailwind variants (`hover:`, `dark:focus:`, `md:`): take last segment.
                let token = raw.rsplit(':').next().unwrap_or(raw);
                // Strip leading `!` (Tailwind important modifier).
                let token = token.strip_prefix('!').unwrap_or(token);
                if token.is_empty() {
                    continue;
                }
                // Arbitrary values (`bg-[#fff]`, `text-[16px]`) are not this
                // rule's concern.
                if token.contains("[") {
                    continue;
                }
                let Some((prefix, suffix)) = match_prefix(token) else {
                    continue;
                };
                // Empty suffix on a "bare" class (`rounded`, `shadow`) →
                // check the `DEFAULT` slot in the bucket.
                let lookup_suffix = if suffix.is_empty() { "DEFAULT" } else { suffix };
                let bucket_key = prefix.trim_end_matches('-');
                if !allowlist.knows_prefix(bucket_key) {
                    continue;
                }
                if allowlist.contains(bucket_key, lookup_suffix) {
                    continue;
                }
                if is_allowed_by_escape_comment(lines, idx0, Finding::CODE) {
                    continue;
                }
                findings.push(Finding {
                    path: path.to_path_buf(),
                    line: line_num,
                    class_token: token.to_string(),
                    prefix: bucket_key.to_string(),
                    suffix: lookup_suffix.to_string(),
                });
            }
        }
    }
    findings
}

/// Match `token` against the closed prefix list. Returns
/// `(prefix_including_trailing_dash_if_any, suffix)`.
///
/// `rounded` matches as prefix `rounded`, suffix `""` (DEFAULT lookup).
/// `rounded-md` matches as prefix `rounded-`, suffix `"md"`.
fn match_prefix(token: &str) -> Option<(&'static str, &str)> {
    for prefix in KNOWN_PREFIXES {
        if token == *prefix && BARE_CLASS_DEFAULTS.iter().any(|(p, _)| *p == *prefix) {
            return Some((*prefix, ""));
        }
        if let Some(rest) = token.strip_prefix(*prefix) {
            // For the bare prefixes (`rounded`, `shadow`), `strip_prefix`
            // would also strip e.g. `roundedness` — guard against the
            // non-dash bare prefixes by requiring next char to be empty
            // or a digit/letter that continues a token suffix.
            if prefix.ends_with('-') {
                if rest.is_empty() {
                    continue;
                }
                return Some((*prefix, rest));
            }
            // Bare prefix: rest must be empty (already covered above)
            // or start with `-` (handled by the `<bare>-` entry in the list).
            // If rest starts with anything else, this isn't the prefix we want.
            if !rest.is_empty() {
                continue;
            }
            return Some((*prefix, ""));
        }
    }
    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn al(pairs: &[(&str, &[&str])]) -> Allowlist {
        let mut buckets = HashMap::new();
        for (k, vs) in pairs {
            buckets.insert((*k).to_string(), vs.iter().map(|s| s.to_string()).collect());
        }
        Allowlist { buckets }
    }

    #[test]
    fn trigger_undeclared_color() {
        let lines = vec![r#"<div className="bg-purple-500" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "bg");
        assert_eq!(f[0].suffix, "purple-500");
    }

    #[test]
    fn allow_declared_token() {
        let lines = vec![r#"<div className="bg-primary text-primary-foreground" />"#];
        let allowlist = al(&[("bg", &["primary"]), ("text", &["primary-foreground"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found unexpected: {:?}", f);
    }

    #[test]
    fn escape_comment_suppresses() {
        let lines = vec![
            "// lazuli-allow: design-token-undefined — third-party widget",
            r#"<div className="bg-purple-500" />"#,
        ];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn unknown_prefix_not_checked() {
        // `flex` / `items-center` are not design-token bound — Doctor
        // doesn't own them. No allowlist bucket = no finding.
        let lines = vec![r#"<div className="flex items-center justify-between" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    #[test]
    fn variant_prefix_stripped_before_lookup() {
        let lines = vec![r#"<div className="hover:bg-primary md:dark:bg-success" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "variants must strip cleanly; found: {:?}", f);
    }

    #[test]
    fn arbitrary_value_not_flagged_here() {
        // `bg-[#fff]` is the hex-leak rule's concern, not this one.
        let lines = vec![r#"<div className="bg-[#fff]" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    #[test]
    fn bare_default_token_resolves() {
        let lines = vec![r#"<div className="rounded" />"#];
        let allowlist = al(&[("rounded", &["DEFAULT", "md", "lg"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(
            f.is_empty(),
            "rounded → DEFAULT slot lookup; found: {:?}",
            f
        );
    }

    #[test]
    fn bare_default_token_missing_fires() {
        let lines = vec![r#"<div className="rounded" />"#];
        let allowlist = al(&[("rounded", &["md", "lg"])]); // no DEFAULT
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].suffix, "DEFAULT");
    }

    #[test]
    fn important_modifier_stripped() {
        let lines = vec![r#"<div className="!bg-primary" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    #[test]
    fn test_file_skipped() {
        // Validated by walk_tsx_files unit tests; here we mainly assert
        // the rule API still works with a `.test.tsx` path passed manually.
        // (Doctor's main entrypoint goes through walk_tsx_files, which
        // already skips `.test.tsx`.)
        let lines = vec![r#"<div className="bg-purple-500" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        // The rule itself does not re-check filename — it's the walker's job.
        // We assert here that the rule remains pure (it fires when called).
        let f = check_file(Path::new("x.test.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
    }
}
