//! `lazuli design import / export / diff` — round-trip between the
//! canonical `design.lzi` (lowered to `lazuli_ir::Design`) and external
//! design-token catalogs (W3C Design Tokens / Figma Tokens Studio, Amazon
//! Style Dictionary).
//!
//! See `docs/proposals/design-tokens.md` §7 for the contract. Three
//! sub-commands share the format codecs (`figma_to_design`,
//! `design_to_figma`, `style_dictionary_to_design`,
//! `design_to_style_dictionary`) plus a flat-path token view (`flat_view`)
//! used by `diff`.
//!
//! Persistence model: `design.lzi` is stored as a JSON-serialised
//! `Design` IR. Cell A owns the `.lzi` text surface (parser + emitter);
//! Cell D consumes the IR directly. When Cell A's textual surface lands
//! the read/write helpers (`read_design`, `write_design`) become the
//! only swap point.
//!
//! ## IR stub
//!
//! Cell A lands the canonical `Design` IR types in `lazuli_ir`. The
//! worktree base commit for this cell predates Cell A, so the file
//! defines the IR locally in a `mod ir` block. The orchestrator
//! reconciles at cherry-pick — when Cell A is in `main`, `mod ir` is
//! replaced by `pub use lazuli_ir::{...} as *;`. The local shape is
//! kept structurally identical to the IR contract documented in
//! `docs/proposals/design-tokens.md` §3 + the prompt's "Canonical IR
//! shape" block, so the swap is a no-op for downstream callers.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub use ir::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};

mod diff;
mod figma;
mod format_sniff;
mod style_dictionary;

use diff::compute_diff;
use figma::{design_to_figma, figma_to_design};
use format_sniff::sniff_format;
use style_dictionary::{design_to_style_dictionary, style_dictionary_to_design};

mod ir;

// =============================================================================
// Public surface
// =============================================================================

/// Import format flag from the CLI (`--format figma|style-dictionary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// W3C Design Tokens spec — emitted by Figma Tokens Studio.
    Figma,
    /// Amazon Style Dictionary source format (`value` / `type`).
    StyleDictionary,
}

/// Export target flag (`--target figma|style-dictionary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    Figma,
    StyleDictionary,
}

/// Single token-level diff between two `Design` catalogs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TokenDiff {
    /// Dotted path, e.g. `color.primary.hover` or `space.4`.
    pub path: String,
    pub from_value: String,
    pub to_value: String,
}

/// Symmetric diff between two `Design` catalogs.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct DiffReport {
    /// Paths present in `against` but missing from `design.lzi`.
    pub added: Vec<String>,
    /// Paths present in `design.lzi` but missing from `against`.
    pub removed: Vec<String>,
    /// Paths present in both but with different values.
    pub changed: Vec<TokenDiff>,
}

impl DiffReport {
    /// True when there are no added, removed, or changed tokens. The
    /// CLI uses this as the CI-gate signal (zero exit when empty).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_design::DiffReport;
    /// assert!(DiffReport::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Human-readable single-block summary written to stdout by `diff`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_design::DiffReport;
    ///
    /// let report = DiffReport::default();
    /// assert!(report.render().contains("equivalent"));
    /// ```
    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.is_empty() {
            out.push_str("design.lzi and external catalog are equivalent.\n");
            return out;
        }
        if !self.added.is_empty() {
            out.push_str("Added (in external, missing from design.lzi):\n");
            for path in &self.added {
                out.push_str("  + ");
                out.push_str(path);
                out.push('\n');
            }
        }
        if !self.removed.is_empty() {
            out.push_str("Removed (in design.lzi, missing from external):\n");
            for path in &self.removed {
                out.push_str("  - ");
                out.push_str(path);
                out.push('\n');
            }
        }
        if !self.changed.is_empty() {
            out.push_str("Changed (value differs):\n");
            for diff in &self.changed {
                out.push_str(&format!(
                    "  ~ {}: {} -> {}\n",
                    diff.path, diff.from_value, diff.to_value
                ));
            }
        }
        out
    }
}

/// `lazuli design import --from <path> [--format figma|style-dictionary] [--overwrite]`.
///
/// Reads the external JSON catalog, builds a `Design` IR, writes it to
/// `out` (typically `design.lzi` at the project root). When `overwrite`
/// is false and `out` already exists, prints a diff to stderr and
/// returns `Err` so the CLI exits non-zero.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::{import, ImportFormat};
///
/// // import(Path::new("figma.json"), ImportFormat::Figma,
/// //        Path::new("design.lzi"), true)?;
/// ```
pub fn import(from: &Path, format: ImportFormat, out: &Path, overwrite: bool) -> Result<()> {
    let raw = fs::read_to_string(from)
        .with_context(|| format!("reading external token catalog at {}", from.display()))?;
    let external: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing JSON at {}", from.display()))?;

    let incoming = match format {
        ImportFormat::Figma => figma_to_design(&external)?,
        ImportFormat::StyleDictionary => style_dictionary_to_design(&external)?,
    };

    if out.exists() && !overwrite {
        let existing = read_design(out).with_context(|| {
            format!("reading existing {} to compute import diff", out.display())
        })?;
        let report = compute_diff(&existing, &incoming);
        eprintln!(
            "{} already exists. Pass --overwrite to replace. Pending changes:",
            out.display()
        );
        eprintln!("{}", report.render());
        bail!(
            "refusing to overwrite {} without --overwrite",
            out.display()
        );
    }

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    write_design(out, &incoming).with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}

/// `lazuli design export --target figma|style-dictionary --out <path>`.
///
/// Serialises an in-memory `Design` IR to the chosen external JSON
/// catalog. Caller pre-loads the `Design` (orchestrator wiring will read
/// `design.lzi` once Cell A's text surface lands; the test suite passes
/// synthesised `Design` values directly).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::{export, ExportTarget, Design};
///
/// // export(Path::new("tokens.json"), ExportTarget::Figma, &design)?;
/// ```
pub fn export(out: &Path, target: ExportTarget, design: &Design) -> Result<()> {
    let value = match target {
        ExportTarget::Figma => design_to_figma(design),
        ExportTarget::StyleDictionary => design_to_style_dictionary(design),
    };
    let pretty =
        serde_json::to_string_pretty(&value).context("serialising token catalog to JSON")?;

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    fs::write(out, pretty.as_bytes()).with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}

/// `lazuli design diff --against <path>`.
///
/// Returns a token-level diff between the in-memory `Design` (typically
/// `design.lzi` lowered) and the external catalog at `against`. Format
/// is sniffed by file extension and structural cues; explicit callers
/// pass through `diff_with_format`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::{diff, Design};
///
/// // let report = diff(Path::new("figma.json"), &design)?;
/// ```
pub fn diff(against: &Path, design: &Design) -> Result<DiffReport> {
    let format = sniff_format(against)?;
    diff_with_format(against, format, design)
}

/// Diff with explicit format override (CLI `--format` flag forwarded).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::{diff_with_format, ImportFormat, Design};
///
/// // let report = diff_with_format(Path::new("tokens.json"),
/// //                               ImportFormat::StyleDictionary, &design)?;
/// ```
pub fn diff_with_format(
    against: &Path,
    format: ImportFormat,
    design: &Design,
) -> Result<DiffReport> {
    let raw = fs::read_to_string(against)
        .with_context(|| format!("reading external token catalog at {}", against.display()))?;
    let external: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing JSON at {}", against.display()))?;
    let incoming = match format {
        ImportFormat::Figma => figma_to_design(&external)?,
        ImportFormat::StyleDictionary => style_dictionary_to_design(&external)?,
    };
    Ok(compute_diff(design, &incoming))
}

// =============================================================================
// Persistence — `design.lzi` as JSON-serialised IR (Cell A swaps to text)
// =============================================================================

/// Reads the `Design` IR from `path`. Today the file is JSON; Cell A
/// will swap this to a `.lzi` text parser without touching the CLI
/// surface.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::read_design;
///
/// // let design = read_design(Path::new("design.lzi"))?;
/// ```
pub fn read_design(path: &Path) -> Result<Design> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("reading design at {}", path.display()))?;
    let design: Design = serde_json::from_str(&raw)
        .with_context(|| format!("parsing design at {}", path.display()))?;
    Ok(design)
}

/// Writes the `Design` IR to `path`. Sort-stable JSON for deterministic
/// diffs; emission order inside the IR is preserved by the IR types
/// themselves (Vec-of-token preserves authored order).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::{write_design, Design};
///
/// // write_design(Path::new("design.lzi"), &design)?;
/// ```
pub fn write_design(path: &Path, design: &Design) -> Result<()> {
    let pretty = serde_json::to_string_pretty(design).context("serialising Design IR to JSON")?;
    fs::write(path, pretty.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

// Format sniff, Figma codec, Style Dictionary codec, and flat-view
// diff live in sibling modules (format_sniff, figma, style_dictionary,
// diff). Imports above wire them back in.

// =============================================================================
// Convenience for orchestrator wiring (used by main.rs post-merge)
// =============================================================================

/// Canonical `design.lzi` path under `project_root`. Honors
/// `Lazurite.toml [lazurite] app_dir` when present (default: project
/// root). Exists for the orchestrator wire-up; the CLI parses `--out`
/// / `--from` directly.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_design::default_design_path;
///
/// let path = default_design_path(Path::new("."));
/// assert!(path.ends_with("design.lzi"));
/// ```
pub fn default_design_path(project_root: &Path) -> PathBuf {
    crate::lazurite_manifest::resolve_in_app_dir(project_root, "design.lzi")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests_fixtures;

#[cfg(test)]
mod tests {
    use super::tests_fixtures::{
        EXT_LAZULI_DARK, demo_fixture, sort_for_round_trip, unique_temp_dir,
    };
    use super::*;
    use serde_json::json;

    #[test]
    fn figma_round_trip_preserves_design() {
        let original = demo_fixture();
        let figma = design_to_figma(&original);
        let mut restored = figma_to_design(&figma).unwrap();
        // Restored design loses the user-chosen name (set to "imported")
        // since the W3C JSON has no name field; align before assert.
        let mut expected = original.clone();
        expected.name = "imported".to_string();
        sort_for_round_trip(&mut restored);
        sort_for_round_trip(&mut expected);
        assert_eq!(restored, expected);
    }

    #[test]
    fn style_dictionary_round_trip_preserves_design() {
        let original = demo_fixture();
        let sd = design_to_style_dictionary(&original);
        let mut restored = style_dictionary_to_design(&sd).unwrap();
        let mut expected = original.clone();
        expected.name = "imported".to_string();
        sort_for_round_trip(&mut restored);
        sort_for_round_trip(&mut expected);
        assert_eq!(restored, expected);
    }

    #[test]
    fn color_with_multiple_states_round_trips() {
        let mut design = demo_fixture();
        // Reduce noise — only the multi-state primary.
        design.colors.retain(|c| c.name == "primary");
        let figma = design_to_figma(&design);
        // Assert the JSON exposes a sub-block keyed by states.
        let primary = figma
            .get("color")
            .and_then(|v| v.get("primary"))
            .and_then(|v| v.as_object())
            .expect("primary sub-block");
        assert!(primary.contains_key("base"));
        assert!(primary.contains_key("hover"));
        assert!(primary.contains_key("active"));
        assert!(primary.contains_key("foreground"));

        let restored = figma_to_design(&figma).unwrap();
        let primary_back = restored
            .colors
            .iter()
            .find(|c| c.name == "primary")
            .unwrap();
        assert_eq!(primary_back.states.len(), 4);
        assert_eq!(primary_back.states[0].kind, ColorStateKind::Base);
        assert_eq!(primary_back.states[0].value, "#7c3aed");
    }

    #[test]
    fn dark_variants_captured_via_lazuli_extension() {
        let mut design = demo_fixture();
        design.colors.retain(|c| c.name == "background");
        let figma = design_to_figma(&design);

        let bg = figma
            .pointer("/color/background/base")
            .and_then(|v| v.as_object())
            .expect("background.base leaf");
        let dark = bg
            .get("$extensions")
            .and_then(|v| v.get(EXT_LAZULI_DARK))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .expect("dark variant via $extensions.com.lazuli.dark");
        assert_eq!(dark, "#09090b");

        // Round-trip preserves the dark hex.
        let restored = figma_to_design(&figma).unwrap();
        let bg_back = restored
            .colors
            .iter()
            .find(|c| c.name == "background")
            .unwrap();
        let base_state = bg_back
            .states
            .iter()
            .find(|s| s.kind == ColorStateKind::Base)
            .unwrap();
        assert_eq!(base_state.dark.as_deref(), Some("#09090b"));
    }
}

#[cfg(test)]
mod tests_io;
