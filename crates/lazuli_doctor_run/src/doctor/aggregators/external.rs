//! External contract aggregator — emits the cross-contract
//! diagnostics that validate `contract <name> external` declarations
//! and the `external_call` capability bindings.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{
    DoctorAppContract, DoctorAppWorkspace, DoctorDiagnostic, DoctorSeverity, OperationalFacts,
};

pub(crate) fn external_contract_diagnostics(
    contracts: &[DoctorAppContract],
    workspace: Option<&DoctorAppWorkspace>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut contract_names = BTreeMap::new();

    for contract in contracts {
        if let Some(previous) = contract_names.insert(contract.manifest.name.as_str(), contract) {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "CONTRACT-001".to_owned(),
                message: format!(
                    "contract `{}` is declared more than once; first seen in {}.",
                    contract.manifest.name,
                    previous.path.display()
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if contract.manifest.imports.is_empty()
            && contract.manifest.operations.is_empty()
            && contract.manifest.events.is_empty()
        {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "CONTRACT-002".to_owned(),
                message: format!(
                    "contract `{}` declares no imports, operations, or events.",
                    contract.manifest.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for operation in &contract.manifest.operations {
            if operation.transport.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-001".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare `transport http|rpc|event`.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.transport.as_deref() == Some("http")
                && (operation.method.is_none() || operation.path.is_none())
            {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-002".to_owned(),
                    message: format!(
                        "contract `{}` HTTP operation `{}` should declare both `method` and `path`.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.input.is_none() || operation.output.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-003".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare input and output records.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.timeout.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-004".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare timeout so Go transport bindings do not infer it.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for event in &contract.manifest.events {
            if event.topic.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-EVENT-001".to_owned(),
                    message: format!(
                        "contract `{}` event `{}` should declare a topic.",
                        contract.manifest.name, event.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    if let Some(workspace) = workspace
        && !contracts.is_empty()
    {
        for app in &workspace.manifest.apps {
            let Some(contract_name) = app.contract.as_deref() else {
                continue;
            };
            if !contract_names.contains_key(contract_name) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-CONTRACT-001".to_owned(),
                    message: format!(
                        "workspace app `{}` references external contract `{contract_name}`, but no local `contract {contract_name}` block was found in this package.",
                        app.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn external_call_contract_diagnostics(
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_slots: BTreeSet<_> = operational
        .integration_requirements
        .iter()
        .map(|requirement| (requirement.feature.as_str(), requirement.slot.as_str()))
        .collect();

    for call in &operational.external_calls {
        if !declared_slots.contains(&(call.feature.as_str(), call.slot.as_str())) {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-001".to_owned(),
                message: format!(
                    "`{}` calls `{}.{}`, but feature `{}` does not declare `requires integration {}: <Contract>`.",
                    call.subject, call.slot, call.operation, call.feature, call.slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if !call.has_timeout {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-002".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without an explicit `timeout \"...\"` on the {} block.",
                    call.subject, call.slot, call.operation, call.subject_kind
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if !call.has_retry {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-003".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible `retry <count> backoff <strategy>` policy.",
                    call.subject, call.slot, call.operation
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if call.subject_kind == "job" && !call.has_idempotency {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-004".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible job `idempotency by ...` key.",
                    call.subject, call.slot, call.operation
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    //! W3-3: the external-contract + external-call aggregators had no
    //! inline test. These pin CONTRACT-001 (duplicate contract),
    //! CONTRACT-002 (empty contract), CONTRACT-OP-001 (operation missing
    //! transport), CONTRACT-EVENT-001 (event missing topic), and the
    //! security/correctness external-call resilience codes INT-CALL-001
    //! (calling an undeclared integration slot — an ungoverned outbound
    //! call), INT-CALL-002 (no timeout — a runtime hang/cascade risk), and
    //! INT-CALL-003 (no retry). A clean contract/call set stays quiet.
    use super::*;
    use crate::doctor::ExternalCallFact;
    use lazuli_ir::AppContract;
    use std::path::PathBuf;

    fn contract(json: &str) -> DoctorAppContract {
        DoctorAppContract {
            path: PathBuf::from("contract.lzi"),
            manifest: serde_json::from_str::<AppContract>(json).expect("contract json"),
        }
    }

    fn codes(diags: &[DoctorDiagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    // A fully-specified operation that should trip NONE of the per-op rules.
    const FULL_OP: &str = r#"{"name":"charge","transport":"http","method":"POST","path":"/charge","input":"ChargeIn","output":"ChargeOut","timeout":"5s"}"#;

    #[test]
    fn clean_contract_emits_nothing() {
        let c = contract(&format!(
            r#"{{"name":"payments","operations":[{FULL_OP}]}}"#
        ));
        let diags = external_contract_diagnostics(&[c], None);
        assert!(diags.is_empty(), "clean contract, got {:?}", codes(&diags));
    }

    #[test]
    fn contract_001_fires_on_duplicate_contract() {
        let a = contract(&format!(r#"{{"name":"dup","operations":[{FULL_OP}]}}"#));
        let b = contract(&format!(r#"{{"name":"dup","operations":[{FULL_OP}]}}"#));
        let diags = external_contract_diagnostics(&[a, b], None);
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "CONTRACT-001").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one CONTRACT-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn contract_002_fires_on_empty_contract() {
        let c = contract(r#"{"name":"empty"}"#);
        let diags = external_contract_diagnostics(&[c], None);
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "CONTRACT-002").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one CONTRACT-002, got {:?}",
            codes(&diags)
        );
    }

    #[test]
    fn contract_op_001_fires_when_operation_missing_transport() {
        // input/output/timeout present so only the transport rule trips.
        let c = contract(
            r#"{"name":"svc","operations":[{"name":"do","input":"I","output":"O","timeout":"5s"}]}"#,
        );
        let diags = external_contract_diagnostics(&[c], None);
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "CONTRACT-OP-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "want one CONTRACT-OP-001, got {:?}",
            codes(&diags)
        );
    }

    #[test]
    fn contract_event_001_fires_when_event_missing_topic() {
        let c = contract(r#"{"name":"svc","events":[{"name":"order_placed"}]}"#);
        let diags = external_contract_diagnostics(&[c], None);
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "CONTRACT-EVENT-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "want one CONTRACT-EVENT-001, got {:?}",
            codes(&diags)
        );
        assert!(hits[0].message.contains("order_placed"));
    }

    // --- external_call resilience (security/correctness) -----------------

    fn external_call(
        feature: &str,
        slot: &str,
        op: &str,
        has_timeout: bool,
        has_retry: bool,
    ) -> ExternalCallFact {
        ExternalCallFact {
            path: PathBuf::from("billing.lzi"),
            line: 4,
            column: 2,
            feature: feature.to_owned(),
            subject_kind: "command".to_owned(),
            subject: "billing.charge".to_owned(),
            slot: slot.to_owned(),
            operation: op.to_owned(),
            has_timeout,
            has_retry,
            has_idempotency: false,
        }
    }

    fn ops_with(reqs: &[(&str, &str)], calls: Vec<ExternalCallFact>) -> OperationalFacts {
        OperationalFacts {
            integration_requirements: reqs
                .iter()
                .map(
                    |(feature, slot)| crate::doctor::IntegrationRequirementFact {
                        path: PathBuf::from("billing.lzi"),
                        line: 1,
                        column: 1,
                        feature: (*feature).to_owned(),
                        slot: (*slot).to_owned(),
                        contract: "Contract".to_owned(),
                    },
                )
                .collect(),
            external_calls: calls,
            ..OperationalFacts::default()
        }
    }

    #[test]
    fn int_call_001_fires_on_undeclared_integration_slot() {
        // call to billing.payments but no `requires integration payments`.
        let ops = ops_with(
            &[],
            vec![external_call("billing", "payments", "charge", true, true)],
        );
        let diags = external_call_contract_diagnostics(&ops);
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "INT-CALL-001").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one INT-CALL-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn int_call_002_fires_when_call_has_no_timeout() {
        let ops = ops_with(
            &[("billing", "payments")],
            vec![external_call("billing", "payments", "charge", false, true)],
        );
        let diags = external_call_contract_diagnostics(&ops);
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "INT-CALL-002").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one INT-CALL-002, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn int_call_003_fires_when_call_has_no_retry() {
        let ops = ops_with(
            &[("billing", "payments")],
            vec![external_call("billing", "payments", "charge", true, false)],
        );
        let diags = external_call_contract_diagnostics(&ops);
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "INT-CALL-003").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one INT-CALL-003, got {:?}",
            codes(&diags)
        );
    }

    #[test]
    fn well_governed_external_call_emits_nothing() {
        let ops = ops_with(
            &[("billing", "payments")],
            vec![external_call("billing", "payments", "charge", true, true)],
        );
        let diags = external_call_contract_diagnostics(&ops);
        assert!(diags.is_empty(), "governed call, got {:?}", codes(&diags));
    }
}
