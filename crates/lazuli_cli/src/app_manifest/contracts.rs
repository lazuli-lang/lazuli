//! Parser for `contracts/*.lzi` — operational contracts shared across apps.
//!
//! A contract file may declare one or more `contract <Name>` blocks. Each
//! block carries `purpose`, `compatibility`, `import` declarations, and any
//! number of `record`, `operation`, and `event` named sub-blocks. The
//! grammar is line-indent driven: indent 0 starts a contract, indent 2
//! opens a sub-block (or simple field), indent 4 fills the sub-block, and
//! indent 6 holds the event payload field list.
//!
//! The parser is intentionally tolerant: unknown indent-4 lines inside an
//! operation are silently dropped (doctor flags shape errors), and
//! malformed `record` field syntax simply skips the line. The shipped IR
//! (`ContractRecord` / `ContractOperation` / `ContractEvent`) captures
//! exactly what was syntactically well-formed.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppContract`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{AppContract, ContractEvent, ContractOperation, ContractRecord};

use super::parsers::{
    leading_spaces, named_block_name, parse_contract_field, parse_contract_import,
    parse_contract_operation_error, unquote,
};

pub fn parse_app_contracts(source: &str) -> Vec<AppContract> {
    let lines: Vec<_> = source.lines().collect();
    let mut contracts = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("contract ") {
            index += 1;
            continue;
        }

        let Some(name) = trimmed.split_whitespace().nth(1) else {
            index += 1;
            continue;
        };

        let start = index;
        index += 1;
        while index < lines.len() {
            let next_trimmed = lines[index].trim_start();
            if leading_spaces(lines[index]) == 0
                && !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
            {
                break;
            }
            index += 1;
        }

        contracts.push(parse_app_contract_block(name, &lines[start + 1..index]));
    }

    contracts
}

fn parse_app_contract_block(name: &str, lines: &[&str]) -> AppContract {
    let mut contract = AppContract {
        name: name.to_owned(),
        purpose: None,
        compatibility: None,
        imports: Vec::new(),
        records: Vec::new(),
        operations: Vec::new(),
        events: Vec::new(),
        span_ref: None,
    };
    let mut current_record: Option<usize> = None;
    let mut current_operation: Option<usize> = None;
    let mut current_event: Option<usize> = None;
    let mut in_event_payload = false;

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            2 => {
                current_record = None;
                current_operation = None;
                current_event = None;
                in_event_payload = false;
                if let Some(rest) = trimmed.strip_prefix("purpose ") {
                    contract.purpose = Some(unquote(rest.trim()).to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("compatibility ") {
                    contract.compatibility = Some(rest.trim().to_owned());
                } else if let Some(import) = parse_contract_import(trimmed) {
                    contract.imports.push(import);
                } else if let Some(name) = named_block_name(trimmed, "record") {
                    contract.records.push(ContractRecord {
                        name: name.to_owned(),
                        fields: Vec::new(),
                    });
                    current_record = contract.records.len().checked_sub(1);
                } else if let Some(name) = named_block_name(trimmed, "operation") {
                    contract.operations.push(ContractOperation {
                        name: name.to_owned(),
                        transport: None,
                        method: None,
                        path: None,
                        input: None,
                        output: None,
                        auth: None,
                        timeout: None,
                        retry: None,
                        idempotency: None,
                        errors: Vec::new(),
                    });
                    current_operation = contract.operations.len().checked_sub(1);
                } else if let Some(name) = named_block_name(trimmed, "event") {
                    contract.events.push(ContractEvent {
                        name: name.to_owned(),
                        topic: None,
                        payload: Vec::new(),
                    });
                    current_event = contract.events.len().checked_sub(1);
                }
            }
            4 => {
                if let Some(record_index) = current_record {
                    if let Some(field) = parse_contract_field(trimmed) {
                        contract.records[record_index].fields.push(field);
                    }
                } else if let Some(operation_index) = current_operation {
                    let operation = &mut contract.operations[operation_index];
                    if let Some(rest) = trimmed.strip_prefix("error ") {
                        if let Some(err) = parse_contract_operation_error(rest) {
                            operation.errors.push(err);
                        }
                        continue;
                    }
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["transport", value] => operation.transport = Some((*value).to_owned()),
                        ["method", value] => operation.method = Some((*value).to_owned()),
                        ["path", value] => operation.path = Some(unquote(value).to_owned()),
                        ["input", value] => operation.input = Some((*value).to_owned()),
                        ["output", rest @ ..] if !rest.is_empty() => {
                            operation.output = Some(rest.join(" "));
                        }
                        ["auth", value] => operation.auth = Some((*value).to_owned()),
                        ["timeout", value] => operation.timeout = Some(unquote(value).to_owned()),
                        ["retry", rest @ ..] if !rest.is_empty() => {
                            operation.retry = Some(rest.join(" "));
                        }
                        ["idempotency", "by", rest @ ..] if !rest.is_empty() => {
                            operation.idempotency = Some(rest.join(" "));
                        }
                        _ => {}
                    }
                } else if let Some(event_index) = current_event {
                    if let Some(rest) = trimmed.strip_prefix("topic ") {
                        contract.events[event_index].topic = Some(unquote(rest.trim()).to_owned());
                        in_event_payload = false;
                    } else {
                        in_event_payload = trimmed == "payload";
                    }
                }
            }
            6 => {
                if in_event_payload
                    && let Some(event_index) = current_event
                    && let Some(field) = parse_contract_field(trimmed)
                {
                    contract.events[event_index].payload.push(field);
                }
            }
            _ => {}
        }
    }

    contract
}
