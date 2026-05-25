//! Source-file walk + per-file recipe application + parse-safe
//! rollback.
//!
//! Responsibilities:
//! - `walk_lazuli_sources` enumerates `.lzi`/`.lzx` files under the
//!   project root, skipping generated / vendor directories.
//! - `process_file` runs every applicable recipe on a single file in
//!   sequence, then re-parses the rewritten source via
//!   `lazuli_syntax`. If the original parsed but the rewrite no
//!   longer does, the file is rolled back and the error surfaces in
//!   the `DslReport`.
//! - `apply_recipe` / `match_line` / `render_replace` form the inner
//!   loop: line-by-line greedy marker matching with limited
//!   backtracking on `Token` captures.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use super::recipe::{AppliesTo, PatternToken, Recipe, ReplaceToken};
use super::{DslDiff, DslReport};

/// Walk `root` for `.lzi`/`.lzx` files, skipping the well-known
/// generated/cache/dependency directories.
pub(super) fn walk_lazuli_sources(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
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

pub(super) fn process_file(
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

pub(super) fn apply_recipe(source: &str, recipe: &Recipe) -> String {
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
pub(super) fn match_line(line: &str, pattern: &[PatternToken]) -> Option<Vec<(String, String)>> {
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
/// `parse_feature_skeletons` (indentation-based, rich).
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
