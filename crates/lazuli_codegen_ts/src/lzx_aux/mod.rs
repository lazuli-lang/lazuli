//! Auxiliary `view list` state emitters for L0 #6 terminal grammar:
//! sort, selection, and settings. These functions only write TypeScript
//! wire code around React state and runtime helper hooks.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::lzx::{
    CommandRef, SelectionDecl, SelectionMode, SettingDecl, SettingPersistence, SettingValueSpace,
    SortDecl, SortDir, Surface, ViewList, command_ident, lower_camel, pascal_case,
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
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn kebab_case(value: &str) -> String {
    value
        .split(['-', '_', ' '])
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
mod tests;
