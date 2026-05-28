//! Aggregate + Invariant IR — DDD consistency-boundary primitives.
//!
//! `aggregate <Name>` declares a Domain-Driven Design consistency boundary:
//! one `root` resource, zero or more `contains` members, and zero or more
//! invariants whose predicates span the cluster. The intent is that a
//! single command modifies one aggregate at a time; cross-aggregate
//! reactions ride the `event` bus. Lazuli's job is to lock that contract
//! in the IR so doctor can refuse violations cold (cross-aggregate writes,
//! invariant predicates that touch unresolved fields).
//!
//! ## Why `Invariant` lives here too
//!
//! [`Invariant`] is shared between `Resource.invariants` (single-resource
//! checks) and [`Aggregate::invariants`] (cluster-spanning checks). Same
//! shape, same predicate lowering path — keeping it in one file means
//! doctor and codegen don't have to reason about two near-identical
//! types. The closed-catalog predicate lives in
//! [`crate::EvalPredicate`], shared with the agent `evals` block.
//!
//! ## No Go escape hatch
//!
//! The `when <expr>` predicate is intentionally closed-catalog. If an
//! author needs an expression the catalog can't express, they rephrase
//! as multiple closed comparisons or move the rule into a `rule` block
//! (which carries its own typed predicate slot). There is no
//! `@fn.handler` fallback for `invariant` — that escape hatch would
//! defeat the point of having a declarative consistency contract.
//!
//! ## Derive note — no `Eq`
//!
//! Both [`Invariant`] and [`Aggregate`] drop `Eq` because their
//! [`crate::EvalPredicate`] payload carries an `Expr` chain that isn't
//! `Eq`. This mirrors the IR's existing `Feature` derive note.
//! Consumers needing equality use `PartialEq`.
//!
//! ## Doctor lattice
//!
//! - `aggregate-root-unknown` — `root <Resource>` must resolve in the
//!   same feature.
//! - `aggregate-contains-unknown` — every `contains` entry must resolve.
//! - `invariant-predicate-invalid` — the `when` predicate must reference
//!   only fields the analyzer can resolve.
//!
//! ## See also
//!
//! - `docs/proposals/cl-c-4-aggregates.md` — CL.C.4 proposal.
//! - [`crate::EvalPredicate`] — shared closed-catalog predicate AST.
//! - [`crate::Resource::invariants`] — single-resource sibling slot.

use serde::{Deserialize, Serialize};

use crate::{EvalPredicate, QualifiedName, SpanRef};

/// CL.C.4 — `Invariant` deliberately drops `Eq` because `EvalPredicate`
/// carries an `Expr` chain that isn't `Eq` (mirrors the IR's existing
/// `Feature` derive note). Consumers needing equality use `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invariant {
    /// Snake-case identifier authored as `invariant <name>`.
    pub name: String,
    /// Closed-catalog predicate text from `when <expr>`. The analyzer
    /// lifts this through the same closed-predicate parser the agent
    /// `evals` block uses, so the shape mirrors `EvalPredicate`.
    pub when: EvalPredicate,
    /// Authored `message "<text>"`. Empty string when no message body
    /// was written (doctor surfaces a follow-up suggestion).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// CL.C.4 — `Aggregate` drops `Eq` to keep parity with `Invariant`
/// (transitively `EvalPredicate`-bearing). Consumers needing equality
/// use `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    /// `aggregate <Name>` — PascalCase declaration name.
    pub name: String,
    /// `root <Resource>` — the consistency-boundary root.
    pub root: QualifiedName,
    /// `contains <Resource>, <Resource>, ...` — closed cluster.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contains: Vec<QualifiedName>,
    /// `invariants` block (zero or more) — predicates spanning the
    /// cluster. Shape mirrors `Resource.invariants` so doctor and
    /// codegen share one predicate-resolution path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<Invariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_invariant() -> Invariant {
        use crate::{EvalContainsRhs, Path};
        Invariant {
            name: "total_non_negative".to_owned(),
            // Contains form has a serializable JSON projection; the
            // `Unparsed(String)` variant can't be tested via JSON
            // round-trip because its serde tag-only form rejects newtype
            // string payloads.
            when: EvalPredicate::Contains {
                lhs: Path::from_segments(["status"]),
                rhs: EvalContainsRhs::Literal("active".to_owned()),
            },
            message: "Order total cannot be negative".to_owned(),
            span_ref: None,
        }
    }

    #[test]
    fn invariant_round_trips_through_json() {
        let inv = sample_invariant();
        let json = serde_json::to_value(&inv).unwrap();
        let back: Invariant = serde_json::from_value(json).unwrap();
        assert_eq!(back, inv);
    }

    #[test]
    fn invariant_omits_empty_message() {
        let mut inv = sample_invariant();
        inv.message = String::new();
        let value = serde_json::to_value(&inv).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("message"));
        assert!(!obj.contains_key("span_ref"));
    }

    #[test]
    fn aggregate_round_trips_with_empty_contains() {
        let agg = Aggregate {
            name: "Order".to_owned(),
            root: QualifiedName {
                feature: None,
                name: "Order".to_owned(),
            },
            contains: vec![],
            invariants: vec![sample_invariant()],
            span_ref: None,
        };
        let json = serde_json::to_value(&agg).unwrap();
        let back: Aggregate = serde_json::from_value(json).unwrap();
        assert_eq!(back, agg);
    }

    #[test]
    fn aggregate_omits_empty_collections() {
        let agg = Aggregate {
            name: "Order".to_owned(),
            root: QualifiedName {
                feature: None,
                name: "Order".to_owned(),
            },
            contains: vec![],
            invariants: vec![],
            span_ref: None,
        };
        let value = serde_json::to_value(&agg).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("contains"));
        assert!(!obj.contains_key("invariants"));
        assert!(!obj.contains_key("span_ref"));
        assert_eq!(value["name"], json!("Order"));
    }
}
