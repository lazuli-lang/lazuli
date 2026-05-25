//! Recipe model + frontmatter loader. A "recipe" is a single
//! Markdown file under `migrations/recipes/<from>-to-<to>/` whose
//! YAML-ish frontmatter declares one match/replace transformation.
//!
//! Pattern markers are parsed into `PatternToken` (typed by suffix:
//! ws / token / rest) and replacement markers into `ReplaceToken`
//! (literal vs. slot reference). The marker syntax is intentionally
//! narrower than regex — see the parent module's docstring for
//! rationale.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// A loaded recipe, after frontmatter parsing.
#[derive(Debug, Clone)]
pub(super) struct Recipe {
    pub(super) name: String,
    pub(super) applies_to: AppliesTo,
    pub(super) match_pattern: Vec<PatternToken>,
    pub(super) replace_template: Vec<ReplaceToken>,
    #[allow(dead_code)]
    pub(super) description: String,
    /// Source path of the recipe (for diagnostics — surfaced by
    /// future "which recipe failed on which file" reporting).
    #[allow(dead_code)]
    pub(super) source: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppliesTo {
    Lzi,
    Lzx,
    Both,
}

/// One token in a recipe's `match:` pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PatternToken {
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
pub(super) enum ReplaceToken {
    Literal(String),
    Slot(String),
}

pub(super) fn load_recipe_dir(dir: &Path) -> Result<Vec<Recipe>, Box<dyn Error>> {
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
pub(super) fn parse_recipe(raw: &str, source: &Path) -> Result<Recipe, String> {
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
