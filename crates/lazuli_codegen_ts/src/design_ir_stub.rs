//! Temporary stub of the `lazuli_ir::Design` IR contract — Cell A (parser +
//! lowering) lands the canonical types in `lazuli_ir`. This stub exists so the
//! emitter crate compiles standalone in the L0 #2 Cell B worktree. The
//! orchestrator reconciles at cherry-pick time: when Cell A lands first, this
//! file is replaced by `use lazuli_ir::design::*;` re-exports; if Cell B lands
//! first, Cell A's types must match this shape (see proposal docs/proposals/
//! design-tokens.md §4 + the cell prompt's "Canonical IR shape" block).
//!
//! Shape is intentionally minimal — no Serialize/Deserialize, no schema
//! versioning, no diagnostics. The emitters only need a read-only data view.
//!
//! All fields are public so emitters can pattern-match without accessor noise.

/// Span back-reference; kept structurally identical to `lazuli_ir::SpanRef` so
/// the eventual swap is a no-op for callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanRef {
    pub start: usize,
    pub end: usize,
}

/// Canonical `Design` IR shape — see `docs/proposals/design-tokens.md` §4
/// (Pleiades fixture in §8.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    pub name: String,
    pub extends: Option<String>,
    pub colors: Vec<ColorToken>,
    pub typography: Typography,
    pub spaces: Vec<ScaleToken>,
    pub radii: Vec<ScaleToken>,
    pub shadows: Vec<ShadowToken>,
    pub motion: Motion,
    pub breakpoints: Vec<ScaleToken>,
    pub z_indices: Vec<ZToken>,
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorToken {
    pub name: String,
    pub states: Vec<ColorState>,
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorState {
    pub kind: ColorStateKind,
    pub value: String,
    pub dark: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorStateKind {
    Base,
    Hover,
    Active,
    Foreground,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Typography {
    pub families: Vec<FamilyToken>,
    pub scale: Vec<TextScaleToken>,
    pub weights: Vec<WeightToken>,
    pub tracking: Vec<TrackingToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextScaleToken {
    pub name: String,
    pub size: String,
    pub line_height: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeightToken {
    pub name: String,
    pub value: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackingToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScaleToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Motion {
    pub durations: Vec<ScaleToken>,
    pub easings: Vec<EasingToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EasingToken {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZToken {
    pub name: String,
    pub value: i32,
}
