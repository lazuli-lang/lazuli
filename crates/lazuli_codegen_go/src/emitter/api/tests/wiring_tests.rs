//! Wiring tests — feature-empty fast path, PG.C.2 gate annotations,
//! cross-run determinism.

use super::super::*;
use super::{base_feature, emit, module_with_features, simple_api, simple_record, simple_resource};
use lazuli_ir::{HttpMethod, QualifiedName, TypeRef};

#[test]
fn empty_feature_returns_none() {
    let feature = base_feature("customer");
    assert!(emit(&feature).is_none());
}

#[test]
fn gated_api_emits_real_prelude_field_and_billing_imports() {
    // PG.C.2 — gated APIs lift the wave-4 comment annotation into
    // a real `Prelude: []billing.GateRef{...}` field that
    // `Api.Invoke` consults via `lazuli.RunPrelude`. Billing and
    // <module>/plan imports appear when any api carries gates.
    let mut feature = base_feature("billing");
    feature.records.push(simple_record("Invoice"));
    feature.apis.push(simple_api(
        "issue_invoice",
        HttpMethod::Post,
        "/api/invoices",
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Invoice".to_owned(),
        }),
    ));

    let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
        std::collections::BTreeMap::new();
    gates.insert(
        "billing/api:issue_invoice".to_owned(),
        vec![
            lazuli_ir::Gate::Behind {
                feature: "issue_invoice".to_owned(),
            },
            lazuli_ir::Gate::Quota {
                limit: "invoices_per_month".to_owned(),
            },
        ],
    );
    let module = module_with_features(vec![feature]);
    let cross_index = CrossFeatureIndex::build(&module);
    let emit_ctx = EmitContext::for_feature(None, "billing-app", "billing", "billing/api.gen.go")
        .with_gates(Some(&gates));
    let out = emit_api_file(
        "examples/billing.lzi",
        &module.features[0],
        "billing-app",
        &cross_index,
        &emit_ctx,
    )
    .expect("must emit");

    assert!(
        out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
        "billing import missing:\n{out}"
    );
    assert!(
        out.contains("\"billing-app/plan\""),
        "plan import missing:\n{out}"
    );
    assert!(
        out.contains("Prelude: []billing.GateRef{"),
        "Prelude field missing:\n{out}"
    );
    assert!(
        out.contains("{Kind: billing.GateBehind, Name: \"issue_invoice\"},"),
        "behind-gate row missing:\n{out}"
    );
    assert!(
        out.contains("{Kind: billing.GateQuota, Name: \"invoices_per_month\"},"),
        "quota-gate row missing:\n{out}"
    );
}

#[test]
fn ungated_api_emits_no_prelude_or_billing_import() {
    // PG.C.2 backward-compat — APIs without gates emit
    // byte-equivalent wave-3 output (no Prelude field, no
    // billing import).
    let mut feature = base_feature("customer");
    feature.records.push(simple_record("Customer"));
    feature.apis.push(simple_api(
        "list_customers",
        HttpMethod::Get,
        "/api/customers",
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        }),
    ));
    let out = emit(&feature).expect("must emit");
    assert!(
        !out.contains("Prelude:"),
        "no Prelude when no gates:\n{out}"
    );
    assert!(
        !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
        "no billing import when no gates:\n{out}"
    );
}

#[test]
fn deterministic_across_runs_and_sorts_by_name() {
    let mut feature = base_feature("customer");
    feature.resources.push(simple_resource("Customer"));
    feature.apis.push(simple_api(
        "zebra",
        HttpMethod::Delete,
        "/api/zebra",
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        }),
    ));
    feature.apis.push(simple_api(
        "alpha",
        HttpMethod::Get,
        "/api/alpha",
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        }),
    ));

    let a = emit(&feature).expect("must emit");
    let b = emit(&feature).expect("must emit");
    assert_eq!(a, b);

    let alpha_pos = a.find("Api: customer.alpha").expect("alpha banner");
    let zebra_pos = a.find("Api: customer.zebra").expect("zebra banner");
    assert!(alpha_pos < zebra_pos);
}
