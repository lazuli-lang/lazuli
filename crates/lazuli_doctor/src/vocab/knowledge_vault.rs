//! Shared scanner for the `knowledge/<sector>/` document vault.
//!
//! The vault is a **first-class, committed authored source directory** at the
//! project root (`knowledge/`) — the source of truth. The disk walk here
//! resolves against `knowledge/<sector>/`, NOT `.lazuli/knowledge/`: `.lazuli/`
//! is the disposable, gitignored internal cache, so it is the wrong home for
//! hand-authored, version-controlled knowledge docs. What *does* live under
//! `.lazuli/` is the **derived knowledge index** (future sqlite-vec embedding
//! store) regenerated from `knowledge/` — never the source docs themselves.
//!
//! Five sibling rules read this vault — `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001`,
//! `VOCAB-KNOWLEDGE-UNGATED-WRITE-001`, `VOCAB-KNOWLEDGE-STALE-001`,
//! `VOCAB-KNOWLEDGE-DANGLING-CITE-001`, `VOCAB-KNOWLEDGE-DUP-TOPIC-001` — so
//! the disk walk + frontmatter parse lives here once instead of being copied
//! five ways. Mirrors the `universal_columns` shared-helper precedent
//! (`crates/lazuli_doctor/src/vocab/universal_columns.rs`).
//!
//! ## File layout (proved first, no grammar)
//!
//! ```text
//! knowledge/<sector>/NNNN-<slug>.md
//!   frontmatter: tier(draft|approved|gold|deprecated) | supersedes
//!              | revalidate_by | cites | tags
//! ```
//!
//! The frontmatter is a leading `---`-fenced YAML-ish block. We parse only
//! the flat scalar / inline-list keys the five rules need with a stdlib
//! line walker — no `serde_yaml` dependency, keeping the helper wire-thin
//! per the founding principle (a frontmatter block is a handful of
//! `key: value` lines, not a document model).
//!
//! Append-only history is **git**, not a bespoke engine: the gate rule
//! shells out to `git log` (see [`passed_through_draft`]) and skips silently
//! when git or history is unavailable — the same "skip, don't fail when the
//! environment is unreachable" discipline the test harness uses for an
//! offline Postgres.
//!
//! Severity: none of its own — this is a shared scanner, not a diagnostic.
//! The five `VOCAB-KNOWLEDGE-*` rules that consume it each carry their own
//! `warning`-tier severity (category `Vocabulary`). Trigger cue: this module
//! has no trigger; for an example of the data it returns, see the unit tests
//! below (a synthetic `knowledge/<sector>/` temp fixture).
//!
//! Reference: docs/proposals/knowledge-sector-field.md §Doctor.

use std::path::{Path, PathBuf};

/// Tier of a vault document, parsed from the `tier:` frontmatter key.
///
/// `gold` is the only tier the rules gate on (it is the published,
/// load-bearing state); the rest are tracked so the gate (`draft -> gold`)
/// and dedup (`two gold without supersession`) rules can reason about them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Work-in-progress; never gated.
    Draft,
    /// Reviewed but not yet promoted.
    Approved,
    /// Published, load-bearing — the tier all five rules key on.
    Gold,
    /// Retired; excluded from dedup + staleness.
    Deprecated,
}

impl Tier {
    /// Parse a `tier:` value, case-insensitively. Unknown / absent => `None`.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "approved" => Some(Self::Approved),
            "gold" => Some(Self::Gold),
            "deprecated" => Some(Self::Deprecated),
            _ => None,
        }
    }
}

/// One parsed vault document. Only the frontmatter keys the five
/// `VOCAB-KNOWLEDGE-*` rules consume are surfaced; the markdown body is
/// dropped (none of the rules read prose).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultDoc {
    /// Absolute (or vault-relative) path to the `.md` file on disk.
    pub path: PathBuf,
    /// Tier from frontmatter, `None` when the `tier:` key is absent or
    /// unrecognized.
    pub tier: Option<Tier>,
    /// `true` when a `supersedes`, `replaces`, or `deprecated_by`/`deprecated`
    /// relation key is present and non-empty — the dedup rule's escape hatch.
    pub has_supersession: bool,
    /// `revalidate_by:` value verbatim (an ISO `YYYY-MM-DD` date), if present.
    pub revalidate_by: Option<String>,
    /// Symbols listed under `cites:` (inline `[a, b]` or block list form).
    pub cites: Vec<String>,
    /// The topic slug, derived from the `NNNN-<slug>.md` filename (the
    /// `<slug>` portion, lower-cased) so dedup can group by topic.
    pub topic_slug: String,
}

impl VaultDoc {
    /// Convenience: is this doc `gold`?
    pub(crate) fn is_gold(&self) -> bool {
        self.tier == Some(Tier::Gold)
    }
}

// ── path resolution ─────────────────────────────────────────────────────────────

/// Project-root-relative name of the knowledge vault directory.
///
/// First-class **committed** authored source — the source of truth for the
/// knowledge sectors. Deliberately NOT under `.lazuli/`: that folder is the
/// disposable, gitignored internal cache (per the CLAUDE.md folder
/// conventions) and is the wrong home for version-controlled docs. The
/// *derived* index built from this vault (future sqlite-vec embedding store)
/// is what lives in `.lazuli/`; the authored `.md` docs live here.
pub const KNOWLEDGE_VAULT_ROOT: &str = "knowledge";

/// The CLOSED CORE catalog of opinionated knowledge sectors every Lazuli
/// project understands without scaffolding a folder or declaring a custom
/// sector. A `knowledge <sector>` naming one of these is always KNOWN to
/// `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` — even before the
/// `knowledge/<sector>/` folder is created — because the framework opines
/// these four as the canonical documentation lanes:
///
/// - `decisions`  — architecture / design decision records (ADRs).
/// - `changes`    — change logs / migration notes / what-moved-and-why.
/// - `gaps`       — known shortfalls / residual-gap backlog.
/// - `lazuli-way` — framework idiom / "how we do it here" guidance.
///
/// `_inbox` is deliberately ABSENT: it is the staging directory for
/// un-triaged drafts, not a sector. Projects extend this core with custom
/// sectors via `[knowledge.sectors]` in `Lazurite.toml` (governed
/// flexibility — a declared slug, not a free-for-all dialect).
pub const CORE_KNOWLEDGE_SECTORS: &[&str] = &["decisions", "changes", "gaps", "lazuli-way"];

/// `true` when `sector` is one of the [`CORE_KNOWLEDGE_SECTORS`]. Trims the
/// input so a stray-whitespace slug still matches the catalog.
pub(crate) fn is_core_sector(sector: &str) -> bool {
    let s = sector.trim();
    CORE_KNOWLEDGE_SECTORS.contains(&s)
}

/// Resolve the `knowledge/<sector>/` directory for a sector slug, relative to
/// the project root. Mirrors `vocab_context_ctxmd_001::resolve_candidate`'s
/// project-root anchoring.
///
/// The vault root is the committed `knowledge/` source directory, **not**
/// `.lazuli/knowledge/` — `.lazuli/` is the gitignored cache and only holds the
/// derived index, never the authored source docs.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::knowledge_vault::sector_dir;
///
/// let dir = sector_dir(Path::new("/proj"), "billing");
/// assert!(dir.ends_with("billing"));
/// ```
pub fn sector_dir(project_root: &Path, sector: &str) -> PathBuf {
    project_root.join(KNOWLEDGE_VAULT_ROOT).join(sector)
}

/// Does a `knowledge/<sector>/` folder exist under the project root?
///
/// This is the single predicate `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` keys on.
pub(crate) fn sector_exists(project_root: &Path, sector: &str) -> bool {
    sector_dir(project_root, sector).is_dir()
}

// ── enumeration ─────────────────────────────────────────────────────────────────

/// Enumerate and parse every `NNNN-<slug>.md` document in a sector folder.
///
/// Returns an empty vector when the folder is missing or unreadable (the
/// sector-unknown rule owns the "folder absent" diagnostic; the doc-level
/// rules simply have nothing to inspect). Non-`.md` entries and dotfiles are
/// ignored. Results are sorted by path for deterministic diagnostics.
pub(crate) fn scan_sector(project_root: &Path, sector: &str) -> Vec<VaultDoc> {
    let dir = sector_dir(project_root, sector);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut docs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.starts_with('.') {
            continue;
        }
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        docs.push(parse_doc(&path, stem, &contents));
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    docs
}

/// Parse one document's frontmatter into a [`VaultDoc`]. Public so rule unit
/// tests can build a `VaultDoc` from an in-memory string without touching
/// disk.
pub(crate) fn parse_doc(path: &Path, file_stem: &str, contents: &str) -> VaultDoc {
    let fm = extract_frontmatter(contents);
    let mut tier = None;
    let mut has_supersession = false;
    let mut revalidate_by = None;
    let mut cites = Vec::new();

    for (key, value) in &fm {
        match key.as_str() {
            "tier" => tier = Tier::parse(value),
            // Any non-empty supersession-relation key satisfies the dedup
            // escape hatch. The proposal names `supersedes`/`replaces`; we
            // also accept the deprecation direction so a doc explicitly
            // deprecated by another doesn't false-positive.
            "supersedes" | "replaces" | "deprecated_by" | "deprecates" | "deprecated" => {
                // Parse as list-or-scalar so an empty value, a bare `[]`
                // (the empty block-list flush), and whitespace all read as
                // "no relation" — only a real target (scalar `0001-old` or
                // `[a, b]`) counts.
                if !parse_inline_list(value).is_empty() {
                    has_supersession = true;
                }
            }
            "revalidate_by" => {
                let v = value.trim();
                if !v.is_empty() {
                    revalidate_by = Some(v.to_string());
                }
            }
            "cites" => cites = parse_inline_list(value),
            _ => {}
        }
    }

    VaultDoc {
        path: path.to_path_buf(),
        tier,
        has_supersession,
        revalidate_by,
        cites,
        topic_slug: slug_from_stem(file_stem),
    }
}

// ── frontmatter parse (stdlib-only) ──────────────────────────────────────────────

/// Pull the leading `---`-fenced frontmatter into `(key, value)` pairs.
///
/// Supports the flat scalar + inline-list subset the five rules need:
///
/// ```text
/// ---
/// tier: gold
/// revalidate_by: 2025-01-01
/// cites: [billing.charge, billing.Invoice]
/// supersedes: 0001-old-topic
/// ---
/// ```
///
/// Block-list `cites:` (a bare `cites:` followed by `- item` lines) is also
/// folded into the `cites` value as a bracketed inline list so the single
/// [`parse_inline_list`] path handles both shapes. Returns an empty vec when
/// no frontmatter fence is present.
fn extract_frontmatter(contents: &str) -> Vec<(String, String)> {
    let mut lines = contents.lines();
    // First non-empty line must be the opening fence.
    let mut started = false;
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            continue;
        }
        if line.trim() == "---" {
            started = true;
        }
        break;
    }
    if !started {
        return Vec::new();
    }

    let mut out: Vec<(String, String)> = Vec::new();
    let mut pending_list_key: Option<String> = None;
    let mut pending_list_items: Vec<String> = Vec::new();

    let flush_list =
        |out: &mut Vec<(String, String)>, key: &mut Option<String>, items: &mut Vec<String>| {
            if let Some(k) = key.take() {
                out.push((k, format!("[{}]", items.join(", "))));
                items.clear();
            }
        };

    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "---" {
            // Closing fence ends the block.
            flush_list(&mut out, &mut pending_list_key, &mut pending_list_items);
            break;
        }
        // Block-list continuation: `  - value`.
        if let Some(item) = trimmed.trim_start().strip_prefix("- ")
            && pending_list_key.is_some()
        {
            pending_list_items.push(item.trim().to_string());
            continue;
        }
        // Any non-list line flushes a pending block list first.
        flush_list(&mut out, &mut pending_list_key, &mut pending_list_items);

        let Some((raw_key, raw_val)) = trimmed.split_once(':') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let val = raw_val.trim();
        if val.is_empty() {
            // Possibly the head of a block list — defer until we see `- `.
            pending_list_key = Some(key);
            pending_list_items.clear();
        } else {
            out.push((key, val.to_string()));
        }
    }
    flush_list(&mut out, &mut pending_list_key, &mut pending_list_items);
    out
}

/// Parse an inline list value `[a, b, c]` OR a single bare scalar into a
/// `Vec<String>`, stripping surrounding quotes/brackets and empty entries.
fn parse_inline_list(raw: &str) -> Vec<String> {
    let inner = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Derive the topic slug from a `NNNN-<slug>` file stem: drop a leading
/// numeric ordinal + separator, lower-case the rest. `0007-charge-flow`
/// -> `charge-flow`. A stem without the `NNNN-` prefix is returned
/// lower-cased verbatim.
fn slug_from_stem(stem: &str) -> String {
    let lower = stem.to_ascii_lowercase();
    if let Some((head, tail)) = lower.split_once('-')
        && !head.is_empty()
        && head.chars().all(|c| c.is_ascii_digit())
    {
        return tail.to_string();
    }
    lower
}

// ── git history probe ────────────────────────────────────────────────────────────

/// Return the set of repo-relative paths that ever appeared in git history
/// for `file`, across renames, as raw `git log --follow --name-only`
/// output lines. Used by the ungated-write gate to ask "did this doc ever
/// exist while tier was *not* gold" — but the gate's heuristic actually
/// keys on whether the file has *any* prior commit at all (an abrupt
/// gold-on-first-commit being the lixão signal). Returns `None` when git is
/// unavailable, the path is untracked, or the command errors — callers MUST
/// treat `None` as "cannot evaluate -> skip" (do not fire), matching the
/// harness "skip, don't fail when the environment is unreachable" rule.
pub(crate) fn git_commit_count(project_root: &Path, file: &Path) -> Option<usize> {
    let rel = file.strip_prefix(project_root).unwrap_or(file);
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .arg("log")
        .arg("--follow")
        .arg("--format=%H")
        .arg("--")
        .arg(rel)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let count = text.lines().filter(|l| !l.trim().is_empty()).count();
    if count == 0 {
        // Untracked / never committed: history can't prove a draft phase
        // either way, so callers skip rather than fire.
        return None;
    }
    Some(count)
}

/// Did the document ever carry a non-`gold` `tier:` in a prior git revision?
///
/// Walks `git log -p` blame-free by reading each historical blob's
/// frontmatter `tier:`. Returns:
///   * `Some(true)`  — a prior revision had tier draft/approved (gate satisfied)
///   * `Some(false)` — every revision (including the first) was already gold
///   * `None`        — git unavailable / untracked / unreadable (skip)
pub(crate) fn passed_through_draft(project_root: &Path, file: &Path) -> Option<bool> {
    let rel = file.strip_prefix(project_root).unwrap_or(file);
    // List historical commit hashes touching the file, oldest last.
    let log = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .arg("log")
        .arg("--follow")
        .arg("--format=%H")
        .arg("--")
        .arg(rel)
        .output()
        .ok()?;
    if !log.status.success() {
        return None;
    }
    let hashes: Vec<String> = String::from_utf8_lossy(&log.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if hashes.is_empty() {
        return None;
    }
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    for hash in &hashes {
        let blob = std::process::Command::new("git")
            .arg("-C")
            .arg(project_root)
            .arg("show")
            .arg(format!("{hash}:{rel_str}"))
            .output()
            .ok()?;
        if !blob.status.success() {
            continue;
        }
        let contents = String::from_utf8_lossy(&blob.stdout);
        let fm = extract_frontmatter(&contents);
        let tier = fm
            .iter()
            .find(|(k, _)| k == "tier")
            .and_then(|(_, v)| Tier::parse(v));
        match tier {
            Some(Tier::Draft) | Some(Tier::Approved) => return Some(true),
            _ => {}
        }
    }
    Some(false)
}

// ── tests ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("knowledge_vault_tests.rs");
}
