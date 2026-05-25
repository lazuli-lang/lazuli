//! Import-block emission for a generated `view list` file.
//!
//! Three import blocks are emitted in order:
//!
//! 1. Runtime hooks (`@lazuli/runtime/react`) — every conditional
//!    hook gated on whether the view actually uses it
//!    (`useLazuliCommand` only when actions/drawer-delete/bulk fire,
//!    `useDrawerSubView` only when a `drawer` is declared, and so on).
//! 2. Feature SDK imports — resource type + source query + each action
//!    command, grouped by feature so consumers see one `import { … }
//!    from "../../<feat>.gen.js"` per origin.
//! 3. Cell-slot prop types — one `import type { <Slot>Props }` per
//!    `cells <field> @client.<slot>` binding.
//!
//! Hook emission must stay in sync with the gates inside
//! `write_hook` (sibling module): if the hook references `useState`,
//! it has to be imported here, and `cargo test smoke_ts_typecheck`
//! catches the drift via `tsc --noEmit`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::lzx::lzx_aux;
use crate::lzx::lzx_filters;
use crate::lzx::lzx_search;
use crate::lzx::{Surface, ViewList, command_ident, pascal_case, query_ident};

pub(super) fn write_imports(s: &mut String, surface: &Surface, view: &ViewList) {
    let feature_pascal = pascal_case(&surface.feature);
    let has_drawer = view.drawer.is_some();
    let has_drawer_delete = super::hook::drawer_delete_action(view).is_some();
    let has_multi_selection = super::hook::is_multi_selection(view);
    let has_filters = !view.filter.is_empty();
    let has_search = view.search.is_some();
    let has_segmented_search = lzx_search::needs_segmented_imports(view.search.as_ref());
    let has_url_synced_filters = lzx_filters::has_url_synced_filters(&view.filter);

    // 1. Runtime hooks.
    writeln!(s, "import {{").ok();
    writeln!(s, "  useLazuliQuery,").ok();
    if !view.actions.is_empty() || has_drawer_delete || lzx_aux::needs_bulk_commands(view) {
        writeln!(s, "  useLazuliCommand,").ok();
    }
    if has_drawer {
        writeln!(s, "  useDrawerSubView,").ok();
    }
    if has_multi_selection || lzx_aux::needs_multi_selection(view) {
        writeln!(s, "  useMultiSelection,").ok();
    }
    if lzx_aux::needs_local_setting(view) {
        writeln!(s, "  useLocalSetting,").ok();
    }
    if has_filters {
        writeln!(s, "  useFilterState,").ok();
    }
    if has_url_synced_filters {
        // TODO: introduce useUrlParams() helper in @lazuli/runtime/react.
        writeln!(s, "  useUrlParams,").ok();
    }
    if has_segmented_search {
        writeln!(s, "  parseSegments,").ok();
        writeln!(s, "  canonicalizeSearch,").ok();
    }
    writeln!(s, "  type UseLazuliQueryOptions,").ok();
    writeln!(s, "}} from \"@lazuli/runtime/react\";").ok();

    // React hooks — combine search-driven needs with the existing
    // drawer/aux state needs so we never emit two `import ... from "react"`
    // lines.
    let needs_use_state = has_drawer || lzx_aux::needs_use_state(view) || has_search;
    let needs_use_callback = has_drawer || has_search;
    let needs_use_memo = has_segmented_search;
    let needs_react_type = has_drawer || !view.cells.is_empty();
    if needs_use_state || needs_use_callback || needs_use_memo {
        let mut parts: Vec<&str> = Vec::new();
        if needs_use_callback {
            parts.push("useCallback");
        }
        if needs_use_memo {
            parts.push("useMemo");
        }
        if needs_use_state {
            parts.push("useState");
        }
        writeln!(s, "import {{ {} }} from \"react\";", parts.join(", ")).ok();
    }
    if needs_react_type {
        writeln!(s, "import type * as React from \"react\";").ok();
    }
    if has_drawer {
        writeln!(
            s,
            "import {{ useRouterState }} from \"@tanstack/react-router\";"
        )
        .ok();
    }
    if has_url_synced_filters {
        writeln!(
            s,
            "import {{ useSearch as useRouterSearch, useNavigate }} from \"@tanstack/react-router\";"
        )
        .ok();
    }
    if has_segmented_search {
        writeln!(
            s,
            "import {{ parse as parseSearchQuery, type SearchParserResult, type SearchParserOptions }} from \"search-query-parser\";"
        )
        .ok();
    }

    // 2. Feature SDK — resource type + source query + each action command.
    let mut sdk_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    sdk_imports
        .entry(view.source.feature.clone())
        .or_default()
        .insert(query_ident(&view.source));
    for cmd in &view.actions {
        sdk_imports
            .entry(cmd.feature.clone())
            .or_default()
            .insert(command_ident(cmd));
    }
    if let Some(drawer) = &view.drawer {
        sdk_imports
            .entry(drawer.source.feature.clone())
            .or_default()
            .insert(query_ident(&drawer.source));
        for cmd in &drawer.actions {
            sdk_imports
                .entry(cmd.feature.clone())
                .or_default()
                .insert(command_ident(cmd));
        }
    }
    for ident in lzx_aux::unique_bulk_command_imports(view) {
        sdk_imports
            .entry(surface.feature.clone())
            .or_default()
            .insert(ident);
    }
    for ident in lzx_filters::enum_value_imports(&view.filter) {
        sdk_imports
            .entry(surface.feature.clone())
            .or_default()
            .insert(ident);
    }
    sdk_imports
        .entry(surface.feature.clone())
        .or_default()
        .insert(format!("type {}", feature_pascal));

    for (feature, imports) in sdk_imports {
        writeln!(s, "import {{").ok();
        for import in imports {
            writeln!(s, "  {},", import).ok();
        }
        writeln!(s, "}} from \"../../{}.gen.js\";", feature).ok();
    }

    // 3. Cell slot prop types — one import per binding.
    for cell in &view.cells {
        let pascal_slot = pascal_case(&cell.slot);
        writeln!(
            s,
            "import type {{ {}Props }} from \"../../cells/{}.gen.js\";",
            pascal_slot, cell.slot
        )
        .ok();
    }
    s.push('\n');
}
