//! B5 framework gap 2 — typed `when <expr>` predicates on webhook `emits` entries.
//!
//! Three closed shapes — equality, set membership, opaque. The opaque variant
//! ([`EmitPredicateKind::Other`]) is the escape hatch: codegen treats it as a
//! runtime-evaluated Go expression so authors can iterate before the typed
//! lift catches up. Doctor still tracks the path the predicate reads
//! ([`EmitPredicate::payload_path`]) for field-resolution diagnostics on the
//! typed shapes.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// B5 framework gap 2 — typed predicate attached to a webhook `emits`
/// entry. Three closed shapes cover the surface today:
///
/// * `field = "literal"` — equality check (most common).
/// * `field in ("a", "b")` — set membership.
/// * raw — opaque expression preserved verbatim for shapes the
///   typed lifter has not been taught yet. Codegen passes raw
///   predicates through as runtime-evaluated Go expressions inside
///   the dispatch table, so authors can still iterate without the
///   typed lift catching up.
///
/// Lowering also captures the **path** the predicate reads (`field`)
/// so the doctor can fail fast when the path does not resolve against
/// the webhook payload contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmitPredicate {
    /// Original `when <expr>` text verbatim (after the `when ` token,
    /// trimmed). Codegen + doctor both consume the structured shape;
    /// this slot is preserved for diagnostics and round-tripping.
    pub raw: String,
    /// Typed predicate kind. `Other(raw)` keeps the surface
    /// permissive while the typed catalog grows.
    pub kind: EmitPredicateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 2 — closed catalog of typed emit predicate shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmitPredicateKind {
    /// `path = "literal"` — equality.
    Equals { path: String, literal: String },
    /// `path in ("a", "b", ...)` — set membership.
    In { path: String, literals: Vec<String> },
    /// Any other predicate shape; codegen treats this as an opaque
    /// runtime expression. Carried verbatim so the dispatch table can
    /// still emit a Go-level comment + a `/* TODO */` placeholder.
    Other { raw: String },
}

impl EmitPredicate {
    /// Returns the payload path the predicate reads, when the typed
    /// catalog recognises one. Used by the doctor diagnostic
    /// `webhook_emit_predicate_field_unresolved_001` to anchor at the
    /// authored path.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::{EmitPredicate, EmitPredicateKind};
    ///
    /// let pred = EmitPredicate {
    ///     kind: EmitPredicateKind::Equals {
    ///         path: "type".into(),
    ///         literal: "active".into(),
    ///     },
    ///     raw: String::new(),
    ///     span_ref: None,
    /// };
    /// assert_eq!(pred.payload_path(), Some("type"));
    /// ```
    pub fn payload_path(&self) -> Option<&str> {
        match &self.kind {
            EmitPredicateKind::Equals { path, .. } => Some(path.as_str()),
            EmitPredicateKind::In { path, .. } => Some(path.as_str()),
            EmitPredicateKind::Other { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn emit_predicate_kind_equals_round_trips() {
        let k = EmitPredicateKind::Equals {
            path: "type".to_owned(),
            literal: "active".to_owned(),
        };
        let v = serde_json::to_value(&k).unwrap();
        assert_eq!(v["kind"], json!("equals"));
        let back: EmitPredicateKind = serde_json::from_value(v).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn emit_predicate_payload_path_returns_path_for_typed_variants() {
        let ep = EmitPredicate {
            raw: "type = \"x\"".to_owned(),
            kind: EmitPredicateKind::Equals {
                path: "type".to_owned(),
                literal: "x".to_owned(),
            },
            span_ref: None,
        };
        assert_eq!(ep.payload_path(), Some("type"));

        let other = EmitPredicate {
            raw: "weird".to_owned(),
            kind: EmitPredicateKind::Other {
                raw: "weird".to_owned(),
            },
            span_ref: None,
        };
        assert!(other.payload_path().is_none());
    }
}
