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
