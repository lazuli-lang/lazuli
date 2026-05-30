//! Shadow → React Native object lowering. Pulled out of mod.rs so the
//! emitter only owns the orchestration, not the parser.

use std::fmt::Write;

use super::super::ir::ShadowToken;
use super::{quote_key, ts_string};

/// Detect a top-level comma outside parentheses → multi-layer shadow.
pub(super) fn is_multi_layer(input: &str) -> bool {
    let mut depth = 0i32;
    for c in input.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

pub(super) fn emit_shadow(s: &mut String, tok: &ShadowToken) {
    let key = quote_key(&tok.name);
    if is_multi_layer(&tok.value) {
        // Don't fail the whole emission — Cell A's lowering is expected to
        // reject multi-layer shadows upstream. Emit a placeholder + a TODO
        // log so the user sees something actionable.
        writeln!(
            s,
            "    // TODO(design-shadow-multi-layer): cannot map multi-layer CSS \
             box-shadow to a single RN object. Token `{}` was: {:?}.",
            tok.name, tok.value
        )
        .ok();
        writeln!(
            s,
            "    {}: {{ shadowColor: \"#000\", shadowOffset: {{ width: 0, height: 0 }}, \
             shadowOpacity: 0, shadowRadius: 0, elevation: 0 }},",
            key
        )
        .ok();
        return;
    }

    match parse_single_shadow(&tok.value) {
        Some(parsed) => {
            writeln!(s, "    {}: {{", key).ok();
            writeln!(s, "      shadowColor: {},", ts_string(&parsed.color)).ok();
            writeln!(
                s,
                "      shadowOffset: {{ width: {}, height: {} }},",
                parsed.offset_x, parsed.offset_y
            )
            .ok();
            writeln!(
                s,
                "      shadowOpacity: {},",
                format_opacity(parsed.opacity)
            )
            .ok();
            writeln!(s, "      shadowRadius: {},", parsed.blur).ok();
            writeln!(s, "      elevation: {},", parsed.elevation).ok();
            writeln!(s, "    }},").ok();
        }
        None => {
            writeln!(
                s,
                "    // TODO(design-shadow-parse): could not parse shadow value for `{}`: {:?}.",
                tok.name, tok.value
            )
            .ok();
            writeln!(
                s,
                "    {}: {{ shadowColor: \"#000\", shadowOffset: {{ width: 0, height: 0 }}, \
                 shadowOpacity: 0, shadowRadius: 0, elevation: 0 }},",
                key
            )
            .ok();
        }
    }
}

struct ParsedShadow {
    offset_x: i32,
    offset_y: i32,
    blur: i32,
    color: String,
    opacity: f32,
    elevation: i32,
}

/// Parse the closed grammar `<offset-x> <offset-y> <blur-radius>
/// [<spread-radius>] <color>` where lengths are bare px (`1px` or `1`) and
/// color is hex / rgb / rgba / `rgb(... / α)`.
fn parse_single_shadow(input: &str) -> Option<ParsedShadow> {
    let input = input.trim();
    // Split on whitespace but keep `rgb(...)` / `rgba(...)` / `hsl(...)` as one token.
    let tokens = split_shadow_tokens(input)?;
    if tokens.len() < 4 {
        return None;
    }
    // Color is always the last token; everything before are length values.
    let color_tok = tokens.last()?.clone();
    let lens = &tokens[..tokens.len() - 1];
    if lens.len() < 3 || lens.len() > 4 {
        return None;
    }
    let offset_x = parse_length_px(&lens[0])?;
    let offset_y = parse_length_px(&lens[1])?;
    let blur = parse_length_px(&lens[2])?;
    // spread (lens[3]) intentionally ignored — RN has no spread-radius.

    let (color_hex, opacity) = parse_color(&color_tok)?;
    let elevation = approx_elevation(offset_y, blur);
    Some(ParsedShadow {
        offset_x,
        offset_y,
        blur,
        color: color_hex,
        opacity,
        elevation,
    })
}

/// Tokenize the shadow string respecting parentheses. Splits on whitespace
/// except inside `(...)`.
fn split_shadow_tokens(input: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut buf = String::new();
    for c in input.chars() {
        match c {
            '(' => {
                depth += 1;
                buf.push(c);
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                buf.push(c);
            }
            ' ' | '\t' if depth == 0 => {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
            }
            _ => buf.push(c),
        }
    }
    if depth != 0 {
        return None;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    Some(out)
}

fn parse_length_px(tok: &str) -> Option<i32> {
    let cleaned = tok.trim_end_matches("px");
    cleaned.parse::<i32>().ok()
}

fn parse_color(tok: &str) -> Option<(String, f32)> {
    // hex first
    if let Some(stripped) = tok.strip_prefix('#') {
        // assume 100% opacity for hex literals; RN supports #rgb/#rgba but we
        // factor the alpha into shadowOpacity in the rgb / rgba paths.
        return Some((format!("#{}", stripped), 1.0));
    }
    // rgb(...) / rgba(...) — extract color triple + optional alpha.
    let lower = tok.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
    {
        let body = rest.strip_suffix(')')?;
        // Body might be `r, g, b, α` (comma) or `r g b / α` (slash space).
        let (r, g, b, a) = parse_rgb_components(body)?;
        let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
        return Some((hex, a));
    }
    None
}

fn parse_rgb_components(body: &str) -> Option<(u8, u8, u8, f32)> {
    let normalized = body.replace([',', '/'], " ");
    let parts: Vec<&str> = normalized.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parts[0].parse::<u8>().ok()?;
    let g = parts[1].parse::<u8>().ok()?;
    let b = parts[2].parse::<u8>().ok()?;
    let a = if parts.len() >= 4 {
        parts[3].parse::<f32>().ok()?
    } else {
        1.0
    };
    Some((r, g, b, a))
}

fn format_opacity(o: f32) -> String {
    // Trim trailing zeros, keep at least one decimal place.
    let rounded = (o * 1000.0).round() / 1000.0;
    let mut formatted = format!("{}", rounded);
    if !formatted.contains('.') {
        formatted.push_str(".0");
    }
    formatted
}

fn approx_elevation(offset_y: i32, blur: i32) -> i32 {
    // Common heuristic across token compilers: Android elevation roughly
    // matches the sum of offset-y and half the blur radius, capped at 24.
    let raw = offset_y + (blur / 2);
    raw.clamp(0, 24)
}
