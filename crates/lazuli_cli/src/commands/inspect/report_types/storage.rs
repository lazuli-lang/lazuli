//! `--expand=storage` projection shapes (Phase L Tier 2).
//!
//! Every typed `@cap.File(...)` site in a feature surfaces here: the
//! `fields` row covers `resource.field: @cap.File(...)`; the
//! `api_outputs` row covers `api ... output @cap.File(...)`. Omitted
//! entirely when no `@cap.File` is authored on the feature.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectStorage {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) fields: Vec<InspectStorageField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) api_outputs: Vec<InspectStorageApiOutput>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectStorageField {
    pub(in crate::commands::inspect) resource: String,
    pub(in crate::commands::inspect) field: String,
    pub(in crate::commands::inspect) file_capability: InspectFileCapability,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectStorageApiOutput {
    pub(in crate::commands::inspect) api: String,
    pub(in crate::commands::inspect) file_capability: InspectFileCapability,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectFileCapability {
    pub(in crate::commands::inspect) max_size: InspectFileSize,
    pub(in crate::commands::inspect) accept: Vec<InspectMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) signed_ttl: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectFileSize {
    pub(in crate::commands::inspect) bytes: u64,
    pub(in crate::commands::inspect) literal: String,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectMimeType {
    pub(in crate::commands::inspect) family: String,
    pub(in crate::commands::inspect) subtype: String,
}
