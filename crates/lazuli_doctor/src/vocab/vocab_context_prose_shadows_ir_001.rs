//! VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 — a feature's co-located
//! `<feature>.ctx.md` prose SHADOWS (duplicates) what the IR already knows.
//!
//! Fires when a `<feature>.ctx.md` sidecar contains a markdown table whose
//! header columns duplicate >= 3 of a single resource's field names (a hand-
//! maintained schema dump that drifts from the IR). See the positive /
//! negative fixtures in the in-module `#[cfg(test)]` block for the exact
//! shape that does vs. does not trigger.
//!
//! The failure mode this rule kills: a hand-maintained context doc that
//! re-states the resource schema as a markdown table. Such a table is
//! *derived* data masquerading as prose — it drifts the instant the real
//! resource definition changes, and a cold reader (human or LLM) can no
//! longer tell which copy is authoritative. The fix the message recommends:
//! delete the duplicated table; the data model is derivable on demand via
//! `inspect --expand=context`.
//!
//! ## Doctrine-enforcement framing (single-pilot)
//!
//! This rule is single-pilot evidence (corbanx only). That does NOT meet
//! RULE-VOCAB-01's net-new-vocab bar (>=3 handlers / >=2 pilots). It does
//! not need to: the rule introduces NO new keyword or grammar — the
//! `<feature>.ctx.md` file is a convention, not vocabulary. It ENFORCES
//! EXISTING doctrine, namely `docs/canonical-semantics.md` (~line 273):
//!
//!   > Do not duplicate schema, operations, policies, rules, events ...
//!   > there.
//!
//! i.e. it is a doctrine-enforcement diagnostic over a documented
//! convention, the direct sibling of `VOCAB-CONTEXT-CTXMD-001` (which
//! enforces the *presence* of that same sidecar). The registry/severity
//! layer carries no `Preview` mechanism for convention-derived codes
//! (the sidecar has no keyword owner — it is claimed in
//! `GLOBAL_DIAGNOSTICS`), so this doc-comment framing is the marker.
//!
//! ## Detection predicate (pinned precision)
//!
//! For each feature that has a co-located `<feature>.ctx.md` (resolved at
//! the SINGLE base of the feature `.lzi` directory, exactly like
//! `vocab_context_ctxmd_001`):
//!   1. Parse the markdown for tables: a header row `| a | b | ... |`
//!      immediately followed by a delimiter row `|---|---|...|`.
//!   2. Take the table's HEADER-row cells. Normalize cells AND resource
//!      field names identically: lowercase, trim, strip backticks/spaces,
//!      and collapse every run of non-alphanumeric characters to a single
//!      `_` (so `snake_case`, `space separated`, and `kebab-case` all
//!      compare equal).
//!   3. FIRE when a table's normalized header-cell SET overlaps a single
//!      resource's normalized field-name SET by **>= 3** cells. Report the
//!      shadowed resource + the table's 1-based line in the sidecar.
//!   4. Decoupled from heading text — fire on ANY qualifying table
//!      regardless of the enclosing markdown heading (headings are
//!      free-form). A field name mentioned in running prose (e.g.
//!      "soft delete on proposals") never fires: only a TABLE meeting the
//!      `>= 3` header-overlap threshold does.
//!
//! The endpoint/route-table secondary case (a list/table of routes matching
//! the feature's command/api routes) is DEFERRED out of v0 — the
//! resource-schema-table case is the must-have, and folding routes in would
//! widen the normalization surface (HTTP verbs, path params) without
//! evidence it is the dominant drift. Tracked for a follow-up cut.
//!
//! Severity: warning (category `Vocabulary`, same posture as the
//! `VOCAB-CONTEXT-*` family that governs `purpose` / `non_goals` /
//! the `<feature>.ctx.md` convention).
//!
//! Reference: docs/canonical-semantics.md#feature-context-vocabulary.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// Minimum number of header-cell ↔ resource-field overlaps before
/// VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 fires. Three is the threshold that
/// separates an incidental coincidence (a table about something else that
/// happens to share one or two column names with a resource) from a genuine
/// schema dump.
pub const SHADOW_OVERLAP_THRESHOLD: usize = 3;

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 finding: a markdown table in the
/// feature's `<feature>.ctx.md` whose header set shadows a resource's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file (the finding anchors at the feature, like the
    /// sibling `VOCAB-CONTEXT-*` rules).
    pub path: PathBuf,
    /// Name of the feature whose sidecar carries the shadowing table.
    pub feature: String,
    /// The `<feature>.ctx.md` sidecar the table lives in.
    pub ctx_path: PathBuf,
    /// The resource whose fields the table header shadows.
    pub resource: String,
    /// 1-based line of the table's HEADER row inside the sidecar.
    pub table_line: usize,
    /// How many header cells overlapped the resource's fields (>= threshold).
    pub overlap: usize,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-CONTEXT-PROSE-SHADOWS-IR-001";

    /// Render the "prose shadows IR" diagnostic — names the sidecar, the
    /// table line, the shadowed resource, and recommends the derive-don't-
    /// prose fix.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_context_prose_shadows_ir_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("proposals.lzi"),
    ///     feature: "proposals".into(),
    ///     ctx_path: PathBuf::from("proposals.ctx.md"),
    ///     resource: "Proposal".into(),
    ///     table_line: 12,
    ///     overlap: 4,
    /// };
    /// assert!(f.message().contains("Proposal"));
    /// assert!(f.message().contains("inspect --expand=context"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "feature `{}`: the `{}` context sidecar has a markdown table (line {}) whose \
             header columns duplicate {} of resource `{}`'s fields — the prose shadows the IR. \
             A hand-maintained schema table drifts the moment the resource definition changes; \
             cold readers can no longer tell which copy is authoritative. Delete the duplicated \
             table and keep the sidecar prose-only: the data model is derivable on demand via \
             `inspect --expand=context`. See docs/canonical-semantics.md#feature-context-vocabulary.",
            self.feature,
            self.ctx_path.display(),
            self.table_line,
            self.overlap,
            self.resource,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CONTEXT-PROSE-SHADOWS-IR-001 for one feature.
///
/// `lzi_path` is the source `.lzi` path; the `<feature>.ctx.md` sidecar is
/// resolved by convention at the SINGLE base of its parent directory (no
/// project-root fallback) — identical resolution to
/// [`super::vocab_context_ctxmd_001::check`]. The rule reads the feature's
/// resources straight off the lifted `Feature` IR, so it carries no extra
/// crate dependency.
///
/// Silent when: the sidecar is absent/unreadable; the feature declares no
/// resources; or no markdown table in the sidecar overlaps any single
/// resource's field set by [`SHADOW_OVERLAP_THRESHOLD`] cells.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_context_prose_shadows_ir_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources");
/// let _ = check(&feature, Path::new("proposals.lzi"));
/// ```
pub fn check(feature: &Feature, lzi_path: &Path) -> Vec<Finding> {
    // Single-base convention resolution — mirror `vocab_context_ctxmd_001`.
    let dir = lzi_path.parent().unwrap_or_else(|| Path::new("."));
    let ctx_path = dir.join(format!("{}.ctx.md", feature.name));

    let Ok(contents) = std::fs::read_to_string(&ctx_path) else {
        // No sidecar → nothing to shadow. (CTXMD-001 owns the absence case.)
        return Vec::new();
    };

    // Pre-compute each resource's normalized field-name set once.
    let resources: Vec<(&str, BTreeSet<String>)> = feature
        .resources
        .iter()
        .map(|r| {
            let fields: BTreeSet<String> = r
                .fields
                .iter()
                .map(|f| normalize(&f.name))
                .filter(|n| !n.is_empty())
                .collect();
            (r.name.as_str(), fields)
        })
        .filter(|(_, fields)| !fields.is_empty())
        .collect();
    if resources.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<Finding> = Vec::new();
    for table in parse_tables(&contents) {
        let header_set: BTreeSet<String> = table
            .headers
            .iter()
            .map(|c| normalize(c))
            .filter(|n| !n.is_empty())
            .collect();
        if header_set.is_empty() {
            continue;
        }
        // For each resource, count the header↔field overlap; report the
        // single best (largest-overlap) shadowed resource per table so one
        // schema dump produces one finding, not N near-duplicates.
        let mut best: Option<(&str, usize)> = None;
        for (name, fields) in &resources {
            let overlap = header_set.intersection(fields).count();
            if overlap >= SHADOW_OVERLAP_THRESHOLD && best.map(|(_, b)| overlap > b).unwrap_or(true)
            {
                best = Some((name, overlap));
            }
        }
        if let Some((resource, overlap)) = best {
            out.push(Finding {
                path: lzi_path.to_path_buf(),
                feature: feature.name.clone(),
                ctx_path: ctx_path.clone(),
                resource: resource.to_string(),
                table_line: table.header_line,
                overlap,
            });
        }
    }
    out
}

// ── internals ─────────────────────────────────────────────────────────────────

/// A parsed markdown table — only the header row + its source line matter
/// for this rule (the body rows are the drifting data the table re-states).
struct MarkdownTable {
    /// Header cells, in source order, un-normalized.
    headers: Vec<String>,
    /// 1-based line of the header row in the source.
    header_line: usize,
}

/// Parse GitHub-flavoured markdown pipe tables out of `src`. A table is a
/// header row of the form `| a | b | ... |` IMMEDIATELY followed by a
/// delimiter row whose cells are all dashes (optionally colon-aligned, e.g.
/// `:---`, `---:`, `:--:`). Anything that isn't a delimiter row after a
/// candidate header is not a table (so a lone pipe-bearing prose line never
/// qualifies).
fn parse_tables(src: &str) -> Vec<MarkdownTable> {
    let lines: Vec<&str> = src.lines().collect();
    let mut tables = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let header = lines[i];
        let delim = lines[i + 1];
        if is_pipe_row(header) && is_delimiter_row(delim) {
            let headers = split_cells(header);
            // A delimiter cell count must match (or be compatible with) the
            // header cell count for a well-formed table, but we don't gate on
            // it — a ragged table still shadows the IR if its headers do.
            if !headers.is_empty() {
                tables.push(MarkdownTable {
                    headers,
                    header_line: i + 1, // 1-based.
                });
            }
            // Skip past the header + delimiter; body rows are irrelevant.
            i += 2;
            continue;
        }
        i += 1;
    }
    tables
}

/// A line that looks like a table row: contains at least one `|` and, after
/// trimming, holds at least one non-pipe character (rejects a bare `|`).
fn is_pipe_row(line: &str) -> bool {
    let t = line.trim();
    t.contains('|') && t.chars().any(|c| c != '|' && !c.is_whitespace())
}

/// A markdown table delimiter row: every cell is a run of `-` with optional
/// leading/trailing `:` alignment markers (e.g. `| --- | :--: |`). Must
/// contain at least one dash and at least one pipe.
fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('|') || !t.contains('-') {
        return false;
    }
    split_cells(line).iter().all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':') && c.contains('-')
    }) && !split_cells(line).is_empty()
}

/// Split a `| a | b | c |` row into trimmed cell strings, dropping the empty
/// leading/trailing fragments produced by the bordering pipes.
fn split_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Normalize a cell or field name for comparison: lowercase, strip
/// backticks/whitespace, and collapse every run of non-alphanumeric
/// characters to a single `_`, then trim leading/trailing `_`. This makes
/// `created_at`, `Created At`, `created-at`, and `` `created_at` `` all
/// compare equal.
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_us = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_us = false;
        } else {
            // Any non-alphanumeric (backtick, space, dash, dot…) → single `_`.
            if !prev_us {
                out.push('_');
                prev_us = true;
            }
        }
    }
    out.trim_matches('_').to_string()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_context_prose_shadows_ir_001_tests.rs");
}
