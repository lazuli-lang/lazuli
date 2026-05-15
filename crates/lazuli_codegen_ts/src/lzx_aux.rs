//! Auxiliary `view list` state emitters for L0 #6 terminal grammar:
//! sort, selection, and settings. These functions only write TypeScript
//! wire code around React state and runtime helper hooks.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::lzx::{
    command_ident, lower_camel, pascal_case, CommandRef, SelectionDecl, SelectionMode, SettingDecl,
    SettingPersistence, SettingValueSpace, SortDecl, SortDir, Surface, ViewList,
};

pub(crate) fn needs_use_state(view: &ViewList) -> bool {
    view.sort.is_some()
        || matches!(
            view.selection.as_ref().map(|s| s.mode),
            Some(SelectionMode::Single)
        )
        || view
            .settings
            .iter()
            .any(|setting| setting.persistence == SettingPersistence::None)
}

pub(crate) fn needs_multi_selection(view: &ViewList) -> bool {
    matches!(
        view.selection.as_ref().map(|s| s.mode),
        Some(SelectionMode::Multi)
    )
}

pub(crate) fn needs_local_setting(view: &ViewList) -> bool {
    view.settings
        .iter()
        .any(|setting| setting.persistence != SettingPersistence::None)
}

pub(crate) fn bulk_actions(view: &ViewList) -> Vec<&CommandRef> {
    view.selection
        .as_ref()
        .filter(|selection| selection.mode == SelectionMode::Multi)
        .map(|selection| selection.bulk_actions.iter().collect())
        .unwrap_or_default()
}

pub(crate) fn needs_bulk_commands(view: &ViewList) -> bool {
    !bulk_actions(view).is_empty()
}

pub(crate) fn write_setting_keys(
    s: &mut String,
    surface: &Surface,
    view: &ViewList,
    app_name: &str,
) {
    if view.settings.is_empty() {
        return;
    }

    // Per proposal §3.7 the localStorage namespace is `<app>:<view>:<setting>`
    // so two different apps don't collide on the same view name. If the
    // codegen entry didn't thread an app name (tests / ad-hoc callers), fall
    // back to the feature name — preserves the previous behaviour so the
    // key stays stable for in-tree consumers that haven't been updated yet.
    let app = if app_name.is_empty() {
        surface.feature.as_str()
    } else {
        app_name
    };

    for setting in &view.settings {
        writeln!(
            s,
            "const SETTING_KEY_{} = \"{}:{}:{}\";",
            upper_snake(&setting.name),
            app,
            kebab_case(&view.name),
            setting.name
        )
        .ok();
    }
    s.push('\n');
}

pub(crate) fn write_hook_state(s: &mut String, surface: &Surface, view: &ViewList) {
    if let Some(sort) = &view.sort {
        write_sort_state(s, sort);
    }

    if let Some(selection) = &view.selection {
        write_selection_state(s, selection, &surface.feature);
    }

    if !view.settings.is_empty() {
        write_settings_state(s, &view.settings);
    }
}

pub(crate) fn write_return_fields(s: &mut String, view: &ViewList) {
    if view.sort.is_some() {
        // `SortField` is emitted by `write_sort_state`. Annotating `field`
        // and `dir` here keeps the closure free of implicit-any (TS7006) and
        // makes `dir` a proper `"asc" | "desc"` union instead of narrowing to
        // the literal `"desc"` default (TS2322).
        writeln!(
            s,
            "    sort: {{ field: sort.field, dir: sort.dir, set: (field: SortField, dir: \"asc\" | \"desc\" = \"desc\") => setSort({{ field, dir }}) }},"
        )
        .ok();
    }

    if let Some(selection) = &view.selection {
        write_selection_return(s, selection);
    }

    if !view.settings.is_empty() {
        let parts: Vec<String> = view
            .settings
            .iter()
            .flat_map(|setting| {
                let camel = lower_camel(&setting.name);
                let setter = format!("set{}", pascal_case(&setting.name));
                [camel, setter]
            })
            .collect();
        writeln!(s, "    settings: {{ {} }},", parts.join(", ")).ok();
    }
}

/// Bulk action SDK ident is just the canonical command_ident — the .lzi
/// command (whatever its name) is what gets imported. Doctor D.6 enforces
/// the input shape (`{ ids: ID[] }` or `{ <feature>_ids: ID[] }`).
pub(crate) fn bulk_command_ident(cmd: &CommandRef) -> String {
    command_ident(cmd)
}

fn write_sort_state(s: &mut String, sort: &SortDecl) {
    // Emit a named `SortField` literal-union alias so the `sort.set` closure
    // in `write_return_fields` can annotate its `field` parameter — without
    // it, `field` defaults to `any` (TS7006).
    let union = string_union(&sort.allowed);
    writeln!(s, "  type SortField = {};", union).ok();
    writeln!(
        s,
        "  const [sort, setSort] = useState<{{ field: SortField; dir: \"asc\" | \"desc\" }}>({{",
    )
    .ok();
    writeln!(
        s,
        "    field: \"{}\", dir: \"{}\",",
        sort.default_field,
        sort_dir_literal(sort.default_dir)
    )
    .ok();
    writeln!(s, "  }});").ok();
}

fn write_selection_state(s: &mut String, selection: &SelectionDecl, feature: &str) {
    match selection.mode {
        SelectionMode::None => {}
        SelectionMode::Single => {
            writeln!(
                s,
                "  const [selectionId, setSelectionId] = useState<string | null>(null);"
            )
            .ok();
        }
        SelectionMode::Multi => {
            // Thread the SDK resource's ID type via indexed-access — `Item["id"]`
            // — instead of hardcoding `string`. The resource interface (emitted
            // by the per-feature SDK) owns the truth: callers that have number
            // ids no longer need defensive `String()/Number()` casts.
            writeln!(
                s,
                "  const selection = useMultiSelection<{}[\"id\"]>(query.data ?? []);",
                pascal_case(feature)
            )
            .ok();
            for cmd in &selection.bulk_actions {
                let bind = bulk_binding_name(cmd);
                let ident = bulk_command_ident(&CommandRef {
                    feature: if cmd.feature.is_empty() {
                        feature.to_owned()
                    } else {
                        cmd.feature.clone()
                    },
                    name: cmd.name.clone(),
                });
                writeln!(s, "  const {} = useLazuliCommand({});", bind, ident).ok();
            }
        }
    }
}

fn write_settings_state(s: &mut String, settings: &[SettingDecl]) {
    for setting in settings {
        let camel = lower_camel(&setting.name);
        let setter = format!("set{}", pascal_case(&setting.name));
        let key = format!("SETTING_KEY_{}", upper_snake(&setting.name));
        let ty = setting_ts_type(setting);
        let default = setting_default_literal(setting);

        match setting.persistence {
            SettingPersistence::None => {
                writeln!(
                    s,
                    "  const [{}, {}] = useState<{}>({});",
                    camel, setter, ty, default
                )
                .ok();
            }
            SettingPersistence::Local => {
                writeln!(
                    s,
                    "  const [{}, {}] = useLocalSetting<{}>({}, {});",
                    camel, setter, ty, key, default
                )
                .ok();
            }
            SettingPersistence::Workspace => {
                writeln!(s, "  // TODO: workspace persistence pending L0 #7").ok();
                writeln!(
                    s,
                    "  const [{}, {}] = useLocalSetting<{}>({}, {});",
                    camel, setter, ty, key, default
                )
                .ok();
            }
        }
    }
}

fn write_selection_return(s: &mut String, selection: &SelectionDecl) {
    match selection.mode {
        SelectionMode::None => {}
        SelectionMode::Single => {
            writeln!(s, "    selection: {{").ok();
            writeln!(s, "      mode: \"single\",").ok();
            writeln!(s, "      id: selectionId,").ok();
            writeln!(s, "      set: setSelectionId,").ok();
            writeln!(s, "      clear: () => setSelectionId(null),").ok();
            writeln!(s, "    }},").ok();
        }
        SelectionMode::Multi => {
            writeln!(s, "    selection: {{").ok();
            writeln!(s, "      mode: \"multi\",").ok();
            writeln!(s, "      ids: selection.ids,").ok();
            writeln!(s, "      has: selection.has,").ok();
            writeln!(s, "      toggle: selection.toggle,").ok();
            writeln!(s, "      selectRange: selection.selectRange,").ok();
            writeln!(s, "      clear: selection.clear,").ok();
            if selection.bulk_actions.is_empty() {
                writeln!(s, "      bulk: {{}},").ok();
            } else {
                let parts: Vec<String> = selection
                    .bulk_actions
                    .iter()
                    .map(|cmd| format!("{}: {}", bulk_return_key(cmd), bulk_binding_name(cmd)))
                    .collect();
                writeln!(s, "      bulk: {{ {} }},", parts.join(", ")).ok();
            }
            writeln!(s, "    }},").ok();
        }
    }
}

fn setting_ts_type(setting: &SettingDecl) -> String {
    match &setting.value_space {
        SettingValueSpace::Enum { values } => string_union(values),
        SettingValueSpace::Bool => "boolean".to_owned(),
        SettingValueSpace::Int { .. } => "number".to_owned(),
    }
}

fn setting_default_literal(setting: &SettingDecl) -> String {
    match setting.value_space {
        SettingValueSpace::Enum { .. } => format!("\"{}\"", setting.default),
        SettingValueSpace::Bool | SettingValueSpace::Int { .. } => setting.default.clone(),
    }
}

fn string_union(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{}\"", value))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn sort_dir_literal(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc => "asc",
        SortDir::Desc => "desc",
    }
}

fn bulk_binding_name(cmd: &CommandRef) -> String {
    format!("bulk{}", pascal_case(&cmd.name))
}

/// Return-object key inside `selection.bulk` — the SHORT action name with
/// the `bulk_` prefix stripped if present, so callers write
/// `selection.bulk.delete(items)` not `selection.bulk.bulkDelete(items)`.
/// The SDK import identifier still preserves the full command name (see
/// `bulk_command_ident`); only the consumer-facing key drops the prefix.
fn bulk_return_key(cmd: &CommandRef) -> String {
    let short = cmd.name.strip_prefix("bulk_").unwrap_or(&cmd.name);
    lower_camel(short)
}

fn upper_snake(value: &str) -> String {
    value
        .split(|c: char| c == '-' || c == '_' || c == ' ')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn kebab_case(value: &str) -> String {
    value
        .split(|c: char| c == '-' || c == '_' || c == ' ')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut out = String::with_capacity(value.len());
            for c in first.to_lowercase() {
                out.push(c);
            }
            out.push_str(chars.as_str());
            out
        }
    }
}

pub(crate) fn unique_bulk_command_imports(view: &ViewList) -> Vec<String> {
    bulk_actions(view)
        .into_iter()
        .map(bulk_command_ident)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
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
    // Bug-fix regression tests (Pleiades dogfood, 2026-05-14).
    // -----------------------------------------------------------------

    #[test]
    fn bulk_action_key_strips_command_prefix() {
        // Bug #1: `selection.bulk.bulkDelete` instead of
        // `selection.bulk.delete`. The bulk command was named
        // `bulk_delete` upstream — Pleiades' real-world shape — and the
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
        write_setting_keys(&mut out, &surface(), &view, "pleiades");
        assert!(
            out.contains(
                "const SETTING_KEY_GRID_SIZE = \"pleiades:item-terminal:grid_size\";"
            ),
            "expected `pleiades:` prefix; got: {out}"
        );
        // No legacy feature-scoped prefix.
        assert!(!out.contains("\"item:item-terminal:"));
    }
}
