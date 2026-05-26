//! Core analyzer test suite — split by domain into sibling files
//! (Rails-style R9 split). Each sub-module carries its own imports
//! and preserves 4-space outer indentation verbatim so raw-string
//! fixtures inside don't shift.

mod agent;
mod auth;
mod design;
mod feature_defaults;
mod field_and_type;
mod module_lowering;
mod policy_and_errors;
mod query_filter;
mod workflow;
