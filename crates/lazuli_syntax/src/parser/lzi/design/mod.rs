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

mod color;
mod helpers;
mod motion;
mod scales;
mod typography;

use super::super::common::{SourceLine, is_trivia, line_error, source_lines, strip_inline_comment};
use super::super::error::ParseError;

use crate::ast::{
    ColorTokenAst, CustomTokenAst, DesignDeclAst, MotionAst, ScaleTokenAst, ShadowTokenAst, Span,
    TypographyAst, ZTokenAst,
};

use color::parse_design_color_group;
use motion::parse_design_motion;
use scales::{
    parse_design_custom_group, parse_design_scale_group, parse_design_shadow_group,
    parse_design_z_group,
};
use typography::parse_design_typography;

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
///
/// ## Examples
///
/// ```
/// use lazuli_syntax::parse_design_document;
///
/// let src = "design tokens\n  space\n    sm 4px\n    md 8px\n";
/// let doc = parse_design_document(src).expect("parses");
/// assert_eq!(doc.name, "tokens");
/// assert_eq!(doc.spaces.len(), 2);
/// assert_eq!(doc.spaces[0].name, "sm");
/// ```
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

// =============================================================================
// L0 #2 — `design.lzi` parser tests.
// =============================================================================

// =============================================================================
// L0 #2 — `design.lzi` parser tests (dispatch + integration).
// Sub-concern tests live next to their respective extracted parsers
// (color/typography/motion/scales).
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
}
