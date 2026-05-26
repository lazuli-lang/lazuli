//! Cross-feature `contract <name>` IR — the typed shape exchanged at
//! microservice boundaries.
//!
//! An `AppContract` collects the records, operations, and events a
//! feature exposes to other apps. Each `ContractOperation` declares
//! its transport / method / path / input / output / timeout / retry
//! / idempotency in framework-agnostic terms — the runtime adapter
//! plugs its specific HTTP / gRPC / NATS shape.
//!
//! `ContractField.markers` carries free-form annotations
//! (e.g. `@pii.email`) so producers and consumers see the same
//! sanitization expectations.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Root IR node for one `contract <name> { … }` block — the typed
/// shape one feature exposes to other apps. Collects the records,
/// operations (HTTP/gRPC/NATS surfaces), and events that flow across
/// the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppContract {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<ContractImport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<ContractRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<ContractOperation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ContractEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `import <format> "<source>"` — pulls a foreign contract definition
/// in for type re-use. Format is free-form (`proto`, `openapi`,
/// `graphql`); source is the path / URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractImport {
    pub format: String,
    pub source: String,
}

/// One `record <name> { … }` entry inside an [`AppContract`]. Names the
/// shape transmitted across the boundary; fields are stringly typed
/// (the runtime adapter resolves them) so contracts can reference foreign
/// types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ContractField>,
}

/// One field inside a [`ContractRecord`] or [`ContractEvent`] payload.
/// `type_name` is stringly-typed so contracts can reference foreign
/// types; `markers` carry free-form annotations (e.g. `@pii.email`)
/// that downstream sanitizers consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requiredness: Option<String>,
}

/// One `operation <name> { … }` entry on a contract — a typed
/// cross-app callable. Transport (`http`/`grpc`/`nats`), method/path,
/// input/output/error shapes, plus the standard async decorators
/// (timeout, retry, idempotency) all live on this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ContractOperationError>,
}

/// One declared error case on a [`ContractOperation`]. `name` is the
/// error code identifier; `status` is the optional HTTP status mapping;
/// `expose` lists which fields are surfaced to callers (vs internal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperationError {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<String>,
}

/// One `event <name> { … }` entry on a contract — declares an event
/// the feature publishes for other apps to subscribe to. `topic` is
/// the transport-level destination (queue / subject / topic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<ContractField>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_contract_round_trips() {
        let c = AppContract {
            name: "Hosts".into(),
            purpose: None,
            compatibility: None,
            imports: vec![],
            records: vec![],
            operations: vec![],
            events: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: AppContract = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}
