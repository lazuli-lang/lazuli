//! `.lzx` app-surface lowering — document, app, routes, experiences.
//!
//! ## Why this slot exists
//!
//! `.lzx` files carry the experience layer: the app manifest, route
//! table, experience definitions (view groupings + extension points),
//! and platform surfaces (web / mobile audiences). The lowering here
//! is mechanical projection — `syntax::Lzx*` AST → `ir::Experience*`
//! shapes — because the parser already enforces structural shape and
//! the doctor cells run cross-module reasoning later.
//!
//! Compared to `feature.rs` (which carries the `.lzi` ViewModel
//! surface lowering: `lower_surface` / `lower_view_ast`), the
//! functions here never validate against feature scope. Their entire
//! domain is the `.lzx` document tree.
//!
//! ## Public API
//!
//! Only `lower_lzx_document` is exported. Everything else is
//! `pub(crate)` and used only by the document walker.
//!
//! Source AST shapes: `lazuli_syntax::LzxDocument` and friends.
//! Destination IR shapes: `lazuli_ir::ExperienceModule` family.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::helpers::span_of;

include!("lzx_p1.rs");
include!("lzx_p2.rs");
