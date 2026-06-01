//! Spec 0014 — referential-guard grammar tests.
//!
//! Covers the resource-scoped `restrict on_delete references <relation>
//! via <fk> [where <predicate>]` clause: single + multi-clause forms, the
//! `where`-subset variant, error paths, and a serde round-trip (the
//! "round-trips through fmt" gate — the AST captures the clause losslessly).

use lazuli_syntax::{ResourceDecl, parse_feature_skeletons};

fn first_resource(source: &str) -> ResourceDecl {
    parse_feature_skeletons(source)
        .expect("referential-guard authoring should parse")
        .remove(0)
        .resources
        .remove(0)
}

#[test]
fn referential_guard_single_clause_parses() {
    let r = first_resource(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n    restrict on_delete references invoice via billing_type_id\n",
    );
    assert_eq!(r.restrict_on_delete.len(), 1);
    let g = &r.restrict_on_delete[0];
    assert_eq!(g.relation, "invoice");
    assert_eq!(g.fk, "billing_type_id");
    assert_eq!(g.extra_where, None);
}

#[test]
fn referential_guard_multiple_clauses_parse_in_order() {
    let r = first_resource(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n    restrict on_delete references invoice via billing_type_id\n    restrict on_delete references quote via billing_type_id\n",
    );
    assert_eq!(r.restrict_on_delete.len(), 2);
    assert_eq!(r.restrict_on_delete[0].relation, "invoice");
    assert_eq!(r.restrict_on_delete[1].relation, "quote");
}

#[test]
fn referential_guard_where_subset_clause_parses() {
    let r = first_resource(
        "\nfeature ops\n  resource WorkflowStep\n    name: Text required\n    restrict on_delete references activity via step_id where status = 'open'\n",
    );
    assert_eq!(r.restrict_on_delete.len(), 1);
    let g = &r.restrict_on_delete[0];
    assert_eq!(g.relation, "activity");
    assert_eq!(g.fk, "step_id");
    assert_eq!(g.extra_where.as_deref(), Some("status = 'open'"));
}

#[test]
fn referential_guard_round_trips_through_serde() {
    // "Round-trips through fmt": the AST captures the clause losslessly,
    // so serialize → deserialize is identity.
    let r = first_resource(
        "\nfeature ops\n  resource WorkflowStep\n    name: Text required\n    restrict on_delete references activity via step_id where status = 'open'\n",
    );
    let json = serde_json::to_string(&r).expect("serialize");
    let back: ResourceDecl = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(r, back);
    assert_eq!(back.restrict_on_delete[0].extra_where.as_deref(), Some("status = 'open'"));
}

#[test]
fn referential_guard_empty_restrict_is_omitted_from_json() {
    // Additive: a resource with no guard ships no `restrict_on_delete` key.
    let r = first_resource(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n",
    );
    assert!(r.restrict_on_delete.is_empty());
    let json = serde_json::to_string(&r).expect("serialize");
    assert!(
        !json.contains("restrict_on_delete"),
        "empty restrict_on_delete must be omitted: {json}"
    );
}

#[test]
fn referential_guard_requires_via_clause() {
    let err = parse_feature_skeletons(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n    restrict on_delete references invoice\n",
    );
    assert!(err.is_err(), "missing `via` must be a parse error");
}

#[test]
fn referential_guard_bare_restrict_errors_with_guidance() {
    let err = parse_feature_skeletons(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n    restrict\n",
    );
    assert!(err.is_err(), "bare `restrict` must be a parse error");
}

#[test]
fn referential_guard_requires_on_delete_keyword() {
    let err = parse_feature_skeletons(
        "\nfeature billing\n  resource BillingType\n    name: Text required\n    restrict references invoice via billing_type_id\n",
    );
    assert!(err.is_err(), "missing `on_delete` must be a parse error");
}
