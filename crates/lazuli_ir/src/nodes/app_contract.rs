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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractImport {
    pub format: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ContractField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requiredness: Option<String>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperationError {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<ContractField>,
}
