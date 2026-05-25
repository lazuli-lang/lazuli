//! `design.lzi` parser — the project's design-token vocabulary.
//!
//! Single `design <name>` block at indent 0 carrying eight closed-catalog
//! groups plus the v0 `custom` extension:
//!
//! - `color` — named tokens with optional `base`/`hover`/`active`/`foreground`
//!   states, each supporting a `dark` override.
//! - `typography` — `family`, `scale` (size + line_height pairs), `weight`,
//!   `tracking` sub-groups.
//! - `space`, `radius`, `breakpoint` — flat `<name> <value>` scales.
//! - `shadow` — quoted CSS box-shadow strings.
//! - `motion` — `duration` + `easing` sub-groups.
//! - `z` — integer z-index tokens.
//! - `custom` — kebab-named ad-hoc hex tokens
//!   (`docs/proposals/design-tokens-custom.md` §2). Lowering enforces hex
//!   validity and the reserved-name / collision rules; this parser only
//!   captures the surface.
//!
//! ## Lexical quirks
//!
//! Token names that start with a digit (`"2xl"`, `"3xl"`, `"16"`) MUST be
//! quoted (canonical-semantics §3.1). `split_design_name` honours both
//! quoted and bare idents.
//!
//! ## See also
//!
//! - `lazuli_ir::nodes::design` — the typed lowering target.
//! - `docs/proposals/design-tokens-custom.md` — the `custom` catalog.

use super::super::common::{
    SourceLine, find_top_level_token, is_trivia, line_error, line_error_owned, source_lines,
    strip_inline_comment,
};
use super::super::error::ParseError;

use crate::ast::{
    ColorStateAst, ColorTokenAst, CustomTokenAst, DesignDeclAst, EasingTokenAst, FamilyTokenAst,
    MotionAst, ScaleTokenAst, ShadowTokenAst, Span, TextScaleTokenAst, TrackingTokenAst,
    TypographyAst, WeightTokenAst, ZTokenAst,
};

// -----------------------------------------------------------------------------
// L0 #2 — `design.lzi` parser.
//
// `design.lzi` lives at project root. The file declares one `design <name>`
// block at indent 0; children sit at indent 2 (one of the eight closed
// groups), grandchildren at indent 4 (token entries inside groups), and
// great-grandchildren at indent 6 (state entries inside a color sub-block,
// or sub-group entries inside `typography` / `motion`).
//
// Surface forms (parser, not lowering):
//
//   color
//     primary
//       base "#7c3aed"
//       hover "#6d28d9"
//       active "#5b21b6" dark "#7c3aed"
//       foreground "#ffffff"
//     success "#16a34a"             # flat (single state, treated as `base`)
//
//   typography
//     family
//       sans "Inter, system-ui, sans-serif"
//     scale
//       base size 1rem, line_height 1.5rem
//       "2xl" size 1.5rem, line_height 2rem
//     weight
//       regular 400
//     tracking
//       tight -0.025em
//
//   space     <name> <value>
//   radius    <name> <value>
//   shadow    <name> "<value>"
//   motion
//     duration <name> <value>
//     easing   <name> "<value>"
//   breakpoint <name> <value>
//   z          <name> <integer>
//
// Names that start with a digit (`"2xl"`, `"3xl"`, `"16"`) MUST be quoted
// per the lexical rule in §3.1. Unquoted idents preserve the existing
// `IDENT_LOWER` snake_case convention.
// -----------------------------------------------------------------------------

/// Entry point: parse a complete `design.lzi` source. Skips trivia,
/// expects exactly one `design <name>` block at indent 0.
pub fn parse_design_document(source: &str) -> Result<DesignDeclAst, ParseError> {
    let lines = source_lines(source);
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `design` declaration must start at indent 0",
            ));
        }
        if trimmed.starts_with("design ") || trimmed == "design" {
            let (parsed, _next) = parse_design_decl(&lines, i)?;
            return Ok(parsed);
        }
        return Err(line_error(
            line,
            "`design.lzi` must begin with a `design <name>` declaration",
        ));
    }
    Err(ParseError::Expected {
        expected: "design <name> declaration",
    })
}

/// Parse a `design <name>` block starting at `lines[start]`. Returns the
/// AST + the index of the first line not consumed. Module-private to
/// match `SourceLine`'s scope; callers use the `parse_design_document`
/// source-text entry point.
fn parse_design_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(DesignDeclAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let name = header_text
        .strip_prefix("design ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "design header must be `design <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "design header requires a name"));
    }
    let header_indent = header.indent;
    let group_indent = header_indent + 2;

    let mut extends: Option<String> = None;
    let mut colors: Vec<ColorTokenAst> = Vec::new();
    let mut typography = TypographyAst::default();
    let mut spaces: Vec<ScaleTokenAst> = Vec::new();
    let mut radii: Vec<ScaleTokenAst> = Vec::new();
    let mut shadows: Vec<ShadowTokenAst> = Vec::new();
    let mut motion = MotionAst::default();
    let mut breakpoints: Vec<ScaleTokenAst> = Vec::new();
    let mut z_indices: Vec<ZTokenAst> = Vec::new();
    let mut custom: Vec<CustomTokenAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != group_indent {
            return Err(line_error(
                line,
                "design body children use one indentation level deeper than the `design` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();

        if let Some(rest) = trimmed.strip_prefix("extends ") {
            let target = rest.trim().to_owned();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`extends` requires a base design name (e.g. `extends base`)",
                ));
            }
            extends = Some(target);
            last_end = line.end;
            i += 1;
        } else if trimmed == "color" {
            let (parsed, next) = parse_design_color_group(lines, i, line.indent + 2)?;
            colors.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "typography" {
            let (parsed, next) = parse_design_typography(lines, i, line.indent + 2)?;
            typography = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "space" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            spaces = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "radius" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            radii = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "shadow" {
            let (parsed, next) = parse_design_shadow_group(lines, i, line.indent + 2)?;
            shadows = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "motion" {
            let (parsed, next) = parse_design_motion(lines, i, line.indent + 2)?;
            motion = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "breakpoint" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            breakpoints = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "z" {
            let (parsed, next) = parse_design_z_group(lines, i, line.indent + 2)?;
            z_indices = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "custom" {
            // L0 #2 — `custom` 9th meta-group per
            // `docs/proposals/design-tokens-custom.md` §2. Flat sub-grammar.
            let (parsed, next) = parse_design_custom_group(lines, i, line.indent + 2)?;
            custom = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "design children are `extends`, `color`, `typography`, `space`, `radius`, `shadow`, `motion`, `breakpoint`, `z`, or `custom`",
            ));
        }
    }

    Ok((
        DesignDeclAst {
            name,
            extends,
            colors,
            typography,
            spaces,
            radii,
            shadows,
            motion,
            breakpoints,
            z_indices,
            custom,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse the body of `color` (group of named entries, each either flat
/// `<name> "<hex>"` or sub-block with state lines).
fn parse_design_color_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ColorTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let state_indent = child_indent + 2;
    let mut colors: Vec<ColorTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "color entries use one indentation level deeper than the `color` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;

        // Disambiguate flat vs sub-block: if `after` is empty (after stripping
        // trailing whitespace), this is a sub-block header; otherwise the
        // remainder is the flat-form hex value (with optional `dark <hex>`).
        let after = after.trim();
        if after.is_empty() {
            let entry_start = line.start;
            let (states, next, last_end) =
                parse_design_color_states(lines, i + 1, state_indent, child_indent)?;
            if states.is_empty() {
                return Err(line_error(
                    line,
                    "color sub-block requires at least one of `base`, `hover`, `active`, `foreground`",
                ));
            }
            colors.push(ColorTokenAst {
                name,
                states,
                span: Span::new(entry_start, last_end),
            });
            i = next;
        } else {
            // Flat form: `<name> "<hex>" [dark "<hex>"]`. Treat the value as
            // an implicit `base` state.
            let (value, dark) = parse_color_value_with_dark(line, after)?;
            colors.push(ColorTokenAst {
                name,
                states: vec![ColorStateAst {
                    kind: "base".to_owned(),
                    value,
                    dark,
                }],
                span: Span::new(line.start, line.end),
            });
            i += 1;
        }
    }

    Ok((colors, i))
}

/// Parse a sequence of `base | hover | active | foreground "<hex>" [dark
/// "<hex>"]` lines at `state_indent` until we leave the parent block.
fn parse_design_color_states(
    lines: &[SourceLine<'_>],
    start: usize,
    state_indent: usize,
    parent_indent: usize,
) -> Result<(Vec<ColorStateAst>, usize, usize), ParseError> {
    let mut states: Vec<ColorStateAst> = Vec::new();
    let mut i = start;
    let mut last_end = if start == 0 { 0 } else { lines[start - 1].end };
    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != state_indent {
            return Err(line_error(
                line,
                "color state entries use one indentation level deeper than the color sub-block name",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (kind, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "color state requires a hex value (e.g. `base \"#7c3aed\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        states.push(ColorStateAst { kind, value, dark });
        last_end = line.end;
        i += 1;
    }
    Ok((states, i, last_end))
}

/// Parse the `<value> [dark <value>]` tail of a color line. The values
/// are typically quoted hex literals; we preserve quotes verbatim so the
/// analyzer can validate.
fn parse_color_value_with_dark(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    // `dark ` may appear after the primary value; we honor a top-level
    // (paren-depth-0) match so embedded `dark` inside an unlikely literal
    // stays put. In practice values are short hex strings.
    if let Some(idx) = find_top_level_token(rest, " dark ") {
        let primary = rest[..idx].trim();
        let dark_part = rest[idx + " dark ".len()..].trim();
        if primary.is_empty() {
            return Err(line_error(
                line,
                "color value missing before `dark` modifier",
            ));
        }
        if dark_part.is_empty() {
            return Err(line_error(
                line,
                "`dark` modifier requires a hex value (e.g. `dark \"#09090b\"`)",
            ));
        }
        Ok((
            strip_design_quotes(primary).to_owned(),
            Some(strip_design_quotes(dark_part).to_owned()),
        ))
    } else {
        Ok((strip_design_quotes(rest).to_owned(), None))
    }
}

/// Parse `typography` body: family / scale / weight / tracking sub-groups.
fn parse_design_typography(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(TypographyAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut typo = TypographyAst::default();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "typography sub-groups use one indentation level deeper than the `typography` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "family" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.families = entries
                    .into_iter()
                    .map(|(name, value)| FamilyTokenAst { name, value })
                    .collect();
                i = next;
            }
            "scale" => {
                let (entries, next) =
                    parse_design_scale_block(lines, sub_header_index, entry_indent)?;
                typo.scale = entries;
                i = next;
            }
            "weight" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.weights = entries
                    .into_iter()
                    .map(|(name, value)| WeightTokenAst { name, value })
                    .collect();
                i = next;
            }
            "tracking" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.tracking = entries
                    .into_iter()
                    .map(|(name, value)| TrackingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "typography sub-groups are `family`, `scale`, `weight`, or `tracking` (got `{other}`)"
                    ),
                ));
            }
        }
    }
    Ok((typo, i))
}

/// Parse the body of a flat `<group>` like `space` / `radius` /
/// `breakpoint`, where each child line is `<name> <value>`.
fn parse_design_scale_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ScaleTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ScaleTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `shadow` body: each child is `<name> "<value>"` where the value is
/// a CSS box-shadow string (lowering validates single-layer).
fn parse_design_shadow_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ShadowTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ShadowTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse the body of `motion` (duration + easing sub-groups).
fn parse_design_motion(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(MotionAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut motion = MotionAst::default();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "motion sub-groups use one indentation level deeper than the `motion` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "duration" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.durations = entries
                    .into_iter()
                    .map(|(name, value)| ScaleTokenAst { name, value })
                    .collect();
                i = next;
            }
            "easing" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.easings = entries
                    .into_iter()
                    .map(|(name, value)| EasingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!("motion sub-groups are `duration` or `easing` (got `{other}`)"),
                ));
            }
        }
    }
    Ok((motion, i))
}

/// Parse `z` body: each child line is `<name> <integer>`.
fn parse_design_z_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ZTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ZTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `custom` body: each child line is `<kebab-name> "<hex>" [dark "<hex>"]`.
/// Flat sub-grammar — no state sub-blocks. Lowering enforces hex validity
/// + reserved-name + collision rules. See
/// `docs/proposals/design-tokens-custom.md` §2.
fn parse_design_custom_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<CustomTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<CustomTokenAst> = Vec::new();
    let mut i = header_index + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "custom entries use one indentation level deeper than the `custom` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "custom entry requires a hex value (e.g. `chat-bubble \"#dcf8c6\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        entries.push(CustomTokenAst {
            name,
            value,
            dark,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((entries, i))
}

/// Generic `<name> <value>` block parser used by space/radius/shadow/
/// breakpoint/z plus motion.duration/easing plus typography.family/
/// weight/tracking. Values are captured verbatim with surrounding quotes
/// stripped if present; the analyzer applies type-specific validation.
fn parse_design_named_value_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<(String, String)>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "design value entries use one indentation level deeper than the group header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, rest) = split_design_name(line, trimmed)?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(line_error(
                line,
                "design value entry requires `<name> <value>`",
            ));
        }
        entries.push((name, strip_design_quotes(rest).to_owned()));
        i += 1;
    }
    Ok((entries, i))
}

/// Parse `typography.scale` body: `<name> size <size>, line_height <lh>`.
fn parse_design_scale_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<TextScaleTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<TextScaleTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "typography.scale entries use one indentation level deeper than the `scale` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        // Expected: `size <size>, line_height <lh>`.
        let after = after.strip_prefix("size ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry must be `<name> size <size>, line_height <lh>`",
            )
        })?;
        let comma_idx = after.find(',').ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry requires `, line_height <value>` after the size",
            )
        })?;
        let size = strip_design_quotes(after[..comma_idx].trim()).to_owned();
        let after_comma = after[comma_idx + 1..].trim();
        let lh = after_comma.strip_prefix("line_height ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry expects `line_height <value>` after the comma",
            )
        })?;
        let line_height = strip_design_quotes(lh.trim()).to_owned();
        entries.push(TextScaleTokenAst {
            name,
            size,
            line_height,
        });
        i += 1;
    }
    Ok((entries, i))
}

/// Split `<name> <rest>` where `<name>` may be a bare ident or a quoted
/// string (needed for digit-leading names like `"2xl"`). The split
/// happens at the first whitespace following the (possibly-quoted) name.
fn split_design_name<'a>(
    line: &SourceLine<'_>,
    trimmed: &'a str,
) -> Result<(String, &'a str), ParseError> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err(line_error(line, "expected a token name"));
    }
    let (name_text, rest) = if bytes[0] == b'"' {
        // Scan to matching closing quote.
        let mut i = 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return Err(line_error(line, "unterminated quoted token name"));
        }
        let name = &trimmed[1..i];
        let after = trimmed[i + 1..].trim_start();
        (name.to_owned(), after)
    } else {
        let end = bytes
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let name = trimmed[..end].to_owned();
        let after = trimmed[end..].trim_start();
        (name, after)
    };
    if name_text.is_empty() {
        return Err(line_error(line, "token name cannot be empty"));
    }
    Ok((name_text, rest))
}

/// Strip surrounding `"..."` quotes if present, returning the inner slice.
fn strip_design_quotes(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}
