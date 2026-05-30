//! End-to-end `parse_lzx_document` tests covering the full `.lzx` surface.
//!
//! Lives as a sibling of `parser::lzx::mod` (rather than inline) so the
//! module file stays under the 500-LOC ceiling. Raw-string fixtures are
//! verbatim — de-indenting them corrupts the canonical-indent contract
//! the parser asserts. The enclosing module path is preserved
//! (`parser::lzx::lzx_parser_tests::...`) via the
//! `#[cfg(test)] #[path = "lzx_parser_tests.rs"] mod lzx_parser_tests;`
//! declaration in `mod.rs`.

use super::parse_lzx_document;
use crate::{LzxPlatform, LzxScalarLiteral, LzxViewTestAssertion};

// Wave 4 — parser must lift view `tests` into the typed
// `LzxViewTestAssertion` enum. `allows extension` / `denies extension` are
// the only admissible shapes; anything else is a hard parse error.
include!("lzx_parser_p1_tests.rs");
include!("lzx_parser_p2_tests.rs");
