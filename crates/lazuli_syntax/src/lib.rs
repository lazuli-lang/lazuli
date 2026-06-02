// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
mod ast;
mod parser;

pub use ast::*;
pub use parser::doctor_allow;
pub use parser::{
    ClassifiedToken, LifecycleBlockAst, LifecycleInvariantAst, LifecycleInvariantForm,
    LifecycleStateAst, LifecycleTransitionAst, ParseError, PollerBlockAst, PollerCursorAst,
    PollerRetryAst, PollerRetryQuirkAst, PollerStateAst, PollerTickAst, classify_tokens,
    parse_design_document, parse_feature_gates, parse_feature_skeletons, parse_lzx_document,
    parse_package_skeleton, parse_plan_blocks, parse_surface_document,
};
