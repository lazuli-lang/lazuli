//! Auxiliary `view list` state emitters for L0 #6 terminal grammar:
//! sort, selection, and settings. These functions only write TypeScript
//! wire code around React state and runtime helper hooks.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::lzx::{
    command_action_key, lower_camel, pascal_case, CommandRef, SelectionDecl, SelectionMode,
    SettingDecl, SettingPersistence, SettingValueSpace, SortDecl, SortDir, Surface, ViewList,
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

pub(crate) fn write_setting_keys(s: &mut String, surface: &Surface, view: &ViewList) {
    if view.settings.is_empty() {
        return;
    }

    for setting in &view.settings {
        writeln!(
            s,
            "const SETTING_KEY_{} = \"{}:{}:{}\";",
            upper_snake(&setting.name),
            surface.feature,
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
        writeln!(
            s,
            "    sort: {{ field: sort.field, dir: sort.dir, set: (field, dir = \"desc\") => setSort({{ field, dir }}) }},"
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

pub(crate) fn bulk_command_ident(cmd: &CommandRef) -> String {
    let resource_plural = format!("{}s", pascal_case(&cmd.feature));
    let mut out = String::from("bulk");
    out.push_str(&pascal_case(&cmd.name));
    out.push_str(&resource_plural);
    lower_first(&out)
}

fn write_sort_state(s: &mut String, sort: &SortDecl) {
    let union = string_union(&sort.allowed);
    writeln!(
        s,
        "  const [sort, setSort] = useState<{{ field: {}; dir: \"asc\" | \"desc\" }}>({{",
        union
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
            writeln!(
                s,
                "  const selection = useMultiSelection<string>(query.data ?? []);"
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
                    .map(|cmd| format!("{}: {}", command_action_key(cmd), bulk_binding_name(cmd)))
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
        assert!(
            state.contains("useState<{ field: \"title\" | \"updated\"; dir: \"asc\" | \"desc\" }>")
        );
        assert!(state.contains("field: \"updated\", dir: \"desc\""));

        let mut ret = String::new();
        write_return_fields(&mut ret, &view);
        assert!(ret.contains("sort: { field: sort.field, dir: sort.dir, set: (field, dir = \"desc\") => setSort({ field, dir }) }"));
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
        assert!(state.contains("const selection = useMultiSelection<string>(query.data ?? []);"));
        assert!(state.contains("const bulkDelete = useLazuliCommand(bulkDeleteItems);"));

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
        write_setting_keys(&mut out, &surface(), &view);
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
        assert_eq!(unique_bulk_command_imports(&view), vec!["bulkDeleteItems"]);
    }
}
