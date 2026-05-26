//! `view list` emitter — emits a typed `<audience><View>View` spec const
//! + `use<Audience><View>View` hook bundling the source query and all
//! action mutations, plus a slot interface for any `cells <field>
//! @client.<slot>` bindings.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.1.
//!
//! ## Module layout
//!
//! - `imports` — three-stage import block emission (runtime hooks +
//!   React hooks, feature SDK imports, cell-slot prop types).
//! - `spec` — the compile-time `<view>View` const, the
//!   `_AssertColumns` type guard for table views, and the
//!   `<Audience><View>Slots` / `Cells` interface for slot bindings.
//! - `hook` — the `use<Audience><View>View` hook body, including
//!   drawer state, multi-selection wiring, search/filter binding,
//!   and the cellClick dispatcher.

mod hook;
mod imports;
mod spec;

#[cfg(test)]
mod tests;

use crate::lzx::lzx_aux;
use crate::lzx::{Audience, Surface, ViewList, banner};

use hook::write_hook;
use imports::write_imports;
use spec::{write_column_assert, write_slot_interface, write_spec_const};

/// Emit `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`.
pub fn emit_view_list(
    surface: &Surface,
    audience: &Audience,
    view: &ViewList,
    app_name: &str,
) -> String {
    let mut s = String::new();
    s.push_str(banner());

    write_imports(&mut s, surface, view);
    lzx_aux::write_setting_keys(&mut s, surface, view, app_name);
    write_spec_const(&mut s, audience, view);
    write_column_assert(&mut s, audience, view, surface);
    write_slot_interface(&mut s, audience, view, surface);
    write_hook(&mut s, audience, view, surface);

    s
}
