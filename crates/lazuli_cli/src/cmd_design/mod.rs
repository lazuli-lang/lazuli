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

mod ir {
    //! Local mirror of the `lazuli_ir::Design` IR — see module docs for
    //! the cherry-pick swap plan. Shape MUST match `lazuli_ir` field for
    //! field; deviations break the orchestrator's `pub use lazuli_ir::*`
    //! reconciliation.

    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Design {
        pub name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub extends: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub colors: Vec<ColorToken>,
        #[serde(default)]
        pub typography: Typography,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub spaces: Vec<ScaleToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub radii: Vec<ScaleToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub shadows: Vec<ShadowToken>,
        #[serde(default)]
        pub motion: Motion,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub breakpoints: Vec<ScaleToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub z_indices: Vec<ZToken>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ColorToken {
        pub name: String,
        pub states: Vec<ColorState>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ColorState {
        pub kind: ColorStateKind,
        pub value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub dark: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ColorStateKind {
        Base,
        Hover,
        Active,
        Foreground,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub struct Typography {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub families: Vec<FamilyToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub scale: Vec<TextScaleToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub weights: Vec<WeightToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub tracking: Vec<TrackingToken>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct FamilyToken {
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TextScaleToken {
        pub name: String,
        pub size: String,
        pub line_height: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct WeightToken {
        pub name: String,
        pub value: u16,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TrackingToken {
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ScaleToken {
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ShadowToken {
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub struct Motion {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub durations: Vec<ScaleToken>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub easings: Vec<EasingToken>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct EasingToken {
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ZToken {
        pub name: String,
        pub value: i32,
    }
}

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
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Human-readable single-block summary written to stdout by `diff`.
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
pub fn diff(against: &Path, design: &Design) -> Result<DiffReport> {
    let format = sniff_format(against)?;
    diff_with_format(against, format, design)
}

/// Diff with explicit format override (CLI `--format` flag forwarded).
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
pub fn default_design_path(project_root: &Path) -> PathBuf {
    crate::lazurite_manifest::resolve_in_app_dir(project_root, "design.lzi")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Test-only access to the dark-mode extension key used by both
    // codec dialects. Mirrors the const in `figma.rs` so the test
    // surface keeps working through the split.
    const EXT_LAZULI_DARK: &str = "com.lazuli.dark";

    fn unique_temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lazuli-design-{label}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// example-style fixture covering every group + dark variants + a
    /// multi-state color sub-block.
    fn demo_fixture() -> Design {
        Design {
            name: "example".to_string(),
            extends: None,
            colors: vec![
                ColorToken {
                    name: "primary".to_string(),
                    states: vec![
                        ColorState {
                            kind: ColorStateKind::Base,
                            value: "#7c3aed".to_string(),
                            dark: None,
                        },
                        ColorState {
                            kind: ColorStateKind::Hover,
                            value: "#6d28d9".to_string(),
                            dark: None,
                        },
                        ColorState {
                            kind: ColorStateKind::Active,
                            value: "#5b21b6".to_string(),
                            dark: None,
                        },
                        ColorState {
                            kind: ColorStateKind::Foreground,
                            value: "#ffffff".to_string(),
                            dark: None,
                        },
                    ],
                },
                ColorToken {
                    name: "background".to_string(),
                    states: vec![
                        ColorState {
                            kind: ColorStateKind::Base,
                            value: "#ffffff".to_string(),
                            dark: Some("#09090b".to_string()),
                        },
                        ColorState {
                            kind: ColorStateKind::Foreground,
                            value: "#09090b".to_string(),
                            dark: Some("#fafafa".to_string()),
                        },
                    ],
                },
                ColorToken {
                    name: "success".to_string(),
                    states: vec![ColorState {
                        kind: ColorStateKind::Base,
                        value: "#16a34a".to_string(),
                        dark: None,
                    }],
                },
            ],
            typography: Typography {
                families: vec![
                    FamilyToken {
                        name: "sans".to_string(),
                        value: "Inter, system-ui, sans-serif".to_string(),
                    },
                    FamilyToken {
                        name: "mono".to_string(),
                        value: "JetBrains Mono, monospace".to_string(),
                    },
                ],
                scale: vec![
                    TextScaleToken {
                        name: "base".to_string(),
                        size: "1rem".to_string(),
                        line_height: "1.5rem".to_string(),
                    },
                    TextScaleToken {
                        name: "lg".to_string(),
                        size: "1.125rem".to_string(),
                        line_height: "1.75rem".to_string(),
                    },
                ],
                weights: vec![
                    WeightToken {
                        name: "regular".to_string(),
                        value: 400,
                    },
                    WeightToken {
                        name: "bold".to_string(),
                        value: 700,
                    },
                ],
                tracking: vec![TrackingToken {
                    name: "tight".to_string(),
                    value: "-0.025em".to_string(),
                }],
            },
            spaces: vec![
                ScaleToken {
                    name: "1".to_string(),
                    value: "0.25rem".to_string(),
                },
                ScaleToken {
                    name: "4".to_string(),
                    value: "1rem".to_string(),
                },
            ],
            radii: vec![ScaleToken {
                name: "md".to_string(),
                value: "0.375rem".to_string(),
            }],
            shadows: vec![ShadowToken {
                name: "base".to_string(),
                value: "0 1px 3px 0 rgb(0 0 0 / 0.1)".to_string(),
            }],
            motion: Motion {
                durations: vec![ScaleToken {
                    name: "fast".to_string(),
                    value: "150ms".to_string(),
                }],
                easings: vec![EasingToken {
                    name: "out".to_string(),
                    value: "cubic-bezier(0, 0, 0.2, 1)".to_string(),
                }],
            },
            breakpoints: vec![ScaleToken {
                name: "md".to_string(),
                value: "768px".to_string(),
            }],
            z_indices: vec![ZToken {
                name: "modal".to_string(),
                value: 1300,
            }],
        }
    }

    /// Round-trip equality is checked against the sorted normal form
    /// because Figma/SD JSON encodes groups as objects (unordered) and
    /// re-import sorts by key. The original Beta fixture is
    /// authored in semantic order, not alphabetic — so we sort both
    /// sides before comparing structural equality.
    fn sort_for_round_trip(design: &mut Design) {
        design.colors.sort_by(|a, b| a.name.cmp(&b.name));
        design
            .typography
            .families
            .sort_by(|a, b| a.name.cmp(&b.name));
        design.typography.scale.sort_by(|a, b| a.name.cmp(&b.name));
        design
            .typography
            .weights
            .sort_by(|a, b| a.name.cmp(&b.name));
        design
            .typography
            .tracking
            .sort_by(|a, b| a.name.cmp(&b.name));
        design.spaces.sort_by(|a, b| a.name.cmp(&b.name));
        design.radii.sort_by(|a, b| a.name.cmp(&b.name));
        design.shadows.sort_by(|a, b| a.name.cmp(&b.name));
        design.motion.durations.sort_by(|a, b| a.name.cmp(&b.name));
        design.motion.easings.sort_by(|a, b| a.name.cmp(&b.name));
        design.breakpoints.sort_by(|a, b| a.name.cmp(&b.name));
        design.z_indices.sort_by(|a, b| a.name.cmp(&b.name));
    }

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
    fn import_overwrite_false_fails_when_design_exists() {
        let tmp = unique_temp_dir("import-overwrite-false");
        let out = tmp.join("design.lzi");
        write_design(&out, &demo_fixture()).unwrap();

        let external = tmp.join("tokens.figma.json");
        fs::write(
            &external,
            serde_json::to_string_pretty(&design_to_figma(&demo_fixture())).unwrap(),
        )
        .unwrap();

        let err = import(&external, ImportFormat::Figma, &out, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--overwrite"),
            "expected --overwrite hint in error, got: {msg}"
        );

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn import_overwrite_true_rewrites_design() {
        let tmp = unique_temp_dir("import-overwrite-true");
        let out = tmp.join("design.lzi");
        // Seed `design.lzi` with a minimal Design.
        let initial = Design {
            name: "old".to_string(),
            extends: None,
            colors: Vec::new(),
            typography: Typography::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: Motion::default(),
            breakpoints: Vec::new(),
            z_indices: Vec::new(),
        };
        write_design(&out, &initial).unwrap();

        let external = tmp.join("tokens.figma.json");
        fs::write(
            &external,
            serde_json::to_string_pretty(&design_to_figma(&demo_fixture())).unwrap(),
        )
        .unwrap();

        import(&external, ImportFormat::Figma, &out, true).unwrap();

        let after = read_design(&out).unwrap();
        assert!(
            !after.colors.is_empty(),
            "import should have replaced design"
        );
        assert!(after.spaces.iter().any(|t| t.name == "1"));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn diff_detects_added_token() {
        let current = demo_fixture();
        let mut incoming = current.clone();
        incoming.spaces.push(ScaleToken {
            name: "8".to_string(),
            value: "2rem".to_string(),
        });

        let report = compute_diff(&current, &incoming);
        assert_eq!(report.added, vec!["space.8".to_string()]);
        assert!(report.removed.is_empty());
        assert!(report.changed.is_empty());
    }

    #[test]
    fn diff_detects_removed_token() {
        let current = demo_fixture();
        let mut incoming = current.clone();
        incoming.spaces.retain(|t| t.name != "1");

        let report = compute_diff(&current, &incoming);
        assert_eq!(report.removed, vec!["space.1".to_string()]);
        assert!(report.added.is_empty());
        assert!(report.changed.is_empty());
    }

    #[test]
    fn diff_detects_value_change() {
        let current = demo_fixture();
        let mut incoming = current.clone();
        for tok in &mut incoming.spaces {
            if tok.name == "4" {
                tok.value = "0.875rem".to_string();
            }
        }

        let report = compute_diff(&current, &incoming);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert_eq!(report.changed.len(), 1);
        let change = &report.changed[0];
        assert_eq!(change.path, "space.4");
        assert_eq!(change.from_value, "1rem");
        assert_eq!(change.to_value, "0.875rem");
    }

    #[test]
    fn diff_against_identical_json_is_empty() {
        let tmp = unique_temp_dir("diff-identical");
        let design = demo_fixture();
        let external = tmp.join("tokens.figma.json");
        fs::write(
            &external,
            serde_json::to_string_pretty(&design_to_figma(&design)).unwrap(),
        )
        .unwrap();

        let report = diff_with_format(&external, ImportFormat::Figma, &design).unwrap();
        assert!(
            report.is_empty(),
            "expected empty diff, got: {}",
            report.render()
        );
        let _ = fs::remove_dir_all(tmp);
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

    #[test]
    fn empty_groups_do_not_crash_export() {
        let tmp = unique_temp_dir("empty-groups");
        let out = tmp.join("tokens.figma.json");
        let empty = Design {
            name: "empty".to_string(),
            extends: None,
            colors: Vec::new(),
            typography: Typography::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: Motion::default(),
            breakpoints: Vec::new(),
            z_indices: Vec::new(),
        };
        export(&out, ExportTarget::Figma, &empty).unwrap();
        let raw = fs::read_to_string(&out).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 0);

        // Style Dictionary path too.
        let sd_path = tmp.join("tokens.sd.json");
        export(&sd_path, ExportTarget::StyleDictionary, &empty).unwrap();
        let sd_raw = fs::read_to_string(&sd_path).unwrap();
        let sd_parsed: Value = serde_json::from_str(&sd_raw).unwrap();
        assert_eq!(sd_parsed.as_object().unwrap().len(), 0);

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn invalid_json_input_returns_err() {
        let tmp = unique_temp_dir("invalid-json");
        let bad = tmp.join("garbage.figma.json");
        fs::write(&bad, "{ not valid json").unwrap();

        let out = tmp.join("design.lzi");
        let err = import(&bad, ImportFormat::Figma, &out, true).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parsing JSON") || msg.contains("expected"),
            "expected parse error message, got: {msg}"
        );

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn export_writes_deterministic_sorted_output() {
        // Two consecutive exports of the same Design must produce
        // byte-identical files — matches `lazuli generate go` discipline.
        let tmp = unique_temp_dir("deterministic");
        let design = demo_fixture();
        let a = tmp.join("a.figma.json");
        let b = tmp.join("b.figma.json");
        export(&a, ExportTarget::Figma, &design).unwrap();
        export(&b, ExportTarget::Figma, &design).unwrap();
        let a_raw = fs::read_to_string(&a).unwrap();
        let b_raw = fs::read_to_string(&b).unwrap();
        assert_eq!(a_raw, b_raw, "export must be deterministic");
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn sniff_format_detects_figma_and_style_dictionary() {
        let tmp = unique_temp_dir("sniff");
        let figma_path = tmp.join("tokens.figma.json");
        let figma_doc = json!({
            "color": {
                "primary": { "$value": "#7c3aed", "$type": "color" }
            }
        });
        fs::write(&figma_path, serde_json::to_string(&figma_doc).unwrap()).unwrap();
        assert_eq!(sniff_format(&figma_path).unwrap(), ImportFormat::Figma);

        let sd_path = tmp.join("tokens.sd.json");
        let sd_doc = json!({
            "color": {
                "primary": { "value": "#7c3aed", "type": "color" }
            }
        });
        fs::write(&sd_path, serde_json::to_string(&sd_doc).unwrap()).unwrap();
        assert_eq!(
            sniff_format(&sd_path).unwrap(),
            ImportFormat::StyleDictionary
        );

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn unknown_color_state_is_rejected() {
        let bad = json!({
            "color": {
                "primary": {
                    "base":  { "$value": "#7c3aed", "$type": "color" },
                    "weird": { "$value": "#000000", "$type": "color" }
                }
            }
        });
        let err = figma_to_design(&bad).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unknown state") || msg.contains("closed catalog"),
            "expected closed-catalog rejection, got: {msg}"
        );
    }
}
