//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

include!("group_01_query_statement_unknown_flags_ty_p1_tests.rs");
include!("group_01_query_statement_unknown_flags_ty_p2_tests.rs");
