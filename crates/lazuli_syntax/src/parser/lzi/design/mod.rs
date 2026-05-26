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

// =============================================================================
// L0 #2 — `design.lzi` parser tests.
// =============================================================================
#[cfg(test)]
mod design_parser_tests {
    #[test]
    fn design_parses_minimal_color_only() {
        let source = r##"
design example
  color
    success "#16a34a"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "example");
        assert!(ast.extends.is_none());
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "success");
        assert_eq!(ast.colors[0].states.len(), 1);
        assert_eq!(ast.colors[0].states[0].kind, "base");
        assert_eq!(ast.colors[0].states[0].value, "#16a34a");
        assert!(ast.colors[0].states[0].dark.is_none());
    }

    #[test]
    fn design_parses_full_eight_group_fixture() {
        // Mirror of `docs/proposals/design-tokens.md` §8.1 (example brand
        // example). Exercises all eight closed groups + dark suffix + the
        // digit-leading `"2xl"` quoted name.
        let source = r##"
design example
  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      foreground "#ffffff"
    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
    foreground
      base "#09090b" dark "#fafafa"
      muted "#71717a" dark "#a1a1aa"
    success "#16a34a"
    warning "#ea580c"
    danger  "#dc2626"

  typography
    family
      sans "Inter, system-ui, sans-serif"
      mono "JetBrains Mono, monospace"
    scale
      sm    size 0.875rem, line_height 1.25rem
      base  size 1rem,     line_height 1.5rem
      lg    size 1.125rem, line_height 1.75rem
      xl    size 1.25rem,  line_height 1.75rem
      "2xl" size 1.5rem,   line_height 2rem
    weight
      regular 400
      medium 500
      semibold 600
      bold 700
    tracking
      tight -0.025em
      normal 0
      wide 0.025em

  space
    "1" 0.25rem
    "2" 0.5rem
    "3" 0.75rem
    "4" 1rem

  radius
    sm 0.125rem
    base 0.25rem
    md 0.375rem

  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
    md "0 4px 6px -1px rgb(0 0 0 / 0.1)"

  motion
    duration
      fast 150ms
      base 200ms
    easing
      out "cubic-bezier(0, 0, 0.2, 1)"

  breakpoint
    sm 640px
    md 768px
    lg 1024px

  z
    dropdown 1000
    modal 1300
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "example");
        // Color group: primary + background + foreground + 3 flat semantic.
        assert_eq!(ast.colors.len(), 6);
        // Color sub-block lift.
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.colors[0].states.len(), 3);
        // Dark suffix lift.
        let bg = &ast.colors[1];
        assert_eq!(bg.name, "background");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
        // Typography sub-groups.
        assert_eq!(ast.typography.families.len(), 2);
        assert_eq!(ast.typography.scale.len(), 5);
        assert_eq!(ast.typography.scale[4].name, "2xl");
        assert_eq!(ast.typography.scale[4].size, "1.5rem");
        assert_eq!(ast.typography.weights.len(), 4);
        assert_eq!(ast.typography.weights[0].value, "400");
        assert_eq!(ast.typography.tracking.len(), 3);
        // Scale groups.
        assert_eq!(ast.spaces.len(), 4);
        assert_eq!(ast.radii.len(), 3);
        assert_eq!(ast.breakpoints.len(), 3);
        // Shadow + motion + z.
        assert_eq!(ast.shadows.len(), 3);
        assert_eq!(ast.motion.durations.len(), 2);
        assert_eq!(ast.motion.easings.len(), 1);
        assert_eq!(ast.motion.easings[0].value, "cubic-bezier(0, 0, 0.2, 1)");
        assert_eq!(ast.z_indices.len(), 2);
    }

    #[test]
    fn design_color_sub_block_with_four_states() {
        let source = r##"
design example
  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      active "#5b21b6"
      foreground "#ffffff"
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.colors[0].states.len(), 4);
        let kinds: Vec<&str> = ast.colors[0]
            .states
            .iter()
            .map(|s| s.kind.as_str())
            .collect();
        assert_eq!(kinds, vec!["base", "hover", "active", "foreground"]);
        assert_eq!(ast.colors[0].states[0].value, "#7c3aed");
        assert_eq!(ast.colors[0].states[3].value, "#ffffff");
    }

    #[test]
    fn design_color_captures_dark_suffix() {
        let source = r##"
design example
  color
    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
"##;
        let ast = super::parse_design_document(source).unwrap();
        let bg = &ast.colors[0];
        assert_eq!(bg.name, "background");
        assert_eq!(bg.states[0].value, "#ffffff");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
        assert_eq!(bg.states[1].value, "#f4f4f5");
        assert_eq!(bg.states[1].dark.as_deref(), Some("#18181b"));
    }

    #[test]
    fn design_typography_scale_pairs_size_and_line_height() {
        let source = r##"
design example
  typography
    scale
      base size 1rem, line_height 1.5rem
      lg   size 1.125rem, line_height 1.75rem
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.typography.scale.len(), 2);
        let base = &ast.typography.scale[0];
        assert_eq!(base.name, "base");
        assert_eq!(base.size, "1rem");
        assert_eq!(base.line_height, "1.5rem");
        let lg = &ast.typography.scale[1];
        assert_eq!(lg.name, "lg");
        assert_eq!(lg.size, "1.125rem");
        assert_eq!(lg.line_height, "1.75rem");
    }

    #[test]
    fn design_extends_keyword_parses() {
        let source = r##"
design alpha
  extends base
  color
    primary
      base "#10b981"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "alpha");
        assert_eq!(ast.extends.as_deref(), Some("base"));
    }

    #[test]
    fn design_digit_prefix_names_require_quotes() {
        let source = r##"
design example
  space
    "1" 0.25rem
    "2" 0.5rem
  breakpoint
    "2xl" 1536px
    "3xl" 1920px
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.spaces[0].name, "1");
        assert_eq!(ast.spaces[0].value, "0.25rem");
        assert_eq!(ast.spaces[1].name, "2");
        assert_eq!(ast.breakpoints[0].name, "2xl");
        assert_eq!(ast.breakpoints[0].value, "1536px");
        assert_eq!(ast.breakpoints[1].name, "3xl");
    }

    #[test]
    fn design_shadow_quoted_strings_preserved_intact() {
        let source = r##"
design example
  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.shadows.len(), 2);
        assert_eq!(ast.shadows[0].name, "sm");
        assert_eq!(ast.shadows[0].value, "0 1px 2px 0 rgb(0 0 0 / 0.05)");
        assert_eq!(ast.shadows[1].value, "0 1px 3px 0 rgb(0 0 0 / 0.1)");
    }

    #[test]
    fn design_z_values_parsed_as_strings() {
        let source = r##"
design example
  z
    docked 10
    modal 1300
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.z_indices.len(), 2);
        assert_eq!(ast.z_indices[0].name, "docked");
        assert_eq!(ast.z_indices[0].value, "10");
        assert_eq!(ast.z_indices[1].value, "1300");
    }

    #[test]
    fn design_tracking_accepts_negative_value() {
        let source = r##"
design example
  typography
    tracking
      tight -0.025em
      normal 0
      wide 0.025em
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.typography.tracking.len(), 3);
        assert_eq!(ast.typography.tracking[0].name, "tight");
        assert_eq!(ast.typography.tracking[0].value, "-0.025em");
        assert_eq!(ast.typography.tracking[1].value, "0");
        assert_eq!(ast.typography.tracking[2].value, "0.025em");
    }

    #[test]
    fn design_empty_motion_block_skips_cleanly() {
        // `motion` header with no children should leave the AST defaults intact.
        let source = r##"
design example
  color
    success "#16a34a"
  motion
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert!(ast.motion.durations.is_empty());
        assert!(ast.motion.easings.is_empty());
        // Sibling group still parsed.
        assert_eq!(ast.colors.len(), 1);
    }

    #[test]
    fn design_rejects_unknown_group_keyword() {
        let source = r##"
design example
  bogus
    foo bar
"##;
        let err = super::parse_design_document(source).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("design children"),
            "expected unknown-group diagnostic, got: {msg}"
        );
    }

    // ── `custom` 9th meta-group ──────────────────────────────────────────────
    // Per `docs/proposals/design-tokens-custom.md` §2.

    #[test]
    fn design_custom_group_parses_flat_entries() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 3);
        assert_eq!(ast.custom[0].name, "chat-bubble-mine");
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert!(ast.custom[0].dark.is_none());
        assert_eq!(ast.custom[1].name, "chat-bubble-other");
        assert_eq!(ast.custom[2].name, "map-marker-active");
    }

    #[test]
    fn design_custom_entry_captures_dark_suffix() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff" dark "#202c33"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 2);
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert_eq!(ast.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(ast.custom[1].dark.as_deref(), Some("#202c33"));
    }

    #[test]
    fn design_custom_group_coexists_with_color_group() {
        let source = r##"
design hostpoint
  color
    primary "#28bbdd"
  custom
    chat-bubble "#dcf8c6"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.custom.len(), 1);
        assert_eq!(ast.custom[0].name, "chat-bubble");
    }

    #[test]
    fn design_custom_entry_requires_value() {
        let source = r##"
design hostpoint
  custom
    chat-bubble
"##;
        let err = super::parse_design_document(source).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("custom entry requires"), "got: {msg}");
    }

    #[test]
    fn design_custom_empty_block_skips_cleanly() {
        // `custom` header with no children should not crash; the field
        // remains an empty Vec.
        let source = r##"
design hostpoint
  custom
  color
    primary "#28bbdd"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
    }

    #[test]
    fn design_without_custom_group_still_parses() {
        // Regression: pre-Z2 `design.lzi` blocks must keep parsing.
        let source = r##"
design legacy
  color
    primary "#28bbdd"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
    }
}
