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
/// shorter ones with the same head (`shadow-` before `shadow`,
/// `ring-offset-` before `ring-`) so the matcher picks the most specific
/// bucket.
const KNOWN_PREFIXES: &[&str] = &[
    "bg-",
    "text-",
    "border-",
    "outline-",
    "ring-offset-",
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

/// Tailwind prefixes whose `<prefix>-<suffix>` form is ambiguous across
/// multiple allowlist buckets. The `text-` prefix has dual meaning:
/// `text-<color>` (color bucket: `text`) and `text-<size>` (typography
/// scale bucket: `text-size`). Looking up only the first bucket would
/// false-fire on every typography-scale class. `match_prefix` resolves
/// the bucket key — `extra_buckets_for(bucket_key)` enumerates the
/// additional buckets the rule must also probe before flagging.
///
/// The `ring-offset` bucket key is rare to be emitted on its own
/// (Tailwind's ring-offset-color shares the project's color palette);
/// we fall the lookup through to the `ring` color bucket so
/// `ring-offset-background` resolves when `background` is declared as
/// a ring color.
fn extra_buckets_for(bucket_key: &str) -> &'static [&'static str] {
    match bucket_key {
        "text" => &["text-size"],
        "ring-offset" => &["ring"],
        _ => &[],
    }
}

/// `true` when `(prefix, suffix)` denotes a Tailwind ring-width or
/// ring-offset-width built-in (`ring-2`, `ring-inherit`, `ring-offset-4`,
/// `ring-offset-inherit`, etc). These are NOT design-token color
/// references — they configure stroke width and never resolve via the
/// allowlist. Doctor short-circuits them before the bucket lookup so
/// projects don't have to declare numeric pseudo-tokens on every
/// `ring` or `ring-offset` bucket.
fn is_ring_width_builtin(prefix: &str, suffix: &str) -> bool {
    if prefix != "ring-" && prefix != "ring-offset-" {
        return false;
    }
    if suffix == "inherit" {
        return true;
    }
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

/// Strip Tailwind's opacity-slash modifier (`brand/90`, `primary/50`)
/// when the substring after `/` is purely numeric. The modifier
/// configures alpha and is not part of the design token's identity —
/// `bg-brand/90` should resolve when `brand` is declared.
///
/// Used as a FALLBACK lookup: the rule first probes the allowlist with
/// the modifier intact so projects that listed explicit `black/45`-style
/// entries in their `allowlist.extension.json` keep working, and only
/// strips when that direct probe misses.
///
/// Non-numeric tails (e.g. `fill-rule/even`-style hypotheticals) are
/// left alone so we don't accidentally swallow class shapes that future
/// Tailwind versions might give meaning to.
fn strip_opacity_modifier(suffix: &str) -> &str {
    let Some(idx) = suffix.rfind('/') else {
        return suffix;
    };
    let tail = &suffix[idx + 1..];
    if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
        &suffix[..idx]
    } else {
        suffix
    }
}

/// One DESIGN-TOKEN-UNDEFINED finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.tsx` file path that contains the undeclared class.
    pub path: PathBuf,
    /// 1-based line number where the class was found.
    pub line: usize,
    /// Full Tailwind token (e.g. `bg-brand`) as authored.
    pub class_token: String,
    /// Token prefix (e.g. `bg-`).
    pub prefix: String,
    /// Token suffix (e.g. `brand`).
    pub suffix: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "design-token-undefined";

    /// Render the user-facing diagnostic body — surfaces the offending
    /// token and prompts to declare it in `design.lzi`.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::design::token_undefined::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("App.tsx"),
    ///     line: 12,
    ///     class_token: "bg-ghost".into(),
    ///     prefix: "bg-".into(),
    ///     suffix: "ghost".into(),
    /// };
    /// assert!(f.message().contains("bg-ghost"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::token_undefined::check;
/// // `allowlist` is loaded from dist/ts-web/design/allowlist.json:
/// let findings = check(Path::new("src"), &allowlist);
/// ```
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
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::token_undefined::check_file;
///
/// let lines = [r#"<div className="bg-ghost" />"#];
/// let findings = check_file(Path::new("App.tsx"), &lines, &allowlist);
/// ```
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
                // Tailwind ring-width / ring-offset-width built-ins
                // (`ring-2`, `ring-inherit`, `ring-offset-4`) are NOT
                // color references — short-circuit them here.
                if is_ring_width_builtin(prefix, suffix) {
                    continue;
                }
                // Empty suffix on a "bare" class (`rounded`, `shadow`) →
                // check the `DEFAULT` slot in the bucket.
                let lookup_suffix = if suffix.is_empty() {
                    "DEFAULT"
                } else {
                    suffix
                };
                let bucket_key = prefix.trim_end_matches('-');
                // Candidate buckets: the primary one parsed from the prefix,
                // plus any ambiguous-overload buckets (e.g. `text-` → also
                // probe `text-size` for the typography scale). The class is
                // allowed when ANY candidate bucket contains the suffix; the
                // diagnostic fires only when NONE do.
                let candidate_buckets: Vec<&str> = std::iter::once(bucket_key)
                    .chain(extra_buckets_for(bucket_key).iter().copied())
                    .collect();
                // Suppress the rule entirely when none of the candidate
                // buckets are known — these prefixes aren't design-token
                // bound in this project.
                if !candidate_buckets.iter().any(|b| allowlist.knows_prefix(b)) {
                    continue;
                }
                if candidate_buckets
                    .iter()
                    .any(|b| allowlist.contains(b, lookup_suffix))
                {
                    continue;
                }
                // Opacity-slash modifier fallback: `bg-brand/90` may not be
                // declared as a literal suffix, but `brand` should be — the
                // `/N` tail is Tailwind's alpha modifier. Try the lookup
                // again with the modifier stripped. This preserves the
                // existing allowlist-extension escape hatch (projects can
                // still list explicit `black/45` entries) while letting
                // declared base tokens resolve under any opacity.
                let stripped = strip_opacity_modifier(lookup_suffix);
                if stripped != lookup_suffix
                    && candidate_buckets
                        .iter()
                        .any(|b| allowlist.contains(b, stripped))
                {
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
    include!("token_undefined_tests.rs");
}
