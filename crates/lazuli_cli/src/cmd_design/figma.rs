//! W3C Design Tokens (Figma Tokens Studio) codec.
//!
//! Round-trips `Design` IR ↔ Figma Tokens Studio JSON. The Figma
//! shape is the **canonical** external surface — every leaf carries
//! `$value` + `$type` keys (W3C spec). Dark-mode variants ride along
//! in `$extensions.com.lazuli.dark`, the Lazuli-owned extension key.
//!
//! Closed catalogs are enforced on the parse side:
//! - Color state names: `base` / `hover` / `active` / `foreground`
//!   only (unknown keys reject).
//! - Token groups: `color`, `typography.{family,scale,weight,tracking}`,
//!   `space`, `radius`, `breakpoint`, `shadow`, `motion.{duration,easing}`,
//!   `z`. Anything else is silently ignored — open extension point
//!   for future groups without breaking existing fixtures.

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};

use super::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};

const EXT_LAZULI_DARK: &str = "com.lazuli.dark";

pub(super) fn design_to_figma(design: &Design) -> Value {
    let mut root = Map::new();

    // color
    if !design.colors.is_empty() {
        let mut color = Map::new();
        for tok in &design.colors {
            // Flat form: a single `Base` state.
            if tok.states.len() == 1 && tok.states[0].kind == ColorStateKind::Base {
                let state = &tok.states[0];
                color.insert(
                    tok.name.clone(),
                    color_leaf(&state.value, state.dark.as_deref()),
                );
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

pub(super) fn color_state_key(kind: ColorStateKind) -> &'static str {
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

pub(super) fn figma_to_design(value: &Value) -> Result<Design> {
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
            let value =
                read_int_value(entry, true)?.ok_or_else(|| anyhow!("z.{name}: missing $value"))?;
            design.z_indices.push(ZToken {
                name: name.clone(),
                value,
            });
        }
    }

    Ok(design)
}

fn parse_color_token(name: &str, value: &Value, figma: bool) -> Result<ColorToken> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("color.{name}: expected object (single value or sub-block)"))?;

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
            let hex = read_scalar_value(entry, figma)?
                .ok_or_else(|| anyhow!("color.{name}.{key}: missing {value_key}"))?;
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
            let value = read_scalar_value(entry, figma)?
                .ok_or_else(|| anyhow!("typography.family.{name}: missing {value_key}"))?;
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
                .ok_or_else(|| anyhow!("typography.scale.{name}: missing fontSize"))?
                .to_string();
            let line_height = inner
                .get("lineHeight")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("typography.scale.{name}: missing lineHeight"))?
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
            let raw = entry
                .get(value_key)
                .ok_or_else(|| anyhow!("typography.weight.{name}: missing {value_key}"))?;
            let n = raw
                .as_u64()
                .or_else(|| raw.as_str().and_then(|s| s.parse::<u64>().ok()))
                .ok_or_else(|| anyhow!("typography.weight.{name}: expected integer (got {raw})"))?;
            typography.weights.push(WeightToken {
                name: name.clone(),
                value: u16::try_from(n).map_err(|_| {
                    anyhow!("typography.weight.{name}: weight {n} exceeds u16 range")
                })?,
            });
        }
    }
    if let Some(tracking) = map.get("tracking").and_then(|v| v.as_object()) {
        for (name, entry) in tracking {
            let value = read_scalar_value(entry, figma)?
                .ok_or_else(|| anyhow!("typography.tracking.{name}: missing {value_key}"))?;
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
    i32::try_from(n)
        .map(Some)
        .map_err(|_| anyhow!("z value {n} exceeds i32 range"))
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
