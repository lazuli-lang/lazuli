//! L0 #3 §10 — Go validator tag emission for inline field constraints.
//!
//! Pure function from `(FieldConstraints, bool /*required*/)` to a
//! `validate:"..."` tag body (the `validate:` prefix and surrounding
//! quotes are caller's responsibility). The tag uses the
//! `go-playground/validator` keyword convention so the runtime
//! `validate.New()` instance can apply the rules without extra glue.
//!
//! Mapping (proposal §10.1):
//!
//! | Constraint | Tag fragment | Notes |
//! |---|---|---|
//! | `min N` | `min=N` | text length OR numeric value |
//! | `max N` | `max=N` | ditto |
//! | `pattern STR` | `regexp=STR` | RE2 (Go's `regexp` is RE2 native) |
//! | `between A and B` | `min=A,max=B` | numerics only |
//! | `length N` | `len=N` | text exact length |
//! | `in [...]` | `oneof=a b c` | space-separated per validator convention |
//!
//! Order is deterministic: `required` (when set) → `min` → `max` →
//! `len` → `between (min,max)` → `regexp` → `oneof`. Multiple entries
//! join with commas (no leading/trailing comma).
//!
//! Caller wraps the result in backtick-delimited tag text alongside
//! `db:"..."` / `json:"..."`.

use lazuli_ir::FieldConstraints;

/// Build the body of a `validate:"…"` struct tag for the given
/// constraints + required flag. Returns the empty string when no
/// constraint is set AND `required` is false; the caller can then
/// elide the entire `validate:` slot.
pub fn validator_tag_body(constraints: &FieldConstraints, required: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if required {
        parts.push("required".to_owned());
    }
    if let Some(n) = constraints.min {
        parts.push(format!("min={}", n));
    }
    if let Some(n) = constraints.max {
        parts.push(format!("max={}", n));
    }
    if let Some(n) = constraints.length {
        parts.push(format!("len={}", n));
    }
    if let Some((a, b)) = constraints.between {
        // §10.1 — `between` projects to a min/max pair on the Go
        // validator side. The analyzer guarantees `between` doesn't
        // coexist with `min`/`max`, so this stays deterministic.
        parts.push(format!("min={}", a));
        parts.push(format!("max={}", b));
    }
    if let Some(pattern) = &constraints.pattern {
        // go-playground/validator's `regexp` validator compiles the
        // pattern with `regexp.MustCompile`; RE2 syntax matches what
        // the Lazuli grammar accepts in `pattern STRING`.
        parts.push(format!("regexp={}", pattern));
    }
    if let Some(values) = &constraints.r#in {
        // `oneof=a b c` — values are space-separated. If a value
        // itself contains a space (which Lazuli's `in [...]` allows
        // in principle), we don't escape — the validator
        // convention here is space-only delimiting. Authors needing
        // space-bearing literals should switch to enum declarations
        // (per proposal §10.2 hint).
        let joined = values
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        parts.push(format!("oneof={}", joined));
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c() -> FieldConstraints {
        FieldConstraints::default()
    }

    #[test]
    fn empty_constraints_without_required_yields_empty_body() {
        assert_eq!(validator_tag_body(&c(), false), "");
    }

    #[test]
    fn empty_constraints_with_required_yields_required() {
        assert_eq!(validator_tag_body(&c(), true), "required");
    }

    #[test]
    fn min_max_emits_min_and_max() {
        let mut k = c();
        k.min = Some(2);
        k.max = Some(80);
        assert_eq!(validator_tag_body(&k, false), "min=2,max=80");
    }

    #[test]
    fn pattern_emits_regexp_clause() {
        let mut k = c();
        k.pattern = Some("^[a-z]+$".to_owned());
        assert_eq!(validator_tag_body(&k, false), "regexp=^[a-z]+$");
    }

    #[test]
    fn length_emits_len_clause() {
        let mut k = c();
        k.length = Some(120);
        assert_eq!(validator_tag_body(&k, false), "len=120");
    }

    #[test]
    fn between_projects_to_min_and_max() {
        let mut k = c();
        k.between = Some((0, 100));
        assert_eq!(validator_tag_body(&k, false), "min=0,max=100");
    }

    #[test]
    fn in_emits_oneof_space_separated() {
        let mut k = c();
        k.r#in = Some(vec![
            "admin".to_owned(),
            "editor".to_owned(),
            "viewer".to_owned(),
        ]);
        assert_eq!(validator_tag_body(&k, false), "oneof=admin editor viewer");
    }

    #[test]
    fn required_stacks_with_min_max() {
        let mut k = c();
        k.min = Some(2);
        k.max = Some(80);
        assert_eq!(validator_tag_body(&k, true), "required,min=2,max=80");
    }

    #[test]
    fn deterministic_emission_across_runs() {
        let mut k = c();
        k.min = Some(1);
        k.max = Some(10);
        k.pattern = Some("^x".to_owned());
        let a = validator_tag_body(&k, true);
        let b = validator_tag_body(&k, true);
        assert_eq!(a, b);
        // Order: required → min → max → regexp.
        let required_pos = a.find("required").expect("required");
        let min_pos = a.find("min=").expect("min");
        let max_pos = a.find("max=").expect("max");
        let regex_pos = a.find("regexp=").expect("regexp");
        assert!(required_pos < min_pos);
        assert!(min_pos < max_pos);
        assert!(max_pos < regex_pos);
    }
}
