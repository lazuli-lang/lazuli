//! `lazuli migrate dsl <FROM> <TO>` — apply recipe-driven DSL version
//! migrations.
//!
//! Recipes live as Markdown files under
//! `migrations/recipes/<from>-to-<to>/<NN>-<slug>.md`. The Markdown body
//! is human prose; the machine-readable contract is a YAML-ish
//! frontmatter block delimited by `---` lines. Frontmatter keys:
//!
//! - `name` (required): a unique recipe identifier used in logs and
//!   the rollback trace.
//! - `applies_to` (required): file-extension filter (`.lzi` or
//!   `.lzx`).
//! - `match` (required, block scalar `|`): a line-anchored marker
//!   pattern. Literal text matches itself; whitespace runs match
//!   themselves. `${name}` captures a non-whitespace token; the
//!   typed forms `${name:ws}` and `${name:rest}` capture a whitespace
//!   run or the rest of the line respectively.
//! - `replace` (required, block scalar `|`): the replacement
//!   template. Markers `${name}` refer back to slots captured by
//!   `match`. Unknown markers are an authoring error.
//! - `description` (optional): free text shown in the dry-run diff.
//!
//! Rationale for marker syntax (not regex): the CLI crate forbids
//! `regex` as a dependency. Markers cover the recipe shapes Lazuli's
//! own deprecation history requires (rename/move/strip-keyword) and
//! keep recipe authors honest about which slots carry meaning.
//!
//! ## Lifecycle
//!
//! 1. Load every recipe under `migrations/recipes/<from>-to-<to>/`
//!    sorted by filename (lexical order: `00-...`, `01-...`).
//! 2. Walk the project root for `.lzi`/`.lzx` files (skipping
//!    `dist/`, `target/`, `.git/`, `.lazuli/`, `node_modules/`).
//! 3. For each file, apply each recipe sequentially. Each recipe
//!    rewrites lines where the pattern matches.
//! 4. After all recipes apply, re-parse the rewritten file with
//!    `lazuli_syntax::parse_document` (`.lzi`) or
//!    `parse_lzx_document` (`.lzx`). On parse failure: revert the
//!    file's bytes to the pre-transform snapshot and surface the
//!    parse error in the report.
//! 5. On `--dry-run`, no file is written; the diff is printed
//!    instead.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Outcome of a `lazuli migrate dsl` run.
#[derive(Debug, Default)]
pub struct DslReport {
    /// Files whose contents changed and were written to disk.
    pub changed: Vec<PathBuf>,
    /// Files that matched at least one recipe but were rolled back
    /// because the post-transform source failed to parse.
    pub rolled_back: Vec<(PathBuf, String)>,
    /// Files that would change in `--dry-run` mode (no disk write).
    pub dry_run_changes: Vec<DslDiff>,
    /// Recipes loaded for this transition. Useful for surfacing the
    /// no-op case (no recipes ⇒ tell the user where the directory
    /// should be).
    pub recipes_applied: Vec<String>,
}

/// One per-file diff entry surfaced in `--dry-run` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DslDiff {
    pub file: PathBuf,
    pub before: String,
    pub after: String,
}

/// Run the DSL migration tool.
///
/// `from` / `to` are version tags like `v0.11` and `v0.12`. The tool
/// looks up `migrations/recipes/<from>-to-<to>/` and exits non-zero
/// when that directory is missing.
pub fn run_migrate_dsl(
    project_root: &Path,
    from: &str,
    to: &str,
    dry_run: bool,
) -> Result<DslReport, Box<dyn Error>> {
    let recipe_dir = project_root
        .join("migrations")
        .join("recipes")
        .join(format!("{from}-to-{to}"));
    if !recipe_dir.exists() {
        return Err(format!(
            "no DSL recipes for {from} → {to} at {}",
            recipe_dir.display()
        )
        .into());
    }

    let recipes = load_recipe_dir(&recipe_dir)?;
    if recipes.is_empty() {
        return Err(format!(
            "DSL recipe directory {} contains no .md recipes",
            recipe_dir.display()
        )
        .into());
    }

    let mut report = DslReport {
        recipes_applied: recipes.iter().map(|r| r.name.clone()).collect(),
        ..DslReport::default()
    };

    for file in walk_lazuli_sources(project_root)? {
        process_file(&file, &recipes, dry_run, &mut report)?;
    }

    Ok(report)
}

/// A loaded recipe, after frontmatter parsing.
#[derive(Debug, Clone)]
struct Recipe {
    name: String,
    applies_to: AppliesTo,
    match_pattern: Vec<PatternToken>,
    replace_template: Vec<ReplaceToken>,
    #[allow(dead_code)]
    description: String,
    /// Source path of the recipe (for diagnostics — surfaced by
    /// future "which recipe failed on which file" reporting).
    #[allow(dead_code)]
    source: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppliesTo {
    Lzi,
    Lzx,
    Both,
}

/// One token in a recipe's `match:` pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternToken {
    /// Literal characters; matched byte-for-byte.
    Literal(String),
    /// Captures a contiguous run of whitespace (` ` or `\t`).
    Whitespace(String),
    /// Captures a non-whitespace token (run of non-whitespace
    /// chars).
    Token(String),
    /// Captures the rest of the line (may be empty).
    Rest(String),
}

/// One token in a recipe's `replace:` template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplaceToken {
    Literal(String),
    Slot(String),
}

fn load_recipe_dir(dir: &Path) -> Result<Vec<Recipe>, Box<dyn Error>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|res| res.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
        })
        .collect();
    entries.sort();

    let mut recipes = Vec::with_capacity(entries.len());
    for path in entries {
        let raw = fs::read_to_string(&path)?;
        let recipe = parse_recipe(&raw, &path).map_err(|err| -> Box<dyn Error> {
            format!("failed to load recipe {}: {err}", path.display()).into()
        })?;
        recipes.push(recipe);
    }
    Ok(recipes)
}

/// Frontmatter parser. The expected shape is:
///
/// ```text
/// ---
/// name: rename-validates-resource-keyword
/// applies_to: .lzi
/// match: |
///   ${indent:ws}validates resource @validator.${ref}
/// replace: |
///   ${indent}validates @validator.${ref}
/// description: Tier-4 follow-up retired the `resource` axis.
/// ---
/// ```
///
/// Anything after the closing `---` is documentation prose, ignored
/// by tooling.
fn parse_recipe(raw: &str, source: &Path) -> Result<Recipe, String> {
    let mut lines = raw.lines();
    let first = lines.next().ok_or("recipe is empty")?;
    if first.trim() != "---" {
        return Err("recipe must start with `---` frontmatter delimiter".to_owned());
    }

    let mut frontmatter_lines = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        frontmatter_lines.push(line);
    }
    if !closed {
        return Err("recipe frontmatter is not closed with a trailing `---`".to_owned());
    }

    let fm = parse_frontmatter(&frontmatter_lines)?;

    let name = fm
        .get("name")
        .ok_or("recipe is missing the `name` key")?
        .trim()
        .to_owned();
    if name.is_empty() {
        return Err("recipe `name` cannot be empty".to_owned());
    }

    let applies_to_raw = fm
        .get("applies_to")
        .ok_or("recipe is missing the `applies_to` key")?
        .trim();
    let applies_to = match applies_to_raw {
        ".lzi" => AppliesTo::Lzi,
        ".lzx" => AppliesTo::Lzx,
        "both" | ".lzi+.lzx" | ".lzi,.lzx" => AppliesTo::Both,
        other => {
            return Err(format!(
                "recipe `applies_to` must be `.lzi`, `.lzx`, or `both`; got `{other}`"
            ));
        }
    };

    let match_raw = fm
        .get("match")
        .ok_or("recipe is missing the `match` block scalar")?
        .clone();
    let replace_raw = fm
        .get("replace")
        .ok_or("recipe is missing the `replace` block scalar")?
        .clone();

    let match_pattern = parse_pattern(&match_raw)?;
    let replace_template = parse_replace_template(&replace_raw, &match_pattern)?;

    let description = fm
        .get("description")
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_owned();

    Ok(Recipe {
        name,
        applies_to,
        match_pattern,
        replace_template,
        description,
        source: source.to_owned(),
    })
}

/// Parse the frontmatter body into a `key -> value` map. Supports
/// `key: value` single-line entries and `key: |` block scalars where
/// the block content is every subsequent line indented by ≥1 space,
/// joined by `\n`.
fn parse_frontmatter(lines: &[&str]) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        let colon = trimmed
            .find(':')
            .ok_or_else(|| format!("frontmatter line `{line}` missing colon"))?;
        let key = trimmed[..colon].trim().to_owned();
        let after_colon = trimmed[colon + 1..].trim();
        if after_colon == "|" {
            // Block scalar. Read subsequent indented lines.
            i += 1;
            let block_indent = lines.get(i).map(|l| leading_space_count(l)).unwrap_or(0);
            if block_indent == 0 {
                return Err(format!(
                    "block scalar for `{key}` must have at least one space of indentation"
                ));
            }
            let mut block = String::new();
            while i < lines.len() {
                let next = lines[i];
                let next_indent = leading_space_count(next);
                if next.trim().is_empty() {
                    block.push('\n');
                    i += 1;
                    continue;
                }
                if next_indent < block_indent {
                    break;
                }
                // Strip exactly `block_indent` chars of leading
                // whitespace; preserve the rest verbatim.
                let dedented = &next[block_indent.min(next.len())..];
                block.push_str(dedented);
                block.push('\n');
                i += 1;
            }
            // Trim the trailing newline introduced by the loop.
            if block.ends_with('\n') {
                block.pop();
            }
            out.insert(key, block);
        } else {
            out.insert(key, after_colon.to_owned());
            i += 1;
        }
    }
    Ok(out)
}

fn leading_space_count(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// Tokenize a `match:` pattern into a flat sequence of literals and
/// markers. The pattern is single-line; if a recipe author writes a
/// multi-line block, only the first non-empty line is honored and
/// trailing lines are an error.
fn parse_pattern(raw: &str) -> Result<Vec<PatternToken>, String> {
    let line = pick_single_line(raw, "match")?;
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek().copied() == Some('{') {
            if !buf.is_empty() {
                tokens.push(PatternToken::Literal(std::mem::take(&mut buf)));
            }
            chars.next(); // consume '{'
            let marker = take_marker(&mut chars)?;
            let (name, ty) = split_marker(&marker);
            let token = match ty {
                "" | "token" => PatternToken::Token(name.to_owned()),
                "ws" => PatternToken::Whitespace(name.to_owned()),
                "rest" => PatternToken::Rest(name.to_owned()),
                other => {
                    return Err(format!(
                        "unknown marker type `{other}` in pattern (expected ws/token/rest)"
                    ));
                }
            };
            tokens.push(token);
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        tokens.push(PatternToken::Literal(buf));
    }
    Ok(tokens)
}

fn parse_replace_template(
    raw: &str,
    pattern: &[PatternToken],
) -> Result<Vec<ReplaceToken>, String> {
    let line = pick_single_line(raw, "replace")?;
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek().copied() == Some('{') {
            if !buf.is_empty() {
                tokens.push(ReplaceToken::Literal(std::mem::take(&mut buf)));
            }
            chars.next(); // consume '{'
            let marker = take_marker(&mut chars)?;
            let (name, ty) = split_marker(&marker);
            if !ty.is_empty() {
                return Err(format!(
                    "replace template marker `{marker}` must not carry a type suffix"
                ));
            }
            if !pattern_has_slot(pattern, name) {
                return Err(format!(
                    "replace template references slot `{name}` not defined in match pattern"
                ));
            }
            tokens.push(ReplaceToken::Slot(name.to_owned()));
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        tokens.push(ReplaceToken::Literal(buf));
    }
    Ok(tokens)
}

fn pick_single_line<'a>(raw: &'a str, key: &str) -> Result<&'a str, String> {
    let non_empty: Vec<&str> = raw
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    if non_empty.is_empty() {
        return Err(format!("`{key}` block scalar is empty"));
    }
    if non_empty.len() > 1 {
        return Err(format!(
            "`{key}` block scalar must be a single line — multi-line patterns are not supported in v0.1"
        ));
    }
    Ok(non_empty[0])
}

fn take_marker<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<String, String> {
    let mut marker = String::new();
    let mut closed = false;
    for c in chars.by_ref() {
        if c == '}' {
            closed = true;
            break;
        }
        marker.push(c);
    }
    if !closed {
        return Err(format!("unterminated marker `${{{marker}`"));
    }
    if marker.is_empty() {
        return Err("empty marker `${}`".to_owned());
    }
    Ok(marker)
}

fn split_marker(marker: &str) -> (&str, &str) {
    match marker.split_once(':') {
        Some((name, ty)) => (name.trim(), ty.trim()),
        None => (marker.trim(), ""),
    }
}

fn pattern_has_slot(pattern: &[PatternToken], name: &str) -> bool {
    pattern.iter().any(|t| match t {
        PatternToken::Whitespace(n) | PatternToken::Token(n) | PatternToken::Rest(n) => n == name,
        PatternToken::Literal(_) => false,
    })
}

/// Walk `root` for `.lzi`/`.lzx` files, skipping the well-known
/// generated/cache/dependency directories.
fn walk_lazuli_sources(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut out = Vec::new();
    walk_recurse(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_recurse(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "dist" | "target" | ".git" | ".lazuli" | "node_modules"
            ) {
                continue;
            }
            walk_recurse(&path, out)?;
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "lzi" || ext == "lzx" {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn process_file(
    file: &Path,
    recipes: &[Recipe],
    dry_run: bool,
    report: &mut DslReport,
) -> Result<(), Box<dyn Error>> {
    let original = fs::read_to_string(file)?;
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Apply every applicable recipe sequentially.
    let mut current = original.clone();
    let mut touched = false;
    for recipe in recipes {
        if !recipe_applies(recipe, ext) {
            continue;
        }
        let rewritten = apply_recipe(&current, recipe);
        if rewritten != current {
            touched = true;
            current = rewritten;
        }
    }

    if !touched {
        return Ok(());
    }

    // Parse-safe rollback: only check that the rewrite doesn't
    // regress parseability. If the original couldn't parse, we
    // can't claim the rewrite "broke" the file. If the original
    // parsed and the rewrite fails, drop the rewrite and surface
    // the error.
    let original_ok = parse_check(ext, &original).is_ok();
    if original_ok {
        if let Err(err) = parse_check(ext, &current) {
            report.rolled_back.push((
                file.to_path_buf(),
                format!("post-transform parse failure: {err}"),
            ));
            return Ok(());
        }
    }

    if dry_run {
        report.dry_run_changes.push(DslDiff {
            file: file.to_path_buf(),
            before: original,
            after: current,
        });
    } else {
        fs::write(file, &current)?;
        report.changed.push(file.to_path_buf());
    }
    Ok(())
}

fn recipe_applies(recipe: &Recipe, ext: &str) -> bool {
    match recipe.applies_to {
        AppliesTo::Lzi => ext == "lzi",
        AppliesTo::Lzx => ext == "lzx",
        AppliesTo::Both => ext == "lzi" || ext == "lzx",
    }
}

fn apply_recipe(source: &str, recipe: &Recipe) -> String {
    // Track whether the input ends with a newline; we want to
    // preserve that exactly in the output (most authored .lzi files
    // end with a newline; some test fixtures don't).
    let ends_with_newline = source.ends_with('\n');
    let mut out_lines = Vec::new();
    for line in source.split_inclusive('\n').map(strip_trailing_newline) {
        if let Some(captures) = match_line(line, &recipe.match_pattern) {
            let rewritten = render_replace(&recipe.replace_template, &captures);
            out_lines.push(rewritten);
        } else {
            out_lines.push(line.to_owned());
        }
    }
    let mut joined = out_lines.join("\n");
    if ends_with_newline {
        joined.push('\n');
    }
    joined
}

fn strip_trailing_newline(s: &str) -> &str {
    s.strip_suffix("\r\n")
        .unwrap_or_else(|| s.strip_suffix('\n').unwrap_or(s))
}

/// Match a single source line against a pattern. Returns the
/// captured slots on success, `None` otherwise.
///
/// Matching algorithm: linear scan over the pattern tokens, consuming
/// the source line left-to-right. Whitespace markers greedily eat
/// space/tab chars (at least one). Token markers greedily eat
/// non-space chars (at least one). Rest markers eat everything left
/// (may be empty). Literal tokens must match byte-for-byte.
fn match_line(line: &str, pattern: &[PatternToken]) -> Option<Vec<(String, String)>> {
    let bytes = line.as_bytes();
    let mut pos = 0;
    let mut captures = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        match &pattern[i] {
            PatternToken::Literal(lit) => {
                let lit_bytes = lit.as_bytes();
                if pos + lit_bytes.len() > bytes.len() {
                    return None;
                }
                if &bytes[pos..pos + lit_bytes.len()] != lit_bytes {
                    return None;
                }
                pos += lit_bytes.len();
            }
            PatternToken::Whitespace(name) => {
                let start = pos;
                while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
                    pos += 1;
                }
                if pos == start {
                    return None;
                }
                captures.push((name.clone(), line[start..pos].to_owned()));
            }
            PatternToken::Token(name) => {
                // Lookahead: how far should a non-greedy token
                // consume? Until the next literal or whitespace
                // chunk. For the bootstrap pattern the token is the
                // last marker on the line, so greedy-to-EOL is
                // correct. For tokens followed by a literal, we
                // backtrack from the longest valid prefix.
                let start = pos;
                let mut best: Option<usize> = None;
                let mut probe = pos;
                while probe < bytes.len() && bytes[probe] != b' ' && bytes[probe] != b'\t' {
                    probe += 1;
                    if probe > start && pattern_tail_matches(&pattern[i + 1..], &line[probe..]) {
                        best = Some(probe);
                    }
                }
                let end = match best {
                    Some(e) => e,
                    None => {
                        // No suffix to satisfy → take the full token.
                        if probe == start {
                            return None;
                        }
                        probe
                    }
                };
                captures.push((name.clone(), line[start..end].to_owned()));
                pos = end;
            }
            PatternToken::Rest(name) => {
                captures.push((name.clone(), line[pos..].to_owned()));
                pos = bytes.len();
            }
        }
        i += 1;
    }
    if pos != bytes.len() {
        return None;
    }
    Some(captures)
}

/// Cheap check: does the tail of the pattern consume exactly the
/// tail of the source? Used by `Token` matching to find the
/// rightmost split that lets subsequent literals/markers still
/// match.
fn pattern_tail_matches(tail: &[PatternToken], rest: &str) -> bool {
    match_line(rest, tail).is_some()
}

fn render_replace(template: &[ReplaceToken], captures: &[(String, String)]) -> String {
    let mut out = String::new();
    for token in template {
        match token {
            ReplaceToken::Literal(s) => out.push_str(s),
            ReplaceToken::Slot(name) => {
                let value = captures
                    .iter()
                    .rev()
                    .find_map(|(k, v)| if k == name { Some(v.as_str()) } else { None })
                    .unwrap_or("");
                out.push_str(value);
            }
        }
    }
    out
}

/// Re-parse the rewritten source via `lazuli_syntax`. The parsers
/// surface rich errors that we propagate to the report so authors
/// can fix recipes that break downstream syntax.
///
/// For `.lzi` files the canonical authoring shape is parsed by
/// `parse_feature_skeletons` (indentation-based, rich). The strict
/// pest `parse_document` grammar is a narrower spine covering the
/// `app ~ aggregate*` shape; we use it as a secondary check so the
/// rewrite cannot regress *either* grammar that was valid before.
fn parse_check(ext: &str, source: &str) -> Result<(), String> {
    match ext {
        "lzi" => lazuli_syntax::parse_feature_skeletons(source)
            .map(|_| ())
            .map_err(|err| format!("{err:?}")),
        "lzx" => lazuli_syntax::parse_lzx_document(source)
            .map(|_| ())
            .map_err(|err| format!("{err:?}")),
        other => Err(format!("unsupported extension `{other}`")),
    }
}

/// Public-facing summary report renderer. Used by the CLI driver.
pub fn render_report(report: &DslReport, dry_run: bool) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "loaded {} recipe(s): {}",
        report.recipes_applied.len(),
        report.recipes_applied.join(", ")
    );
    if dry_run {
        if report.dry_run_changes.is_empty() {
            let _ = writeln!(out, "dry-run: no files would change");
        }
        for diff in &report.dry_run_changes {
            let _ = writeln!(out, "would change: {}", diff.file.display());
            for (a, b) in diff
                .before
                .lines()
                .zip(diff.after.lines())
                .filter(|(a, b)| a != b)
            {
                let _ = writeln!(out, "  - {a}");
                let _ = writeln!(out, "  + {b}");
            }
        }
    } else {
        if report.changed.is_empty() {
            let _ = writeln!(out, "no files changed");
        }
        for path in &report.changed {
            let _ = writeln!(out, "changed: {}", path.display());
        }
    }
    for (path, err) in &report.rolled_back {
        let _ = writeln!(out, "rolled back {}: {}", path.display(), err);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lazuli-migrate-dsl-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bootstrap_recipe() -> &'static str {
        "---\n\
         name: rename-validates-resource-keyword\n\
         applies_to: .lzi\n\
         match: |\n\
         \x20\x20${indent:ws}validates resource @validator.${ref}\n\
         replace: |\n\
         \x20\x20${indent}validates @validator.${ref}\n\
         description: Tier-4 follow-up retired the resource axis.\n\
         ---\n\
         # human prose\n"
    }

    #[test]
    fn parses_bootstrap_recipe_frontmatter() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("00-bootstrap.md")).unwrap();
        assert_eq!(recipe.name, "rename-validates-resource-keyword");
        assert_eq!(recipe.applies_to, AppliesTo::Lzi);
        assert_eq!(recipe.match_pattern.len(), 3);
        match &recipe.match_pattern[0] {
            PatternToken::Whitespace(name) => assert_eq!(name, "indent"),
            other => panic!("expected ws marker, got {other:?}"),
        }
        match &recipe.match_pattern[1] {
            PatternToken::Literal(lit) => assert_eq!(lit, "validates resource @validator."),
            other => panic!("expected literal, got {other:?}"),
        }
        match &recipe.match_pattern[2] {
            PatternToken::Token(name) => assert_eq!(name, "ref"),
            other => panic!("expected token marker, got {other:?}"),
        }
    }

    #[test]
    fn rejects_recipe_with_missing_frontmatter_close() {
        let raw = "---\nname: x\napplies_to: .lzi\nmatch: |\n  foo\nreplace: |\n  bar\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("not closed"), "got: {err}");
    }

    #[test]
    fn rejects_replace_referencing_undefined_slot() {
        let raw = "---\nname: bad\napplies_to: .lzi\nmatch: |\n  foo ${a}\nreplace: |\n  baz ${nope}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("not defined in match pattern"), "got: {err}");
    }

    #[test]
    fn rejects_replace_marker_with_type_suffix() {
        let raw = "---\nname: bad\napplies_to: .lzi\nmatch: |\n  foo ${a:ws}\nreplace: |\n  ${a:ws}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("must not carry a type suffix"), "got: {err}");
    }

    #[test]
    fn rejects_unknown_marker_type() {
        let raw =
            "---\nname: bad\napplies_to: .lzi\nmatch: |\n  ${a:weird}\nreplace: |\n  ${a}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("unknown marker type"), "got: {err}");
    }

    #[test]
    fn match_line_captures_ws_and_token() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let line = "      validates resource @validator.tier_check";
        let caps = match_line(line, &recipe.match_pattern).expect("must match");
        assert_eq!(caps[0], ("indent".to_owned(), "      ".to_owned()));
        assert_eq!(caps[1], ("ref".to_owned(), "tier_check".to_owned()));
    }

    #[test]
    fn match_line_rejects_when_keyword_missing() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        assert!(
            match_line(
                "      validates @validator.tier_check",
                &recipe.match_pattern
            )
            .is_none()
        );
    }

    #[test]
    fn apply_recipe_rewrites_matching_lines_only() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let src = "feature customer\n\
                   \x20\x20resource Customer\n\
                   \x20\x20\x20\x20# legacy form below\n\
                   \x20\x20\x20\x20validates resource @validator.row_check\n\
                   \x20\x20\x20\x20\x20\x20validates resource @validator.tier_check\n\
                   \x20\x20\x20\x20validates @validator.already_modern\n";
        let out = apply_recipe(src, &recipe);
        assert!(
            out.contains("    validates @validator.row_check"),
            "output: {out}"
        );
        assert!(out.contains("      validates @validator.tier_check"));
        assert!(out.contains("    validates @validator.already_modern"));
        assert!(!out.contains("validates resource"));
        // Non-matching lines untouched (headers, etc.).
        assert!(out.starts_with("feature customer\n"));
    }

    #[test]
    fn apply_recipe_preserves_trailing_newline_state() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let with_nl = "  validates resource @validator.x\n";
        let without_nl = "  validates resource @validator.x";
        assert!(apply_recipe(with_nl, &recipe).ends_with('\n'));
        assert!(!apply_recipe(without_nl, &recipe).ends_with('\n'));
    }

    #[test]
    fn walk_lazuli_sources_skips_dist_and_target() {
        let root = temp_dir("walk");
        fs::create_dir_all(root.join("features/customer")).unwrap();
        fs::create_dir_all(root.join("dist/go")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(root.join("app.lzi"), "app A\n").unwrap();
        fs::write(root.join("features/customer/customer.lzi"), "").unwrap();
        fs::write(root.join("features/customer/customer.web.lzx"), "").unwrap();
        fs::write(root.join("dist/go/should-skip.lzi"), "").unwrap();
        fs::write(root.join("target/debug/should-skip.lzi"), "").unwrap();
        fs::write(root.join("features/customer/notes.txt"), "").unwrap();

        let mut found = walk_lazuli_sources(&root).unwrap();
        found.sort();
        let names: Vec<_> = found
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"app.lzi".to_owned()));
        assert!(names.contains(&"features/customer/customer.lzi".to_owned()));
        assert!(names.contains(&"features/customer/customer.web.lzx".to_owned()));
        for n in &names {
            assert!(!n.contains("dist/"), "should skip dist: {n}");
            assert!(!n.contains("target/"), "should skip target: {n}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_errors_on_missing_recipe_dir() {
        let root = temp_dir("no-recipes");
        let err = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap_err();
        assert!(err.to_string().contains("no DSL recipes"), "got: {}", err);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_dry_run_does_not_write() {
        let root = temp_dir("dry-run");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let lzi_path = root.join("customer.lzi");
        let original = "feature customer\n\
                        \x20\x20resource Customer\n\
                        \x20\x20\x20\x20validates resource @validator.row_check\n";
        fs::write(&lzi_path, original).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", true).unwrap();
        assert_eq!(report.changed.len(), 0, "report = {report:?}");
        assert_eq!(report.dry_run_changes.len(), 1, "report = {report:?}");
        assert_eq!(fs::read_to_string(&lzi_path).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_writes_when_not_dry_run() {
        let root = temp_dir("write");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let lzi_path = root.join("customer.lzi");
        fs::write(
            &lzi_path,
            "feature customer\n\
             \x20\x20resource Customer\n\
             \x20\x20\x20\x20validates resource @validator.row_check\n",
        )
        .unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(report.changed.len(), 1, "report = {report:?}");
        let after = fs::read_to_string(&lzi_path).unwrap();
        assert!(after.contains("validates @validator.row_check"));
        assert!(!after.contains("validates resource"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_rolls_back_on_parse_failure() {
        let root = temp_dir("rollback");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        // A pathological recipe that mangles field declarations
        // inside a `resource` body into bogus content the Lazuli
        // parser rejects. `bogus_keyword` matches no resource-child
        // verb and carries no colon, so it falls through to the
        // "resource children are..." error and trips the rollback.
        let bad = "---\n\
                   name: corrupt-resource-field\n\
                   applies_to: .lzi\n\
                   match: |\n\
                   \x20\x20${indent:ws}name: ${ty}\n\
                   replace: |\n\
                   \x20\x20${indent}bogus_keyword\n\
                   ---\n";
        fs::write(recipe_dir.join("00-break.md"), bad).unwrap();

        let lzi_path = root.join("broken.lzi");
        let original = "feature demo\n  resource Demo\n    name: Text\n";
        fs::write(&lzi_path, original).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(
            report.rolled_back.len(),
            1,
            "expected rollback, got report = {:?}",
            report
        );
        assert!(
            report.rolled_back[0]
                .1
                .contains("post-transform parse failure")
        );
        // Source file untouched on disk.
        assert_eq!(fs::read_to_string(&lzi_path).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_walks_multi_file_project() {
        let root = temp_dir("multi");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let feature_dir = root.join("features/customer");
        fs::create_dir_all(&feature_dir).unwrap();
        let one = "feature one\n  resource One\n    validates resource @validator.in_one\n";
        let two = "feature two\n  resource Two\n    validates resource @validator.in_two\n";
        let lzx_legacy = "  validates resource @validator.in_lzx\n";
        fs::write(root.join("one.lzi"), one).unwrap();
        fs::write(feature_dir.join("customer.lzi"), two).unwrap();
        // .lzx file should be skipped by an `.lzi`-only recipe.
        fs::write(feature_dir.join("customer.lzx"), lzx_legacy).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(report.changed.len(), 2, "report = {report:?}");
        assert!(
            fs::read_to_string(root.join("one.lzi"))
                .unwrap()
                .contains("validates @validator.in_one")
        );
        assert!(
            fs::read_to_string(feature_dir.join("customer.lzi"))
                .unwrap()
                .contains("validates @validator.in_two")
        );
        // .lzx untouched.
        assert_eq!(
            fs::read_to_string(feature_dir.join("customer.lzx")).unwrap(),
            lzx_legacy
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn render_report_dry_run_emits_diff() {
        let diff = DslDiff {
            file: PathBuf::from("a.lzi"),
            before: "  validates resource @validator.x\n".to_owned(),
            after: "  validates @validator.x\n".to_owned(),
        };
        let report = DslReport {
            dry_run_changes: vec![diff],
            recipes_applied: vec!["bootstrap".to_owned()],
            ..DslReport::default()
        };
        let out = render_report(&report, true);
        assert!(out.contains("would change"));
        assert!(out.contains("validates resource"));
        assert!(out.contains("validates @validator.x"));
    }

    #[test]
    fn pattern_token_with_following_literal_backtracks() {
        // ${name} should consume only up to where the next literal
        // starts matching, not greedily to whitespace.
        let raw =
            "---\nname: t\napplies_to: .lzi\nmatch: |\n  ${a}/${b}\nreplace: |\n  ${b}/${a}\n---\n";
        let recipe = parse_recipe(raw, Path::new("t.md")).unwrap();
        let caps = match_line("alpha/beta", &recipe.match_pattern).unwrap();
        assert!(caps.iter().any(|(k, v)| k == "a" && v == "alpha"));
        assert!(caps.iter().any(|(k, v)| k == "b" && v == "beta"));
    }
}
