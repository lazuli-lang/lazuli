//! Diagnostics for the LZX surface contract family.
//!
//! `.lzx` files declare experiences and per-stack surface projections
//! (`*.web.lzx`, `*.mobile.lzx`, etc.). Sub-concerns split:
//!
//! | Module | Concern |
//! |---|---|
//! | [`contract`] | `experience` / `surface` headers and audience scopes. |
//! | [`route`] | Top-level `route <name>: <Path>` declarations + `view <kind> <name> for route.<slot>` references. |
//! | [`filename`] | File-name → platform / audience pairing. |
//! | [`lex`] | Tiny shared lexers (`unquote_lzx_literal`, `split_items`, `lzx_declared_path_params`, `route_slot_name`, `lzx_route_references`, `path_references`). |
//!
//! The lex helpers are heavily consumed by other catalog modules
//! (api, cache, profile, workspace, external, app, …) and ride the
//! standard `pub(crate) use diagnostics::lzx::*;` re-export so every
//! existing `crate::*` import keeps resolving.

mod contract;
mod filename;
mod lex;
mod route;

#[allow(unused_imports)]
pub(crate) use contract::lzx_contract_diagnostics;
#[allow(unused_imports)]
pub(crate) use filename::{
    first_lzx_surface_header, lzx_filename_diagnostics, lzx_platform_from_file_name,
};
#[allow(unused_imports)]
pub(crate) use lex::{
    is_quoted_lzx_literal, lzx_declared_path_params, lzx_route_references, path_references,
    route_slot_name, split_items, unquote_lzx_literal,
};
#[allow(unused_imports)]
pub(crate) use route::{
    LzxAppRouteFacts, LzxRouteViewFacts, lzx_app_route_diagnostics, lzx_route_contract_diagnostics,
    lzx_route_view_diagnostics,
};
