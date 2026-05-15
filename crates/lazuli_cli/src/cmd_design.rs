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

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};

pub use ir::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};

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
            format!(
                "reading existing {} to compute import diff",
                out.display()
            )
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
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    write_design(out, &incoming)
        .with_context(|| format!("writing {}", out.display()))?;
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
    let pretty = serde_json::to_string_pretty(&value)
        .context("serialising token catalog to JSON")?;

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    fs::write(out, pretty.as_bytes())
        .with_context(|| format!("writing {}", out.display()))?;
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
    let pretty = serde_json::to_string_pretty(design)
        .context("serialising Design IR to JSON")?;
    fs::write(path, pretty.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

// =============================================================================
// Format detection
// =============================================================================

/// Heuristic: filename `*.figma.json` => Figma; `*.sd.json` => Style
/// Dictionary; otherwise inspect the JSON for the W3C `$value` /
/// `$type` markers.
fn sniff_format(path: &Path) -> Result<ImportFormat> {
    let lower = path
        .to_string_lossy()
        .to_ascii_lowercase();
    if lower.ends_with(".figma.json") {
        return Ok(ImportFormat::Figma);
    }
    if lower.ends_with(".sd.json") || lower.contains("style-dictionary") {
        return Ok(ImportFormat::StyleDictionary);
    }
    // Fallback to JSON inspection. Look for any `$value` field anywhere
    // in the tree — present in W3C, absent in Style Dictionary.
    let raw = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing JSON at {}", path.display()))?;
    if json_contains_key(&value, "$value") {
        Ok(ImportFormat::Figma)
    } else if json_contains_key(&value, "value") {
        Ok(ImportFormat::StyleDictionary)
    } else {
        Err(anyhow!(
            "could not sniff design-token format for {}: pass --format figma|style-dictionary",
            path.display()
        ))
    }
}

fn json_contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                return true;
            }
            map.values().any(|child| json_contains_key(child, key))
        }
        Value::Array(items) => items.iter().any(|child| json_contains_key(child, key)),
        _ => false,
    }
}

// =============================================================================
// W3C Design Tokens (Figma Tokens Studio) ↔ Design
// =============================================================================

const EXT_LAZULI_DARK: &str = "com.lazuli.dark";

fn design_to_figma(design: &Design) -> Value {
    let mut root = Map::new();

    // color
    if !design.colors.is_empty() {
        let mut color = Map::new();
        for tok in &design.colors {
            // Flat form: a single `Base` state.
            if tok.states.len() == 1 && tok.states[0].kind == ColorStateKind::Base {
                let state = &tok.states[0];
                color.insert(tok.name.clone(), color_leaf(&state.value, state.dark.as_deref()));
            } else {
                let mut sub = Map::new();
                for state in &tok.states {
                    sub.insert(
                        color_state_key(state.kind).to_string(),
                        color_leaf(&state.value, state.dark.as_deref()),
                    );
                }
                color.insert(tok.name.clone(), Value::Object(sub));
            }
        }
        root.insert("color".to_string(), Value::Object(color));
    }

    // typography
    if let Some(obj) = typography_to_figma(&design.typography) {
        root.insert("typography".to_string(), obj);
    }

    // space / radius / breakpoint — dimension-typed flat groups
    insert_dimension_group(&mut root, "space", &design.spaces);
    insert_dimension_group(&mut root, "radius", &design.radii);
    insert_dimension_group(&mut root, "breakpoint", &design.breakpoints);

    // shadow
    if !design.shadows.is_empty() {
        let mut shadow = Map::new();
        for tok in &design.shadows {
            shadow.insert(tok.name.clone(), typed_leaf(&tok.value, "shadow"));
        }
        root.insert("shadow".to_string(), Value::Object(shadow));
    }

    // motion
    if let Some(obj) = motion_to_figma(&design.motion) {
        root.insert("motion".to_string(), obj);
    }

    // z
    if !design.z_indices.is_empty() {
        let mut z = Map::new();
        for tok in &design.z_indices {
            z.insert(
                tok.name.clone(),
                json!({ "$value": tok.value, "$type": "number" }),
            );
        }
        root.insert("z".to_string(), Value::Object(z));
    }

    Value::Object(root)
}

fn insert_dimension_group(root: &mut Map<String, Value>, group: &str, tokens: &[ScaleToken]) {
    if tokens.is_empty() {
        return;
    }
    let mut map = Map::new();
    for tok in tokens {
        map.insert(tok.name.clone(), typed_leaf(&tok.value, "dimension"));
    }
    root.insert(group.to_string(), Value::Object(map));
}

fn typography_to_figma(typography: &Typography) -> Option<Value> {
    if typography.families.is_empty()
        && typography.scale.is_empty()
        && typography.weights.is_empty()
        && typography.tracking.is_empty()
    {
        return None;
    }
    let mut typ = Map::new();

    if !typography.families.is_empty() {
        let mut family = Map::new();
        for tok in &typography.families {
            family.insert(tok.name.clone(), typed_leaf(&tok.value, "fontFamily"));
        }
        typ.insert("family".to_string(), Value::Object(family));
    }
    if !typography.scale.is_empty() {
        let mut scale = Map::new();
        for tok in &typography.scale {
            scale.insert(
                tok.name.clone(),
                json!({
                    "$value": { "fontSize": tok.size, "lineHeight": tok.line_height },
                    "$type": "typography",
                }),
            );
        }
        typ.insert("scale".to_string(), Value::Object(scale));
    }
    if !typography.weights.is_empty() {
        let mut weight = Map::new();
        for tok in &typography.weights {
            weight.insert(
                tok.name.clone(),
                json!({ "$value": tok.value, "$type": "fontWeight" }),
            );
        }
        typ.insert("weight".to_string(), Value::Object(weight));
    }
    if !typography.tracking.is_empty() {
        let mut tracking = Map::new();
        for tok in &typography.tracking {
            tracking.insert(tok.name.clone(), typed_leaf(&tok.value, "dimension"));
        }
        typ.insert("tracking".to_string(), Value::Object(tracking));
    }

    Some(Value::Object(typ))
}

fn motion_to_figma(motion: &Motion) -> Option<Value> {
    if motion.durations.is_empty() && motion.easings.is_empty() {
        return None;
    }
    let mut m = Map::new();
    if !motion.durations.is_empty() {
        let mut dur = Map::new();
        for tok in &motion.durations {
            dur.insert(tok.name.clone(), typed_leaf(&tok.value, "duration"));
        }
        m.insert("duration".to_string(), Value::Object(dur));
    }
    if !motion.easings.is_empty() {
        let mut eas = Map::new();
        for tok in &motion.easings {
            eas.insert(tok.name.clone(), typed_leaf(&tok.value, "cubicBezier"));
        }
        m.insert("easing".to_string(), Value::Object(eas));
    }
    Some(Value::Object(m))
}

fn typed_leaf(value: &str, type_name: &str) -> Value {
    json!({ "$value": value, "$type": type_name })
}

fn color_leaf(value: &str, dark: Option<&str>) -> Value {
    let mut leaf = Map::new();
    leaf.insert("$value".to_string(), Value::String(value.to_string()));
    leaf.insert("$type".to_string(), Value::String("color".to_string()));
    if let Some(dark) = dark {
        leaf.insert(
            "$extensions".to_string(),
            json!({ EXT_LAZULI_DARK: { "value": dark } }),
        );
    }
    Value::Object(leaf)
}

fn color_state_key(kind: ColorStateKind) -> &'static str {
    match kind {
        ColorStateKind::Base => "base",
        ColorStateKind::Hover => "hover",
        ColorStateKind::Active => "active",
        ColorStateKind::Foreground => "foreground",
    }
}

fn parse_color_state_key(key: &str) -> Option<ColorStateKind> {
    match key {
        "base" => Some(ColorStateKind::Base),
        "hover" => Some(ColorStateKind::Hover),
        "active" => Some(ColorStateKind::Active),
        "foreground" => Some(ColorStateKind::Foreground),
        _ => None,
    }
}

fn figma_to_design(value: &Value) -> Result<Design> {
    let root = value
        .as_object()
        .ok_or_else(|| anyhow!("expected JSON object at root"))?;

    let mut design = Design {
        name: "imported".to_string(),
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

    if let Some(color) = root.get("color").and_then(|v| v.as_object()) {
        for (name, entry) in color {
            design.colors.push(parse_color_token(name, entry, true)?);
        }
    }

    if let Some(typography) = root.get("typography").and_then(|v| v.as_object()) {
        design.typography = parse_typography(typography, true)?;
    }

    if let Some(space) = root.get("space").and_then(|v| v.as_object()) {
        design.spaces = parse_scale_group(space, true)?;
    }
    if let Some(radius) = root.get("radius").and_then(|v| v.as_object()) {
        design.radii = parse_scale_group(radius, true)?;
    }
    if let Some(breakpoint) = root.get("breakpoint").and_then(|v| v.as_object()) {
        design.breakpoints = parse_scale_group(breakpoint, true)?;
    }
    if let Some(shadow) = root.get("shadow").and_then(|v| v.as_object()) {
        for (name, entry) in shadow {
            let value = read_scalar_value(entry, true)?
                .ok_or_else(|| anyhow!("shadow.{name}: missing $value"))?;
            design.shadows.push(ShadowToken {
                name: name.clone(),
                value,
            });
        }
    }
    if let Some(motion) = root.get("motion").and_then(|v| v.as_object()) {
        if let Some(dur) = motion.get("duration").and_then(|v| v.as_object()) {
            design.motion.durations = parse_scale_group(dur, true)?;
        }
        if let Some(eas) = motion.get("easing").and_then(|v| v.as_object()) {
            for (name, entry) in eas {
                let value = read_scalar_value(entry, true)?
                    .ok_or_else(|| anyhow!("motion.easing.{name}: missing $value"))?;
                design.motion.easings.push(EasingToken {
                    name: name.clone(),
                    value,
                });
            }
        }
    }
    if let Some(z) = root.get("z").and_then(|v| v.as_object()) {
        for (name, entry) in z {
            let value = read_int_value(entry, true)?
                .ok_or_else(|| anyhow!("z.{name}: missing $value"))?;
            design.z_indices.push(ZToken {
                name: name.clone(),
                value,
            });
        }
    }

    Ok(design)
}

fn parse_color_token(name: &str, value: &Value, figma: bool) -> Result<ColorToken> {
    let obj = value.as_object().ok_or_else(|| {
        anyhow!("color.{name}: expected object (single value or sub-block)")
    })?;

    let value_key = if figma { "$value" } else { "value" };

    // Flat form: object has `$value`/`value` directly.
    if obj.contains_key(value_key) {
        let hex = read_scalar_value(value, figma)?
            .ok_or_else(|| anyhow!("color.{name}: missing $value"))?;
        let dark = read_dark_extension(value, figma);
        return Ok(ColorToken {
            name: name.to_string(),
            states: vec![ColorState {
                kind: ColorStateKind::Base,
                value: hex,
                dark,
            }],
        });
    }

    // Sub-block: keys are state names (`base`, `hover`, etc.).
    let mut states = Vec::new();
    // Preserve canonical state order so round-trip stays stable.
    for kind in [
        ColorStateKind::Base,
        ColorStateKind::Hover,
        ColorStateKind::Active,
        ColorStateKind::Foreground,
    ] {
        let key = color_state_key(kind);
        if let Some(entry) = obj.get(key) {
            let hex = read_scalar_value(entry, figma)?.ok_or_else(|| {
                anyhow!("color.{name}.{key}: missing {value_key}")
            })?;
            let dark = read_dark_extension(entry, figma);
            states.push(ColorState {
                kind,
                value: hex,
                dark,
            });
        }
    }
    // Reject unknown state keys outright — closed catalog.
    for key in obj.keys() {
        if parse_color_state_key(key).is_none() {
            bail!(
                "color.{name}: unknown state '{key}' (closed catalog: base/hover/active/foreground)"
            );
        }
    }
    if states.is_empty() {
        bail!("color.{name}: empty sub-block — declare at least `base`");
    }
    Ok(ColorToken {
        name: name.to_string(),
        states,
    })
}

fn parse_typography(map: &Map<String, Value>, figma: bool) -> Result<Typography> {
    let mut typography = Typography::default();
    let value_key = if figma { "$value" } else { "value" };

    if let Some(family) = map.get("family").and_then(|v| v.as_object()) {
        for (name, entry) in family {
            let value = read_scalar_value(entry, figma)?.ok_or_else(|| {
                anyhow!("typography.family.{name}: missing {value_key}")
            })?;
            typography.families.push(FamilyToken {
                name: name.clone(),
                value,
            });
        }
    }
    if let Some(scale) = map.get("scale").and_then(|v| v.as_object()) {
        for (name, entry) in scale {
            let inner = entry
                .get(value_key)
                .and_then(|v| v.as_object())
                .ok_or_else(|| {
                    anyhow!(
                        "typography.scale.{name}: expected {value_key} object with fontSize/lineHeight"
                    )
                })?;
            let size = inner
                .get("fontSize")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!("typography.scale.{name}: missing fontSize")
                })?
                .to_string();
            let line_height = inner
                .get("lineHeight")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!("typography.scale.{name}: missing lineHeight")
                })?
                .to_string();
            typography.scale.push(TextScaleToken {
                name: name.clone(),
                size,
                line_height,
            });
        }
    }
    if let Some(weight) = map.get("weight").and_then(|v| v.as_object()) {
        for (name, entry) in weight {
            let raw = entry.get(value_key).ok_or_else(|| {
                anyhow!("typography.weight.{name}: missing {value_key}")
            })?;
            let n = raw
                .as_u64()
                .or_else(|| raw.as_str().and_then(|s| s.parse::<u64>().ok()))
                .ok_or_else(|| {
                    anyhow!(
                        "typography.weight.{name}: expected integer (got {raw})"
                    )
                })?;
            typography.weights.push(WeightToken {
                name: name.clone(),
                value: u16::try_from(n).map_err(|_| {
                    anyhow!(
                        "typography.weight.{name}: weight {n} exceeds u16 range"
                    )
                })?,
            });
        }
    }
    if let Some(tracking) = map.get("tracking").and_then(|v| v.as_object()) {
        for (name, entry) in tracking {
            let value = read_scalar_value(entry, figma)?.ok_or_else(|| {
                anyhow!("typography.tracking.{name}: missing {value_key}")
            })?;
            typography.tracking.push(TrackingToken {
                name: name.clone(),
                value,
            });
        }
    }
    Ok(typography)
}

fn parse_scale_group(map: &Map<String, Value>, figma: bool) -> Result<Vec<ScaleToken>> {
    let mut out = Vec::new();
    let value_key = if figma { "$value" } else { "value" };
    for (name, entry) in map {
        let value = read_scalar_value(entry, figma)?
            .ok_or_else(|| anyhow!("{name}: missing {value_key}"))?;
        out.push(ScaleToken {
            name: name.clone(),
            value,
        });
    }
    Ok(out)
}

fn read_scalar_value(entry: &Value, figma: bool) -> Result<Option<String>> {
    let value_key = if figma { "$value" } else { "value" };
    let Some(raw) = entry.get(value_key) else {
        return Ok(None);
    };
    Ok(Some(value_to_string(raw)))
}

fn read_int_value(entry: &Value, figma: bool) -> Result<Option<i32>> {
    let value_key = if figma { "$value" } else { "value" };
    let Some(raw) = entry.get(value_key) else {
        return Ok(None);
    };
    let n = raw
        .as_i64()
        .or_else(|| raw.as_str().and_then(|s| s.parse::<i64>().ok()))
        .ok_or_else(|| anyhow!("expected integer, got {raw}"))?;
    i32::try_from(n).map(Some).map_err(|_| {
        anyhow!("z value {n} exceeds i32 range")
    })
}

fn value_to_string(raw: &Value) -> String {
    match raw {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn read_dark_extension(entry: &Value, figma: bool) -> Option<String> {
    let ext_key = if figma { "$extensions" } else { "extensions" };
    let ext = entry.get(ext_key)?.as_object()?;
    let dark = ext.get(EXT_LAZULI_DARK)?;
    // Accept either `{ "value": "#..." }` or a bare string.
    if let Some(value) = dark.as_str() {
        return Some(value.to_string());
    }
    let obj = dark.as_object()?;
    let inner_key = if figma { "$value" } else { "value" };
    obj.get(inner_key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            obj.get("value")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

// =============================================================================
// Style Dictionary ↔ Design
// =============================================================================

fn design_to_style_dictionary(design: &Design) -> Value {
    let figma = design_to_figma(design);
    convert_keys_dollar_to_plain(figma)
}

fn style_dictionary_to_design(value: &Value) -> Result<Design> {
    let figma = convert_keys_plain_to_dollar(value.clone());
    figma_to_design(&figma)
}

/// Translates `$value` / `$type` / `$extensions` -> `value` / `type` /
/// `extensions`. Style Dictionary's source format mirrors W3C structurally
/// but drops the `$` prefix.
fn convert_keys_dollar_to_plain(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                let new_key = match key.as_str() {
                    "$value" => "value".to_string(),
                    "$type" => "type".to_string(),
                    "$extensions" => "extensions".to_string(),
                    _ => key,
                };
                out.insert(new_key, convert_keys_dollar_to_plain(child));
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.into_iter().map(convert_keys_dollar_to_plain).collect())
        }
        other => other,
    }
}

fn convert_keys_plain_to_dollar(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                let new_key = match key.as_str() {
                    "value" => "$value".to_string(),
                    "type" => "$type".to_string(),
                    "extensions" => "$extensions".to_string(),
                    _ => key,
                };
                out.insert(new_key, convert_keys_plain_to_dollar(child));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(convert_keys_plain_to_dollar)
                .collect(),
        ),
        other => other,
    }
}

// =============================================================================
// Flat-path view + diff
// =============================================================================

fn flat_view(design: &Design) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for tok in &design.colors {
        if tok.states.len() == 1 && tok.states[0].kind == ColorStateKind::Base {
            let state = &tok.states[0];
            out.insert(format!("color.{}", tok.name), encode_color(state));
        } else {
            for state in &tok.states {
                let key = format!(
                    "color.{}.{}",
                    tok.name,
                    color_state_key(state.kind)
                );
                out.insert(key, encode_color(state));
            }
        }
    }
    for tok in &design.typography.families {
        out.insert(format!("typography.family.{}", tok.name), tok.value.clone());
    }
    for tok in &design.typography.scale {
        out.insert(
            format!("typography.scale.{}", tok.name),
            format!("{}|{}", tok.size, tok.line_height),
        );
    }
    for tok in &design.typography.weights {
        out.insert(
            format!("typography.weight.{}", tok.name),
            tok.value.to_string(),
        );
    }
    for tok in &design.typography.tracking {
        out.insert(
            format!("typography.tracking.{}", tok.name),
            tok.value.clone(),
        );
    }
    for tok in &design.spaces {
        out.insert(format!("space.{}", tok.name), tok.value.clone());
    }
    for tok in &design.radii {
        out.insert(format!("radius.{}", tok.name), tok.value.clone());
    }
    for tok in &design.shadows {
        out.insert(format!("shadow.{}", tok.name), tok.value.clone());
    }
    for tok in &design.motion.durations {
        out.insert(
            format!("motion.duration.{}", tok.name),
            tok.value.clone(),
        );
    }
    for tok in &design.motion.easings {
        out.insert(
            format!("motion.easing.{}", tok.name),
            tok.value.clone(),
        );
    }
    for tok in &design.breakpoints {
        out.insert(format!("breakpoint.{}", tok.name), tok.value.clone());
    }
    for tok in &design.z_indices {
        out.insert(format!("z.{}", tok.name), tok.value.to_string());
    }
    out
}

fn encode_color(state: &ColorState) -> String {
    match &state.dark {
        Some(dark) => format!("{}|dark={}", state.value, dark),
        None => state.value.clone(),
    }
}

fn compute_diff(current: &Design, incoming: &Design) -> DiffReport {
    let lhs = flat_view(current);
    let rhs = flat_view(incoming);

    let mut report = DiffReport::default();
    for (path, value) in &rhs {
        match lhs.get(path) {
            None => report.added.push(path.clone()),
            Some(existing) if existing != value => report.changed.push(TokenDiff {
                path: path.clone(),
                from_value: existing.clone(),
                to_value: value.clone(),
            }),
            _ => {}
        }
    }
    for path in lhs.keys() {
        if !rhs.contains_key(path) {
            report.removed.push(path.clone());
        }
    }
    report.added.sort();
    report.removed.sort();
    report.changed.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

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
    use std::time::{SystemTime, UNIX_EPOCH};

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

    /// Pleiades-style fixture covering every group + dark variants + a
    /// multi-state color sub-block.
    fn pleiades_fixture() -> Design {
        Design {
            name: "pleiades".to_string(),
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
    /// re-import sorts by key. The original Pleiades fixture is
    /// authored in semantic order, not alphabetic — so we sort both
    /// sides before comparing structural equality.
    fn sort_for_round_trip(design: &mut Design) {
        design.colors.sort_by(|a, b| a.name.cmp(&b.name));
        design.typography.families.sort_by(|a, b| a.name.cmp(&b.name));
        design.typography.scale.sort_by(|a, b| a.name.cmp(&b.name));
        design.typography.weights.sort_by(|a, b| a.name.cmp(&b.name));
        design.typography.tracking.sort_by(|a, b| a.name.cmp(&b.name));
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
        let original = pleiades_fixture();
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
        let original = pleiades_fixture();
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
        write_design(&out, &pleiades_fixture()).unwrap();

        let external = tmp.join("tokens.figma.json");
        fs::write(
            &external,
            serde_json::to_string_pretty(&design_to_figma(&pleiades_fixture())).unwrap(),
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
            serde_json::to_string_pretty(&design_to_figma(&pleiades_fixture())).unwrap(),
        )
        .unwrap();

        import(&external, ImportFormat::Figma, &out, true).unwrap();

        let after = read_design(&out).unwrap();
        assert!(!after.colors.is_empty(), "import should have replaced design");
        assert!(after.spaces.iter().any(|t| t.name == "1"));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn diff_detects_added_token() {
        let current = pleiades_fixture();
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
        let current = pleiades_fixture();
        let mut incoming = current.clone();
        incoming.spaces.retain(|t| t.name != "1");

        let report = compute_diff(&current, &incoming);
        assert_eq!(report.removed, vec!["space.1".to_string()]);
        assert!(report.added.is_empty());
        assert!(report.changed.is_empty());
    }

    #[test]
    fn diff_detects_value_change() {
        let current = pleiades_fixture();
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
        let design = pleiades_fixture();
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
        let mut design = pleiades_fixture();
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
        let mut design = pleiades_fixture();
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
        let design = pleiades_fixture();
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
