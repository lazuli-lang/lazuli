//! Diagnostics for the `contract` family (external contracts).
//!
//! External contracts (`contract <namespace.version>` in `contracts/*.lzi`)
//! declare the public shape of a service: imports, records, operations,
//! and events. This module owns the file-local shape checks on that
//! surface plus the small contract-token validators
//! ([`is_contract_name`], [`is_contract_type_token`]) the dispatcher
//! consumes.
//!
//! Sub-concerns:
//!
//! | Module | Concern |
//! |---|---|
//! | [`contract`] | `external_contract_diagnostics` + record/operation/event/field shape + `is_contract_*` validators. |
//! | [`requirements`] | `feature.requires integration <slot>: <Contract>` shape + `parse_feature_integration_requirement`. |
//! | [`calls`] | `calls <slot>.<operation>` shape + reliability surface (timeout/retry/idempotency) on commands and jobs. |
//!
//! All producers are dispatched from `crate::dispatch`; sub-helpers
//! stay pub(crate) and ride the `pub(crate) use diagnostics::external::*;`
//! re-export so existing `crate::*` paths used by neighbouring catalog
//! modules keep resolving. Strictly additive ABI.

mod calls;
mod contract;
mod requirements;

#[allow(unused_imports)]
pub(crate) use calls::{
    ExternalCallBlockFacts, ExternalCallLine, external_call_block_diagnostics,
    external_call_contract_diagnostics, parse_external_call_header,
};
#[allow(unused_imports)]
pub(crate) use contract::{
    external_contract_diagnostics, is_contract_name, is_contract_operation_error,
    is_contract_operation_idempotency, is_contract_operation_retry, is_contract_type_token,
    validate_contract_field_line, validate_contract_import_line, validate_contract_operation_line,
};
#[allow(unused_imports)]
pub(crate) use requirements::{
    feature_requirements_contract_diagnostics, parse_feature_integration_requirement,
    validate_feature_requirement_line,
};
