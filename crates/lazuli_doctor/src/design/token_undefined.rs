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
        let allowlist = al(&[
            ("bg", &["primary"]),
            ("text", &["primary-foreground"]),
        ]);
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
        assert!(f.is_empty(), "rounded → DEFAULT slot lookup; found: {:?}", f);
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

    // ── BB.4 — `text-` prefix ambiguity (color vs typography scale) ───────────
    //
    // The `text-` prefix has dual meaning in Tailwind: `text-<color>` (color
    // bucket) and `text-<size>` (typography scale bucket). The doctor probes
    // BOTH the `text` bucket (color) and the `text-size` bucket (scale); the
    // diagnostic fires only when NEITHER contains the suffix.

    #[test]
    fn text_size_class_resolves_via_scale_bucket() {
        // `text-xs` with the scale bucket containing `xs` → no diagnostic,
        // even when the color bucket has no `xs` entry.
        let lines = vec![r#"<div className="text-xs" />"#];
        let allowlist = al(&[
            ("text", &["primary", "foreground"]),
            ("text-size", &["xs", "sm", "base", "lg", "2xl"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-xs should resolve via text-size; found: {:?}", f);
    }

    #[test]
    fn text_color_class_resolves_via_color_bucket() {
        // `text-primary` with the color bucket containing `primary` → no
        // diagnostic, even when the scale bucket has no `primary` entry.
        let lines = vec![r#"<div className="text-primary" />"#];
        let allowlist = al(&[
            ("text", &["primary", "foreground"]),
            ("text-size", &["xs", "base"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-primary should resolve via text; found: {:?}", f);
    }

    #[test]
    fn text_class_fires_when_in_neither_bucket() {
        // `text-asdf` not in either bucket → diagnostic fires.
        let lines = vec![r#"<div className="text-asdf" />"#];
        let allowlist = al(&[
            ("text", &["primary"]),
            ("text-size", &["xs", "base"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "text");
        assert_eq!(f[0].suffix, "asdf");
    }

    #[test]
    fn text_class_resolves_when_in_only_color_bucket() {
        // Scale bucket missing entirely → fall back to color bucket only.
        let lines = vec![r#"<div className="text-primary" />"#];
        let allowlist = al(&[("text", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn text_class_resolves_when_in_only_scale_bucket() {
        // Color bucket missing entirely → fall back to scale bucket only.
        let lines = vec![r#"<div className="text-base" />"#];
        let allowlist = al(&[("text-size", &["xs", "base", "lg"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn regression_existing_color_class_still_works() {
        // Non-`text-` prefix path must not regress under the bucket fan-out.
        let lines = vec![r#"<div className="bg-primary" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "bg-primary regression; found: {:?}", f);
    }

    // ── CC.4 — opacity-slash modifier + ring/ring-offset routing ────────────
    //
    // Tailwind allows `bg-X/N` for alpha; `ring-N` is a width built-in;
    // `ring-offset-N` is the offset-width built-in; `ring-offset-<color>`
    // shares the `ring` color palette. These tests pin the doctor-side
    // routing so the final 5 hostpoint residuals resolve.

    #[test]
    fn opacity_slash_modifier_stripped_for_color_lookup() {
        let lines = vec![r#"<div className="bg-brand/90 bg-brand/5" />"#];
        let allowlist = al(&[("bg", &["brand"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "opacity-slash on color must resolve; found: {:?}", f);
    }

    #[test]
    fn ring_width_builtin_not_flagged() {
        // `ring-N` digits AND `ring-inherit` are width built-ins, never
        // a color lookup — even when the `ring` color bucket exists.
        let lines = vec![r#"<div className="ring-2 ring-1 ring-4 ring-8 ring-inherit" />"#];
        let allowlist = al(&[("ring", &["primary", "background"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring width built-ins must not fire; found: {:?}", f);
    }

    #[test]
    fn ring_offset_width_builtin_not_flagged() {
        let lines = vec![
            r#"<div className="ring-offset-0 ring-offset-2 ring-offset-4 ring-offset-8 ring-offset-inherit" />"#,
        ];
        let allowlist = al(&[("ring", &["primary", "background"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-offset width built-ins must not fire; found: {:?}", f);
    }

    #[test]
    fn ring_offset_color_resolves_via_color_bucket() {
        // `ring-offset-background` falls through to the `ring` color
        // bucket (Tailwind's ring-offset-color shares the project palette).
        let lines = vec![r#"<div className="ring-offset-background" />"#];
        let allowlist = al(&[("ring", &["primary", "background", "foreground"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-offset-background must resolve via ring bucket; found: {:?}", f);
    }

    #[test]
    fn ring_color_resolves_via_color_bucket() {
        let lines = vec![r#"<div className="ring-primary ring-ring" />"#];
        let allowlist = al(&[("ring", &["primary", "ring"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-<color> must resolve; found: {:?}", f);
    }

    #[test]
    fn unknown_ring_color_fires_diagnostic() {
        // Regression guard: the width/offset relaxation must NOT
        // over-allow arbitrary suffixes. Non-digit, non-`inherit`
        // suffixes still hit the color lookup.
        let lines = vec![r#"<div className="ring-undeclared" />"#];
        let allowlist = al(&[("ring", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "ring");
        assert_eq!(f[0].suffix, "undeclared");
    }

    #[test]
    fn regression_opacity_slash_on_text_class() {
        // `text-primary/50` must still resolve via the text color bucket.
        let lines = vec![r#"<div className="text-primary/50" />"#];
        let allowlist = al(&[("text", &["primary"]), ("text-size", &["base"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-primary/50 must resolve; found: {:?}", f);
    }

    #[test]
    fn opacity_slash_explicit_extension_entry_honored() {
        // The allowlist-extension escape hatch (`allowlist.extension.json`)
        // lets projects declare literal `black/45`-style suffixes for
        // colors that aren't part of the design.lzi palette. The doctor
        // probes the full suffix BEFORE stripping, so these explicit
        // entries continue to allow the class even after the opacity
        // strip fallback was added.
        let lines = vec![r#"<div className="bg-black/45" />"#];
        let allowlist = al(&[("bg", &["primary", "black/45"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "explicit black/45 entry must allow; found: {:?}", f);
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
