/// Rich hover detail strings for every token in [`DESIGN_KEYWORDS`].
/// Returns `None` for keywords without a curated description.
///
/// Consumed by `handlers::hover` (for `*.design.lzi` documents) and by
/// `completion_items::design_keyword_completion_items` so authors get
/// the same explanation in tooltips and completion details.
pub(crate) fn design_keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "design" => Some(
            "Declares the project-root design token catalog. See `docs/proposals/design-tokens.md`.",
        ),
        "extends" => Some(
            "Declares a base design catalog that this design overrides. See `docs/proposals/design-tokens.md`.",
        ),
        "color" => Some(
            "Closed design token group for brand, semantic, and surface colors. See `docs/proposals/design-tokens.md`.",
        ),
        "typography" => Some(
            "Closed design token group for type families, scales, weights, and tracking. See `docs/proposals/design-tokens.md`.",
        ),
        "space" => Some(
            "Closed design token group for the spacing scale. See `docs/proposals/design-tokens.md`.",
        ),
        "radius" => Some(
            "Closed design token group for border radius values. See `docs/proposals/design-tokens.md`.",
        ),
        "shadow" => Some(
            "Closed design token group for CSS box-shadow elevation values. See `docs/proposals/design-tokens.md`.",
        ),
        "motion" => Some(
            "Closed design token group for transition and animation primitives. See `docs/proposals/design-tokens.md`.",
        ),
        "breakpoint" => Some(
            "Closed design token group for responsive viewport cutoffs. See `docs/proposals/design-tokens.md`.",
        ),
        "z" => Some(
            "Closed design token group for stacking order values. See `docs/proposals/design-tokens.md`.",
        ),
        "family" => Some(
            "Typography sub-group for named font stacks. See `docs/proposals/design-tokens.md`.",
        ),
        "scale" => Some(
            "Typography sub-group for named text sizes and line heights. See `docs/proposals/design-tokens.md`.",
        ),
        "weight" => Some(
            "Typography sub-group for named font weights. See `docs/proposals/design-tokens.md`.",
        ),
        "tracking" => Some(
            "Typography sub-group for named letter-spacing values. See `docs/proposals/design-tokens.md`.",
        ),
        "duration" => Some(
            "Motion sub-group for named transition durations. See `docs/proposals/design-tokens.md`.",
        ),
        "easing" => {
            Some("Motion sub-group for named easing curves. See `docs/proposals/design-tokens.md`.")
        }
        "size" => Some(
            "Typography scale field for a text token's font size. See `docs/proposals/design-tokens.md`.",
        ),
        "line_height" => Some(
            "Typography scale field for a text token's line height. See `docs/proposals/design-tokens.md`.",
        ),
        "base" => Some(
            "Required default color state; also commonly used as a token name. See `docs/proposals/design-tokens.md`.",
        ),
        "hover" => Some(
            "Optional color state for mouse hover or touch press start. See `docs/proposals/design-tokens.md`.",
        ),
        "active" => Some(
            "Optional color state for mouse down or touch press end. See `docs/proposals/design-tokens.md`.",
        ),
        "foreground" => Some(
            "Optional text/icon color when the token is used as a background. See `docs/proposals/design-tokens.md`.",
        ),
        "dark" => Some(
            "Optional dark-theme suffix for a color value. See `docs/proposals/design-tokens.md`.",
        ),
        _ => None,
    }
}
