//! Generic file-write helper + `go.work` entry-preserving writer.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor).
//!
//! - [`write_generated_file`]: generic relative-path write with
//!   `mkdir -p` semantics. Used by every emitter that writes under
//!   `dist/`.
//! - [`write_go_work_preserving_entries`]: when the project already
//!   has a hand-edited `go.work` (e.g. extra `use` entries pointing at
//!   sibling modules), merge the codegen's required entries into the
//!   existing file instead of overwriting. New entries are inserted
//!   at the bottom of the existing `use ( … )` block; if no block
//!   exists, a fresh one is appended.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub(crate) fn write_generated_file(root: &Path, relative: &str, contents: &str) -> Result<()> {
    let path = root.join(relative);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn write_go_work_preserving_entries(
    project_root: &Path,
    generated_contents: &str,
) -> Result<()> {
    let path = project_root.join("go.work");
    let required_entries = extract_go_work_use_entries(generated_contents);

    if !path.exists() {
        write_generated_file(project_root, "go.work", generated_contents)?;
        return Ok(());
    }

    let original =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    // SPEC 0030 — normalize the runtime wiring BEFORE merging `use`
    // entries. The preserve-merge below only ADDS missing `use` lines; it
    // never touches `replace` directives. A pilot that hand-added an
    // absolute `replace lazuli.dev/runtime => C:/...` into go.work (the
    // pauta BT-01 footgun) would otherwise keep that stale absolute line
    // forever — and a `replace` WINS over a `use` in Go resolution, so
    // the build stays non-portable and `RUNTIME-WIRING-ABSOLUTE-PATH-001`
    // keeps firing. Codegen resolves the runtime via the relative `use`
    // entry, so the standalone runtime `replace` in go.work is never
    // emitted by us and is always stale; strip it on every regen.
    let original = strip_runtime_replace_directive(&original);
    let updated = add_missing_go_work_use_entries(&original, &required_entries);
    fs::write(&path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Remove any `replace lazuli.dev/runtime => <path>` directive (and a
/// `replace (` block entry of the same module) from a `go.work` body.
///
/// Codegen wires the local runtime through a relative `use` entry, never
/// a go.work `replace`. A leftover `replace lazuli.dev/runtime` — almost
/// always the hand-pasted absolute path from before SPEC 0030 — is
/// stale, shadows the portable `use` entry, and trips the absolute-path
/// gate. We drop it so each regen produces a portable file with no hand
/// editing.
///
/// Both the single-line form
/// (`replace lazuli.dev/runtime => C:/.../runtime/go`) and the
/// block form
/// (`replace (\n  lazuli.dev/runtime => C:/...\n)`) are handled; an
/// emptied `replace (...)` block is removed entirely.
pub(crate) fn strip_runtime_replace_directive(original: &str) -> String {
    const MODULE: &str = "lazuli.dev/runtime";
    let mut out_lines: Vec<&str> = Vec::new();
    let mut in_replace_block = false;
    let mut block_buf: Vec<&str> = Vec::new();
    let mut block_kept_any = false;

    for line in original.lines() {
        let trimmed = line.trim();
        if in_replace_block {
            if trimmed == ")" {
                in_replace_block = false;
                if block_kept_any {
                    out_lines.append(&mut block_buf);
                    out_lines.push(line);
                } else {
                    // Whole block was just the runtime replace → drop it
                    // (including the opening `replace (` we buffered).
                    block_buf.clear();
                }
                block_buf.clear();
                block_kept_any = false;
                continue;
            }
            // Inside a `replace ( ... )` block: drop the runtime line,
            // keep everything else.
            if is_runtime_replace_entry(trimmed, MODULE) {
                continue;
            }
            block_buf.push(line);
            block_kept_any = true;
            continue;
        }

        if trimmed == "replace (" {
            // Buffer until we know whether the block has survivors.
            in_replace_block = true;
            block_buf.clear();
            block_buf.push(line);
            block_kept_any = false;
            continue;
        }

        // Single-line form: `replace lazuli.dev/runtime => <path>`.
        if let Some(rest) = trimmed.strip_prefix("replace ")
            && is_runtime_replace_entry(rest, MODULE)
        {
            continue;
        }

        out_lines.push(line);
    }

    // Unterminated block (malformed input) — flush what we buffered so we
    // never silently delete more than the runtime line.
    if in_replace_block {
        out_lines.append(&mut block_buf);
    }

    let mut result = out_lines.join("\n");
    if original.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// True when a (trimmed) replace clause body — `lazuli.dev/runtime => …`
/// or, inside a block, the same — targets the runtime module.
fn is_runtime_replace_entry(clause: &str, module: &str) -> bool {
    let clause = clause.trim();
    // Must be `<module> => <path>` (with optional version on the LHS,
    // which the runtime replace never carries, but be liberal).
    let Some((lhs, _rhs)) = clause.split_once("=>") else {
        return false;
    };
    lhs.split_whitespace().next() == Some(module)
}

pub(crate) fn add_missing_go_work_use_entries(
    original: &str,
    required_entries: &[String],
) -> String {
    let existing_entries = extract_go_work_use_entries(original);
    let missing_entries: Vec<&str> = required_entries
        .iter()
        .map(String::as_str)
        .filter(|entry| !existing_entries.iter().any(|existing| existing == entry))
        .collect();

    if missing_entries.is_empty() {
        return original.to_owned();
    }

    if let Some((close_idx, entry_indent)) = find_go_work_use_block_close(original) {
        let inserted = missing_entries
            .iter()
            .map(|entry| format!("{entry_indent}{entry}\n"))
            .collect::<String>();
        let (head, tail) = original.split_at(close_idx);
        return format!("{head}{inserted}{tail}");
    }

    let mut updated = original.trim_end().to_owned();
    updated.push_str("\n\nuse (\n");
    for entry in missing_entries {
        updated.push('\t');
        updated.push_str(entry);
        updated.push('\n');
    }
    updated.push_str(")\n");
    updated
}

pub(crate) fn extract_go_work_use_entries(contents: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut in_use_block = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if in_use_block {
            if trimmed == ")" {
                in_use_block = false;
                continue;
            }
            if let Some(entry) = go_work_entry_from_line(trimmed) {
                entries.push(entry);
            }
            continue;
        }

        if trimmed == "use (" {
            in_use_block = true;
            continue;
        }

        if let Some(entry) = trimmed.strip_prefix("use ")
            && entry.trim() != "("
            && let Some(entry) = go_work_entry_from_line(entry.trim())
        {
            entries.push(entry);
        }
    }

    entries
}

fn find_go_work_use_block_close(contents: &str) -> Option<(usize, String)> {
    let mut in_use_block = false;
    let mut entry_indent: Option<String> = None;
    let mut offset = 0;

    for line in contents.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\r', '\n']);
        let trimmed = raw.trim();

        if in_use_block {
            if trimmed == ")" {
                return Some((offset, entry_indent.unwrap_or_else(|| "\t".to_owned())));
            }
            if entry_indent.is_none() && go_work_entry_from_line(trimmed).is_some() {
                entry_indent = Some(raw.chars().take_while(|c| c.is_whitespace()).collect());
            }
        } else if trimmed == "use (" {
            in_use_block = true;
        }

        offset += line.len();
    }

    None
}

fn go_work_entry_from_line(line: &str) -> Option<String> {
    let entry = line
        .split_once("//")
        .map_or(line, |(entry, _)| entry)
        .trim();
    if entry.is_empty() || entry.starts_with("//") {
        None
    } else {
        Some(entry.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_runtime_replace_removes_single_line_absolute() {
        let original = "\
go 1.26.0

use (
\t.
\t./dist/go
\t../../lazuli/runtime/go
)

replace lazuli.dev/runtime => C:/Users/lucas/lazuli/runtime/go
";
        let stripped = strip_runtime_replace_directive(original);
        assert!(
            !stripped.contains("replace lazuli.dev/runtime"),
            "stale runtime replace must be stripped:\n{stripped}"
        );
        // The portable `use` entry is preserved untouched.
        assert!(stripped.contains("../../lazuli/runtime/go"));
        assert!(stripped.contains("./dist/go"));
    }

    #[test]
    fn strip_runtime_replace_removes_block_entry_keeps_others() {
        let original = "\
go 1.26.0

use (
\t.
)

replace (
\tlazuli.dev/runtime => C:/Users/lucas/lazuli/runtime/go
\texample.com/other => ../other
)
";
        let stripped = strip_runtime_replace_directive(original);
        assert!(!stripped.contains("lazuli.dev/runtime"));
        // The non-runtime replace survives, block intact.
        assert!(stripped.contains("example.com/other => ../other"));
        assert!(stripped.contains("replace ("));
    }

    #[test]
    fn strip_runtime_replace_removes_whole_block_when_only_runtime() {
        let original = "\
go 1.26.0

replace (
\tlazuli.dev/runtime => C:/Users/lucas/lazuli/runtime/go
)
";
        let stripped = strip_runtime_replace_directive(original);
        assert!(!stripped.contains("lazuli.dev/runtime"));
        // Emptied block is removed entirely (no dangling `replace (`).
        assert!(!stripped.contains("replace ("));
    }

    #[test]
    fn strip_runtime_replace_noop_when_absent() {
        let original = "\
go 1.26.0

use (
\t.
\t../lazuli/runtime/go
)
";
        let stripped = strip_runtime_replace_directive(original);
        assert_eq!(stripped, original);
    }
}
