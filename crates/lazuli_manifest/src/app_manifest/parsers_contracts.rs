//! Contract-specific line-level parsers for `parse_app_contracts`.
//! Each helper is `pub(super)` and re-exported from `parsers.rs`.

use lazuli_ir::{ContractField, ContractImport, ContractOperationError};

use super::parsers_common::{is_identifier, unquote};

pub(super) fn parse_contract_import(trimmed: &str) -> Option<ContractImport> {
    let rest = trimmed.strip_prefix("import ")?;
    let parts: Vec<_> = rest.split_whitespace().collect();
    if parts.len() == 2 && is_contract_import_format(parts[0]) {
        Some(ContractImport {
            format: parts[0].to_owned(),
            source: unquote(parts[1]).to_owned(),
        })
    } else {
        None
    }
}

pub(super) fn is_contract_import_format(value: &str) -> bool {
    matches!(
        value,
        "openapi" | "asyncapi" | "proto" | "json_schema" | "avro"
    )
}

pub(super) fn parse_contract_operation_error(rest: &str) -> Option<ContractOperationError> {
    // Shape: `<Name> [status <code>] [expose <field>, <field>...]`
    let mut tokens = rest.split_whitespace();
    let name = tokens.next()?.to_owned();
    let mut status = None;
    let mut expose: Vec<String> = Vec::new();

    let mut state = "start";
    for token in tokens {
        match (state, token) {
            (_, "status") => state = "status",
            (_, "expose") => state = "expose",
            ("status", value) => {
                status = Some(value.to_owned());
                state = "after";
            }
            ("expose", value) => {
                expose.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .map(str::to_owned),
                );
            }
            _ => {}
        }
    }

    Some(ContractOperationError {
        name,
        status,
        expose,
    })
}

pub(super) fn parse_contract_field(trimmed: &str) -> Option<ContractField> {
    let (name, rest) = trimmed.split_once(':')?;
    let name = name.trim();
    if !is_identifier(name) {
        return None;
    }

    let mut parts: Vec<_> = rest.split_whitespace().collect();
    let requiredness = parts
        .last()
        .copied()
        .filter(|value| matches!(*value, "required" | "optional"))
        .map(str::to_owned);
    if requiredness.is_some() {
        parts.pop();
    }

    let type_name = parts.first()?.to_string();
    let markers = parts
        .iter()
        .skip(1)
        .filter(|part| part.starts_with('@'))
        .map(|part| (*part).to_owned())
        .collect();

    Some(ContractField {
        name: name.to_owned(),
        type_name,
        markers,
        requiredness,
    })
}
