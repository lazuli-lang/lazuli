//! `design-custom-*` doctor rules — three lints over the `custom` 9th
//! meta-group of `design.lzi`. See
//! `docs/proposals/design-tokens-custom.md` §4 for the rule catalog.
//!
//! All three rules operate over the lowered `lazuli_ir::Design` value (the
//! analyzer has already enforced hex shape via
//! `AnalyzeError::DesignColorHexInvalid`; doctor adds the proposal-pending
//! collision + reserved-name policy that lives outside the closed-state
//! catalog).
//!
//! Rules:
//! - `DESIGN-CUSTOM-DUPLICATE`     — name collides with an existing `color` token.
//! - `DESIGN-CUSTOM-INVALID-VALUE` — value isn't a valid hex literal (defensive
//!   re-check beside the analyzer; we keep the doctor surface self-contained
//!   so it can also be wired against ad-hoc fixtures that bypass the
//!   analyzer).
//! - `DESIGN-CUSTOM-RESERVED-NAME` — name collides with the Shadcn-semantic
//!   vocabulary catalogue (closed list of 13 names, §4 of the proposal).

use lazuli_ir::Design;

// ── reserved-name catalog (closed per proposal §4) ────────────────────────────

/// Reserved Shadcn-semantic token names. Custom tokens may NOT use these
/// names — they belong in the `color` group (with state sub-blocks if
/// stateful). New names land here only when upstream Shadcn ships a new
/// semantic token.
pub const RESERVED_NAMES: &[&str] = &[
    "primary",
    "secondary",
    "accent",
    "muted",
    "destructive",
    "card",
    "popover",
    "background",
    "foreground",
    "border",
    "input",
    "ring",
    "brand",
];

// ── DESIGN-CUSTOM-DUPLICATE ──────────────────────────────────────────────────

/// `custom <name>` collides with an existing `color <name>` token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateFinding {
    pub name: String,
}

impl DuplicateFinding {
    pub const CODE: &'static str = "design-custom-duplicate";

    pub fn message(&self) -> String {
        format!(
            "custom token `{}` collides with an existing `color` group token. \
             Rename the custom token OR move it into the `color` group (with \
             state sub-blocks if stateful). See \
             `docs/proposals/design-tokens-custom.md` §4.",
            self.name,
        )
    }
}

pub fn check_duplicate(design: &Design) -> Vec<DuplicateFinding> {
    let color_names: std::collections::HashSet<&str> =
        design.colors.iter().map(|c| c.name.as_str()).collect();
    let mut out: Vec<DuplicateFinding> = design
        .custom
        .iter()
        .filter(|tok| color_names.contains(tok.name.as_str()))
        .map(|tok| DuplicateFinding {
            name: tok.name.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

// ── DESIGN-CUSTOM-INVALID-VALUE ───────────────────────────────────────────────

/// `custom <name> <value>` carries a value that isn't a valid hex literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValueFinding {
    pub name: String,
    pub value: String,
    /// `false` = base value is invalid; `true` = the `dark` overlay is.
    pub is_dark: bool,
}

impl InvalidValueFinding {
    pub const CODE: &'static str = "design-custom-invalid-value";

    pub fn message(&self) -> String {
        let slot = if self.is_dark { "dark" } else { "base" };
        format!(
            "custom token `{}` {} value `{}` is not a valid hex literal \
             (expected `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`). v0 \
             accepts hex only; non-color custom values are deferred. See \
             `docs/proposals/design-tokens-custom.md` §3.",
            self.name, slot, self.value,
        )
    }
}

pub fn check_invalid_value(design: &Design) -> Vec<InvalidValueFinding> {
    let mut out = Vec::new();
    for tok in &design.custom {
        if !is_valid_hex(&tok.base) {
            out.push(InvalidValueFinding {
                name: tok.name.clone(),
                value: tok.base.clone(),
                is_dark: false,
            });
        }
        if let Some(dark) = &tok.dark {
            if !is_valid_hex(dark) {
                out.push(InvalidValueFinding {
                    name: tok.name.clone(),
                    value: dark.clone(),
                    is_dark: true,
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.is_dark.cmp(&b.is_dark)));
    out
}

// ── DESIGN-CUSTOM-RESERVED-NAME ───────────────────────────────────────────────

/// `custom <name>` matches a Shadcn-semantic reserved name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedNameFinding {
    pub name: String,
}

impl ReservedNameFinding {
    pub const CODE: &'static str = "design-custom-reserved-name";

    pub fn message(&self) -> String {
        format!(
            "custom token `{}` collides with the Shadcn-semantic reserved \
             vocabulary. Use `color <name>` group with states for stateful \
             semantic tokens; `custom` is for product-domain. See \
             `docs/proposals/design-tokens-custom.md` §4.",
            self.name,
        )
    }
}

pub fn check_reserved_name(design: &Design) -> Vec<ReservedNameFinding> {
    let mut out: Vec<ReservedNameFinding> = design
        .custom
        .iter()
        .filter(|tok| RESERVED_NAMES.iter().any(|r| *r == tok.name.as_str()))
        .map(|tok| ReservedNameFinding {
            name: tok.name.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Match `^#[0-9a-fA-F]{3,8}$` (3/4/6/8 hex digits). Mirrors the analyzer's
/// `is_valid_design_hex`. Kept in-module so doctor is self-contained.
fn is_valid_hex(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('#') {
        return false;
    }
    let digits = &trimmed[1..];
    let len = digits.len();
    if !matches!(len, 3 | 4 | 6 | 8) {
        return false;
    }
    digits.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{ColorState, ColorStateKind, ColorToken, CustomToken, Design, Motion, Typography};

    fn design_with(colors: Vec<ColorToken>, custom: Vec<CustomToken>) -> Design {
        Design {
            name: "test".to_owned(),
            extends: None,
            colors,
            typography: Typography::default(),
            spaces: vec![],
            radii: vec![],
            shadows: vec![],
            motion: Motion::default(),
            breakpoints: vec![],
            z_indices: vec![],
            custom,
            span_ref: None,
        }
    }

    fn color(name: &str) -> ColorToken {
        ColorToken {
            name: name.to_owned(),
            states: vec![ColorState {
                kind: ColorStateKind::Base,
                value: "#000".to_owned(),
                dark: None,
            }],
            span_ref: None,
        }
    }

    fn custom(name: &str, base: &str, dark: Option<&str>) -> CustomToken {
        CustomToken {
            name: name.to_owned(),
            base: base.to_owned(),
            dark: dark.map(String::from),
            span_ref: None,
        }
    }

    #[test]
    fn duplicate_fires_when_custom_collides_with_color() {
        let d = design_with(
            vec![color("brand-blue")],
            vec![custom("brand-blue", "#28bbdd", None)],
        );
        let f = check_duplicate(&d);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "brand-blue");
    }

    #[test]
    fn duplicate_silent_when_names_distinct() {
        let d = design_with(
            vec![color("primary")],
            vec![custom("chat-bubble", "#dcf8c6", None)],
        );
        assert!(check_duplicate(&d).is_empty());
    }

    #[test]
    fn invalid_value_flags_non_hex_base() {
        let d = design_with(
            vec![],
            vec![custom("oops", "not-a-color", None)],
        );
        let f = check_invalid_value(&d);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "oops");
        assert!(!f[0].is_dark);
    }

    #[test]
    fn invalid_value_flags_bad_dark_overlay() {
        let d = design_with(
            vec![],
            vec![custom("chat-bubble", "#dcf8c6", Some("rgb(255,0,0)"))],
        );
        let f = check_invalid_value(&d);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "chat-bubble");
        assert!(f[0].is_dark);
    }

    #[test]
    fn invalid_value_silent_for_valid_hex() {
        let d = design_with(
            vec![],
            vec![
                custom("a", "#fff", None),
                custom("b", "#ffff", None),
                custom("c", "#ffffff", None),
                custom("d", "#ffffffff", Some("#000000aa")),
            ],
        );
        assert!(check_invalid_value(&d).is_empty());
    }

    #[test]
    fn reserved_name_fires_on_each_shadcn_semantic() {
        for name in RESERVED_NAMES {
            let d = design_with(vec![], vec![custom(name, "#fff", None)]);
            let f = check_reserved_name(&d);
            assert_eq!(f.len(), 1, "expected reserved finding for `{name}`");
            assert_eq!(f[0].name, *name);
        }
    }

    #[test]
    fn reserved_name_silent_for_product_domain() {
        let d = design_with(
            vec![],
            vec![
                custom("chat-bubble-mine", "#dcf8c6", None),
                custom("map-marker-active", "#ff5722", None),
                custom("property-status-vacant", "#16a34a", None),
            ],
        );
        assert!(check_reserved_name(&d).is_empty());
    }

    #[test]
    fn all_three_rules_can_coexist_on_one_design() {
        // `primary` reserved + duplicates the color group + a separate bad-hex
        // token. Three distinct findings.
        let d = design_with(
            vec![color("primary")],
            vec![
                custom("primary", "#fff", None),
                custom("oops", "not-hex", None),
            ],
        );
        assert_eq!(check_duplicate(&d).len(), 1);
        assert_eq!(check_reserved_name(&d).len(), 1);
        assert_eq!(check_invalid_value(&d).len(), 1);
    }
}
