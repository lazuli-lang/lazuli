//! Auxiliary view-list emitter tests. Kept verbatim from the original
//! `lzx_aux.rs` inline test module.

use super::*;
use crate::lzx::ir::*;

fn surface() -> Surface {
    Surface {
        feature: "item".to_owned(),
        target: SurfaceTarget::Web,
        audiences: vec![],
        span_ref: None,
    }
}

fn view() -> ViewList {
    ViewList {
        name: "item_terminal".to_owned(),
        route: None,
        source: QueryRef {
            feature: "item".to_owned(),
            kind: QueryKind::List,
            name: "search".to_owned(),
        },
        render: ListRender::Table {
            columns: vec!["id".to_owned()],
        },
        search: None,
        filter: vec![],
        cells: vec![],
        actions: vec![],
        drawer: None,
        sort: None,
        selection: None,
        settings: vec![],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    }
}

fn command(name: &str) -> CommandRef {
    CommandRef {
        feature: "item".to_owned(),
        name: name.to_owned(),
    }
}

#[test]
fn emits_sort_state_and_return_field() {
    let mut view = view();
    view.sort = Some(SortDecl {
        allowed: vec!["title".into(), "updated".into()],
        default_field: "updated".into(),
        default_dir: SortDir::Desc,
        span_ref: None,
    });

    let mut state = String::new();
    write_hook_state(&mut state, &surface(), &view);
    // The `SortField` alias holds the literal union; the useState
    // generic now references it instead of inlining the union, so the
    // closure typed signature below can reuse the same name.
    assert!(state.contains("type SortField = \"title\" | \"updated\";"));
    assert!(state.contains("useState<{ field: SortField; dir: \"asc\" | \"desc\" }>"));
    assert!(state.contains("field: \"updated\", dir: \"desc\""));

    let mut ret = String::new();
    write_return_fields(&mut ret, &view);
    // `field: SortField, dir: "asc" | "desc"` — fixes TS7006 + TS2322.
    assert!(ret.contains("sort: { field: sort.field, dir: sort.dir, set: (field: SortField, dir: \"asc\" | \"desc\" = \"desc\") => setSort({ field, dir }) }"));
}

#[test]
fn emits_multi_selection_state_bulk_command_and_return() {
    let mut view = view();
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![command("delete")],
        span_ref: None,
    });

    let mut state = String::new();
    write_hook_state(&mut state, &surface(), &view);
    // Resource ID type is threaded via indexed-access (`Item["id"]`)
    // so number / string / branded IDs flow without casts.
    assert!(state
        .contains("const selection = useMultiSelection<Item[\"id\"]>(query.data ?? []);"));
    assert!(state.contains("const bulkDelete = useLazuliCommand(deleteItem);"));

    let mut ret = String::new();
    write_return_fields(&mut ret, &view);
    assert!(ret.contains("mode: \"multi\""));
    assert!(ret.contains("ids: selection.ids"));
    assert!(ret.contains("bulk: { delete: bulkDelete }"));
}

#[test]
fn emits_single_selection_state_and_return() {
    let mut view = view();
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Single,
        bulk_actions: vec![],
        span_ref: None,
    });

    let mut state = String::new();
    write_hook_state(&mut state, &surface(), &view);
    assert!(
        state.contains("const [selectionId, setSelectionId] = useState<string | null>(null);")
    );

    let mut ret = String::new();
    write_return_fields(&mut ret, &view);
    assert!(ret.contains("mode: \"single\""));
    assert!(ret.contains("clear: () => setSelectionId(null)"));
}

#[test]
fn omits_none_selection() {
    let mut view = view();
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::None,
        bulk_actions: vec![],
        span_ref: None,
    });

    let mut out = String::new();
    write_hook_state(&mut out, &surface(), &view);
    write_return_fields(&mut out, &view);
    assert!(!out.contains("selection"));
}

#[test]
fn emits_enum_local_setting_key_state_and_return() {
    let mut view = view();
    view.settings.push(SettingDecl {
        name: "grid_size".to_owned(),
        value_space: SettingValueSpace::Enum {
            values: vec!["sm".into(), "md".into(), "lg".into()],
        },
        default: "sm".to_owned(),
        persistence: SettingPersistence::Local,
        span_ref: None,
    });

    let mut out = String::new();
    // Empty app_name → falls back to surface.feature ("item") — preserves
    // the pre-fix behaviour for legacy callers; the app-scoped key path is
    // exercised by `settings_namespace_is_app_scoped` below.
    write_setting_keys(&mut out, &surface(), &view, "");
    write_hook_state(&mut out, &surface(), &view);
    write_return_fields(&mut out, &view);
    assert!(out.contains("const SETTING_KEY_GRID_SIZE = \"item:item-terminal:grid_size\";"));
    assert!(out.contains("const [gridSize, setGridSize] = useLocalSetting<\"sm\" | \"md\" | \"lg\">(SETTING_KEY_GRID_SIZE, \"sm\");"));
    assert!(out.contains("settings: { gridSize, setGridSize }"));
}

#[test]
fn emits_bool_ephemeral_setting_with_use_state() {
    let mut view = view();
    view.settings.push(SettingDecl {
        name: "show_archived".to_owned(),
        value_space: SettingValueSpace::Bool,
        default: "false".to_owned(),
        persistence: SettingPersistence::None,
        span_ref: None,
    });

    let mut out = String::new();
    write_hook_state(&mut out, &surface(), &view);
    assert!(out.contains("const [showArchived, setShowArchived] = useState<boolean>(false);"));
}

#[test]
fn emits_int_local_setting_as_number() {
    let mut view = view();
    view.settings.push(SettingDecl {
        name: "page_size".to_owned(),
        value_space: SettingValueSpace::Int { min: 10, max: 200 },
        default: "25".to_owned(),
        persistence: SettingPersistence::Local,
        span_ref: None,
    });

    let mut out = String::new();
    write_hook_state(&mut out, &surface(), &view);
    assert!(out.contains(
        "const [pageSize, setPageSize] = useLocalSetting<number>(SETTING_KEY_PAGE_SIZE, 25);"
    ));
}

#[test]
fn emits_workspace_setting_todo_and_local_fallback() {
    let mut view = view();
    view.settings.push(SettingDecl {
        name: "grid_size".to_owned(),
        value_space: SettingValueSpace::Enum {
            values: vec!["sm".into(), "md".into()],
        },
        default: "md".to_owned(),
        persistence: SettingPersistence::Workspace,
        span_ref: None,
    });

    let mut out = String::new();
    write_hook_state(&mut out, &surface(), &view);
    assert!(out.contains("// TODO: workspace persistence pending L0 #7"));
    assert!(out.contains("useLocalSetting<\"sm\" | \"md\">"));
}

#[test]
fn detects_needed_import_helpers() {
    let mut view = view();
    assert!(!needs_use_state(&view));
    assert!(!needs_multi_selection(&view));
    assert!(!needs_local_setting(&view));

    view.sort = Some(SortDecl {
        allowed: vec!["updated".into()],
        default_field: "updated".into(),
        default_dir: SortDir::Asc,
        span_ref: None,
    });
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![command("delete")],
        span_ref: None,
    });
    view.settings.push(SettingDecl {
        name: "density".to_owned(),
        value_space: SettingValueSpace::Enum {
            values: vec!["compact".into()],
        },
        default: "compact".to_owned(),
        persistence: SettingPersistence::Local,
        span_ref: None,
    });

    assert!(needs_use_state(&view));
    assert!(needs_multi_selection(&view));
    assert!(needs_local_setting(&view));
    assert!(needs_bulk_commands(&view));
    // SDK import identifier preserves the canonical command name
    // (`deleteItem`); only the return-object key strips a `bulk_`
    // prefix (asserted in `bulk_action_key_strips_command_prefix`).
    assert_eq!(unique_bulk_command_imports(&view), vec!["deleteItem"]);
}

// -----------------------------------------------------------------
// Bug-fix regression tests (pilot dogfood, 2026-05-14).
// -----------------------------------------------------------------

#[test]
fn bulk_action_key_strips_command_prefix() {
    // Bug #1: `selection.bulk.bulkDelete` instead of
    // `selection.bulk.delete`. The bulk command was named
    // `bulk_delete` upstream — the canonical downstream shape — and the
    // return-object key was prefix-duplicating the `bulk_` token.
    let mut view = view();
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![command("bulk_delete")],
        span_ref: None,
    });

    let mut ret = String::new();
    write_return_fields(&mut ret, &view);
    // KEY drops the `bulk_` prefix → `delete`.
    assert!(
        ret.contains("bulk: { delete: bulkBulkDelete }"),
        "got: {ret}"
    );
    // No stutter — never `bulk: { bulkDelete:` (the previous shape).
    assert!(
        !ret.contains("bulk: { bulkDelete:"),
        "should strip bulk_ prefix; got: {ret}"
    );

    // SDK import identifier is unchanged (full canonical name).
    let import = bulk_command_ident(&command("bulk_delete"));
    assert_eq!(import, "bulkItemDelete");
}

#[test]
fn multi_selection_threads_resource_id_type() {
    // Bug #2: `useMultiSelection<string>` was hardcoded. The
    // resource interface (`Item`) is the source of truth — emit
    // indexed-access `Item["id"]` so number ids flow without casts.
    let mut view = view();
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![],
        span_ref: None,
    });

    let mut state = String::new();
    write_hook_state(&mut state, &surface(), &view);
    assert!(state
        .contains("const selection = useMultiSelection<Item[\"id\"]>(query.data ?? []);"));
    // No `<string>` literal anywhere in the multi-selection state.
    assert!(!state.contains("useMultiSelection<string>"));
}

#[test]
fn sort_set_emits_typed_signature() {
    // Bug #3: `sort.set: (field, dir = "desc") => ...` was emitted
    // without type annotations — TS7006 implicit-any on `field`,
    // TS2322 narrowed `dir` to the literal `"desc"`.
    let mut view = view();
    view.sort = Some(SortDecl {
        allowed: vec!["updated_at".into(), "name".into()],
        default_field: "updated_at".into(),
        default_dir: SortDir::Desc,
        span_ref: None,
    });

    let mut state = String::new();
    write_hook_state(&mut state, &surface(), &view);
    // SortField alias precedes the useState call.
    assert!(state.contains("type SortField = \"updated_at\" | \"name\";"));

    let mut ret = String::new();
    write_return_fields(&mut ret, &view);
    assert!(
        ret.contains("set: (field: SortField, dir: \"asc\" | \"desc\" = \"desc\") => setSort"),
        "typed sort.set signature missing; got: {ret}"
    );
}

#[test]
fn settings_namespace_is_app_scoped() {
    // Bug #4: localStorage key was `<feature>:<view>:<setting>` —
    // collision risk across apps that share a feature/view name.
    // Per proposal §3.7 the namespace should be the app/project
    // name from `Lazurite.toml`.
    let mut view = view();
    view.settings.push(SettingDecl {
        name: "grid_size".to_owned(),
        value_space: SettingValueSpace::Enum {
            values: vec!["sm".into(), "md".into(), "lg".into()],
        },
        default: "sm".to_owned(),
        persistence: SettingPersistence::Local,
        span_ref: None,
    });

    let mut out = String::new();
    write_setting_keys(&mut out, &surface(), &view, "example");
    assert!(
        out.contains(
            "const SETTING_KEY_GRID_SIZE = \"example:item-terminal:grid_size\";"
        ),
        "expected `example:` prefix; got: {out}"
    );
    // No legacy feature-scoped prefix.
    assert!(!out.contains("\"item:item-terminal:"));
}
