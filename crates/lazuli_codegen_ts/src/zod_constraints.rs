//! L0 #3 §10 — Zod schema constraint emission.
//!
//! Pure function from `(BuiltinType, FieldConstraints)` to a Zod
//! chain suffix. Emits the `.min(N)` / `.max(N)` / `.regex(...)` /
//! `.length(N)` / `.gte(A).lte(B)` calls (and the `z.enum([...])`
//! replacement for closed-string `in [...]`). The base `z.string()` /
//! `z.number()` selection lives in the parent emitter; this module
//! only contributes the suffix.
//!
//! Mapping (proposal §10.1):
//!
//! | Constraint | Text base | Numeric base | Notes |
//! |---|---|---|---|
//! | `min N` | `.min(N)` | `.gte(N)` | text=char count, numeric=value |
//! | `max N` | `.max(N)` | `.lte(N)` | ditto |
//! | `length N` | `.length(N)` | — | text only |
//! | `pattern STR` | `.regex(new RegExp("STR"))` | — | RE2 |
//! | `between A and B` | — | `.gte(A).lte(B)` | numerics |
//! | `in [...]` (string) | `z.enum([...])` REPLACES `z.string()` | — | closed |
//! | `in [...]` (numeric) | — | `.refine` (TODO) | numerics |
//!
//! `between` + `min`/`max` is rejected at the analyzer (proposal
//! §10.2); we don't double-check here.
//!
//! Stability: chain segments are emitted in fixed order
//! (min → max → length → between → pattern → in) so generated text
//! is deterministic across runs.

use lazuli_ir::{BuiltinType, FieldConstraints};

/// Decide whether the field is numeric (Integer / Decimal) so the
/// caller can pick the right `z.string()` vs `z.number()` base and
/// the right `.min` vs `.gte` semantic.
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::is_numeric;
/// use lazuli_ir::BuiltinType;
/// assert!(is_numeric(BuiltinType::Integer));
/// assert!(!is_numeric(BuiltinType::Text));
/// ```
pub fn is_numeric(builtin: BuiltinType) -> bool {
    matches!(builtin, BuiltinType::Integer | BuiltinType::Decimal)
}

/// Returns the suffix Zod chain for the given constraints. Empty
/// string when no constraints are declared. Caller is responsible
/// for the base `z.string()` / `z.number()` / `z.bigint()` call and
/// any trailing `.optional()`.
///
/// `is_text_base` is true for Text-shape and similar string-like
/// builtins (`SemanticEmail`, `SemanticUrl`, `SemanticPhone`,
/// `SemanticUuid`, …); the analyzer guarantees the constraint set
/// is type-compatible per §10.1.
///
/// For `in [...]` on a text base, this function returns a chain
/// suffix starting with `.pipe(z.enum([...]))` so callers can
/// preserve their existing `z.string()` prefix without rewriting.
/// (Equivalent to swapping the base; the prefix-preserving variant
/// keeps the public emitter shape simpler.)
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::zod_constraint_chain;
/// use lazuli_ir::FieldConstraints;
///
/// let mut constraints = FieldConstraints::default();
/// constraints.min = Some(3);
/// assert_eq!(zod_constraint_chain(&constraints, true), ".min(3)");
/// assert_eq!(zod_constraint_chain(&constraints, false), ".gte(3)");
/// ```
pub fn zod_constraint_chain(constraints: &FieldConstraints, is_text_base: bool) -> String {
    if constraints.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    // min → .min(N) (text) or .gte(N) (numeric)
    if let Some(n) = constraints.min {
        if is_text_base {
            out.push_str(&format!(".min({})", n));
        } else {
            out.push_str(&format!(".gte({})", n));
        }
    }
    if let Some(n) = constraints.max {
        if is_text_base {
            out.push_str(&format!(".max({})", n));
        } else {
            out.push_str(&format!(".lte({})", n));
        }
    }
    if let Some(n) = constraints.length {
        // `.length(N)` is text only per §10.1.
        out.push_str(&format!(".length({})", n));
    }
    if let Some((a, b)) = constraints.between {
        // §10.1: between → .gte(A).lte(B) on numerics. We don't
        // re-derive `is_text_base` here because the analyzer rejects
        // text + between (between only applies to Integer/Decimal).
        out.push_str(&format!(".gte({}).lte({})", a, b));
    }
    if let Some(pattern) = &constraints.pattern {
        // RE2 syntax — Go's regexp and JS's RegExp both accept the
        // common subset. We escape backslashes for the JS string
        // literal but keep the regex pattern intact otherwise.
        let escaped = escape_for_js_string(pattern);
        out.push_str(&format!(".regex(new RegExp(\"{}\"))", escaped));
    }
    if let Some(values) = &constraints.r#in {
        if is_text_base {
            // Closed string enum — chain `.pipe(z.enum([...]))` so the
            // outer `z.string()` base remains intact. Production
            // emitters that prefer the swap-base form can call
            // `zod_enum_replacement` directly instead.
            out.push_str(&format!(".pipe({})", zod_enum_replacement(values)));
        } else {
            // Numeric in [...] — emit `.refine` since `z.enum` is
            // string-only in Zod.
            let values_list: Vec<String> = values.iter().map(|v| v.trim().to_owned()).collect();
            out.push_str(&format!(
                ".refine((n) => [{}].includes(n), {{ message: \"value must be in [{}]\" }})",
                values_list.join(", "),
                values_list.join(", "),
            ));
        }
    }
    out
}

/// Emit `z.enum(["a", "b", "c"])` for a closed string-list. Caller
/// uses this when it wants to REPLACE the base `z.string()` for the
/// `in [...]` case (proposal §10.1).
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::zod_enum_replacement;
/// let values = vec!["draft".to_owned(), "published".to_owned()];
/// assert_eq!(zod_enum_replacement(&values), "z.enum([\"draft\", \"published\"])");
/// ```
pub fn zod_enum_replacement(values: &[String]) -> String {
    let formatted: Vec<String> = values
        .iter()
        .map(|v| format!("\"{}\"", escape_for_js_string(v)))
        .collect();
    format!("z.enum([{}])", formatted.join(", "))
}

/// Escape a string for use inside a JS double-quoted string literal.
/// Backslashes double-escape and double-quotes get backslash-escaped;
/// nothing else is touched (regex metachars are RE2 native).
fn escape_for_js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c() -> FieldConstraints {
        FieldConstraints::default()
    }

    #[test]
    fn no_constraints_emits_empty_chain() {
        let chain = zod_constraint_chain(&c(), true);
        assert_eq!(chain, "");
    }

    #[test]
    fn min_max_on_text_emits_min_max() {
        let mut k = c();
        k.min = Some(2);
        k.max = Some(80);
        let chain = zod_constraint_chain(&k, true);
        assert_eq!(chain, ".min(2).max(80)");
    }

    #[test]
    fn min_max_on_numeric_emits_gte_lte() {
        let mut k = c();
        k.min = Some(2);
        k.max = Some(80);
        let chain = zod_constraint_chain(&k, false);
        assert_eq!(chain, ".gte(2).lte(80)");
    }

    #[test]
    fn pattern_emits_regex_with_new_regexp() {
        let mut k = c();
        k.pattern = Some("^[a-z]+$".to_owned());
        let chain = zod_constraint_chain(&k, true);
        assert_eq!(chain, ".regex(new RegExp(\"^[a-z]+$\"))");
    }

    #[test]
    fn pattern_escapes_backslash_and_quote() {
        let mut k = c();
        k.pattern = Some(r#"^a\d"b"#.to_owned());
        let chain = zod_constraint_chain(&k, true);
        assert_eq!(chain, r#".regex(new RegExp("^a\\d\"b"))"#);
    }

    #[test]
    fn length_emits_length_only_on_text() {
        let mut k = c();
        k.length = Some(120);
        let chain = zod_constraint_chain(&k, true);
        assert_eq!(chain, ".length(120)");
    }

    #[test]
    fn between_on_numeric_emits_gte_lte() {
        let mut k = c();
        k.between = Some((0, 100));
        let chain = zod_constraint_chain(&k, false);
        assert_eq!(chain, ".gte(0).lte(100)");
    }

    #[test]
    fn in_on_text_pipes_enum_replacement() {
        let mut k = c();
        k.r#in = Some(vec!["admin".to_owned(), "editor".to_owned()]);
        let chain = zod_constraint_chain(&k, true);
        assert_eq!(chain, ".pipe(z.enum([\"admin\", \"editor\"]))");
    }

    #[test]
    fn enum_replacement_helper() {
        let values = vec!["admin".to_owned(), "editor".to_owned(), "viewer".to_owned()];
        assert_eq!(
            zod_enum_replacement(&values),
            "z.enum([\"admin\", \"editor\", \"viewer\"])"
        );
    }

    #[test]
    fn in_on_numeric_emits_refine() {
        let mut k = c();
        k.r#in = Some(vec!["1".to_owned(), "2".to_owned(), "3".to_owned()]);
        let chain = zod_constraint_chain(&k, false);
        assert!(chain.starts_with(".refine"));
        assert!(chain.contains("[1, 2, 3]"));
    }

    #[test]
    fn deterministic_order_across_invocations() {
        let mut k = c();
        k.min = Some(1);
        k.max = Some(10);
        k.pattern = Some("^x".to_owned());
        let a = zod_constraint_chain(&k, true);
        let b = zod_constraint_chain(&k, true);
        assert_eq!(a, b);
        // Stable ordering: min before max before regex.
        let min_pos = a.find(".min(").expect("min");
        let max_pos = a.find(".max(").expect("max");
        let regex_pos = a.find(".regex(").expect("regex");
        assert!(min_pos < max_pos);
        assert!(max_pos < regex_pos);
    }

    #[test]
    fn is_numeric_matches_integer_and_decimal() {
        assert!(is_numeric(BuiltinType::Integer));
        assert!(is_numeric(BuiltinType::Decimal));
        assert!(!is_numeric(BuiltinType::Text));
        assert!(!is_numeric(BuiltinType::Boolean));
    }
}
